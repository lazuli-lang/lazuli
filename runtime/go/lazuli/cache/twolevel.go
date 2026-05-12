package cache

import (
	"context"
	"errors"
	"fmt"
	"time"
)

const twoLevelBackfillTag = "lazuli:cache:twolevel:backfill"

var errTwoLevelBackendNotConfigured = errors.New("lazuli/cache: two-level backend requires local and remote backends")

// TwoLevelBackend composes a fast local cache with a durable remote cache.
//
// Reads check Local first, then Remote. Remote hits are written back to Local
// with BackfillTTL so subsequent reads stay in-process. Writes and invalidations
// are sent to both layers.
type TwoLevelBackend struct {
	// Local is the first-level backend checked before Remote.
	Local Backend

	// Remote is the second-level backend checked after a local miss.
	Remote Backend

	// BackfillTTL is used when a remote hit is written back to Local.
	BackfillTTL time.Duration
}

var _ Backend = (*TwoLevelBackend)(nil)

// NewTwoLevelBackend returns a backend that reads through Local and Remote.
func NewTwoLevelBackend(local, remote Backend, backfillTTL time.Duration) *TwoLevelBackend {
	return &TwoLevelBackend{
		Local:       local,
		Remote:      remote,
		BackfillTTL: backfillTTL,
	}
}

func (b *TwoLevelBackend) Get(ctx context.Context, key string) ([]byte, bool, error) {
	local, remote, err := b.backends()
	if err != nil {
		return nil, false, err
	}

	value, hit, err := local.Get(ctx, key)
	if err != nil {
		return nil, false, fmt.Errorf("local get: %w", err)
	}
	if hit {
		return value, true, nil
	}

	value, hit, err = remote.Get(ctx, key)
	if err != nil {
		return nil, false, fmt.Errorf("remote get: %w", err)
	}
	if !hit {
		return nil, false, nil
	}

	if err := local.Put(ctx, key, value, b.BackfillTTL, []string{twoLevelBackfillTag}); err != nil {
		return value, true, fmt.Errorf("local backfill: %w", err)
	}
	return value, true, nil
}

func (b *TwoLevelBackend) Put(ctx context.Context, key string, value []byte, ttl time.Duration, tags []string) error {
	local, remote, err := b.backends()
	if err != nil {
		return err
	}

	localErr := local.Put(ctx, key, value, ttl, tags)
	remoteErr := remote.Put(ctx, key, value, ttl, tags)
	return joinLayerErrors("local put", localErr, "remote put", remoteErr)
}

func (b *TwoLevelBackend) InvalidateQueries(ctx context.Context, names []string) (int, error) {
	local, remote, err := b.backends()
	if err != nil {
		return 0, err
	}

	localDeleted, localErr := local.InvalidateQueries(ctx, names)
	remoteDeleted, remoteErr := remote.InvalidateQueries(ctx, names)
	return localDeleted + remoteDeleted, joinLayerErrors("local invalidate queries", localErr, "remote invalidate queries", remoteErr)
}

func (b *TwoLevelBackend) InvalidateTags(ctx context.Context, labels []string) (int, error) {
	local, remote, err := b.backends()
	if err != nil {
		return 0, err
	}

	localLabels := labels
	if hasNonEmptyLabel(labels) {
		localLabels = make([]string, 0, len(labels)+1)
		localLabels = append(localLabels, labels...)
		localLabels = append(localLabels, twoLevelBackfillTag)
	}

	localDeleted, localErr := local.InvalidateTags(ctx, localLabels)
	remoteDeleted, remoteErr := remote.InvalidateTags(ctx, labels)
	return localDeleted + remoteDeleted, joinLayerErrors("local invalidate tags", localErr, "remote invalidate tags", remoteErr)
}

func (b *TwoLevelBackend) Stats(ctx context.Context) (QueryStats, error) {
	local, remote, err := b.backends()
	if err != nil {
		return QueryStats{}, err
	}

	localStats, localErr := local.Stats(ctx)
	remoteStats, remoteErr := remote.Stats(ctx)
	return addQueryStats(localStats, remoteStats), joinLayerErrors("local stats", localErr, "remote stats", remoteErr)
}

func (b *TwoLevelBackend) backends() (Backend, Backend, error) {
	if b == nil || b.Local == nil || b.Remote == nil {
		return nil, nil, errTwoLevelBackendNotConfigured
	}
	return b.Local, b.Remote, nil
}

func addQueryStats(a, b QueryStats) QueryStats {
	return QueryStats{
		Entries: a.Entries + b.Entries,
		Hits:    a.Hits + b.Hits,
		Misses:  a.Misses + b.Misses,
		Evicts:  a.Evicts + b.Evicts,
	}
}

func joinLayerErrors(firstLabel string, firstErr error, secondLabel string, secondErr error) error {
	var errs []error
	if firstErr != nil {
		errs = append(errs, fmt.Errorf("%s: %w", firstLabel, firstErr))
	}
	if secondErr != nil {
		errs = append(errs, fmt.Errorf("%s: %w", secondLabel, secondErr))
	}
	return errors.Join(errs...)
}

func hasNonEmptyLabel(labels []string) bool {
	for _, label := range labels {
		if label != "" {
			return true
		}
	}
	return false
}
