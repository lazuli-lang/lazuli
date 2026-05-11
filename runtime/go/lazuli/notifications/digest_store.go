// Package notifications — digest store surface for the
// `notification.digest` sub-block. The dispatcher accumulates per-
// trigger payloads keyed on `(notification, group_by_value)` and
// flushes the bucket either when the time window expires
// (`NotificationDigest.Every`) or when the cap is hit
// (`NotificationDigest.MaxSize`, surfaced as `ErrDigestFull`).
//
// Two implementations ship with the Lazuli runtime core:
//
//   - `MemoryDigestStore` — in-process, single-instance. Used for
//     unit tests and single-pod deployments. Safe for `testing/synctest`.
//   - real distributed stores (Redis, Postgres) live in `@runtime/...`
//     adapter packages and bind via `@adapter.notification.digest.<store>`
//     in `registry.lzi`. No provider names appear in core.
//
// Notifications expanded bucket cycle — language-side stub.
package notifications

import (
	"context"
	"sync"
	"time"
)

// DigestStore is the adapter contract the dispatcher consults to
// aggregate triggers within a digest window. Implementations decide
// the persistence model; the language only requires the bucket key
// to be `(notification, group_by_value)`.
type DigestStore interface {
	// Add appends a payload to the bucket keyed on the notification
	// and the group_by value. Returns the post-add size and a
	// boolean indicating whether the caller should flush immediately
	// (e.g. because the cap was hit). When `ErrDigestFull` is
	// returned, the caller MUST call `Flush` before any subsequent
	// `Add` for the same key.
	Add(
		ctx context.Context,
		notification string,
		groupByValue string,
		payload map[string]any,
	) (size uint32, full bool, err error)

	// Flush returns and clears the buffered payloads for the given
	// key. The dispatcher calls Flush on window expiry or on
	// `ErrDigestFull`.
	Flush(
		ctx context.Context,
		notification string,
		groupByValue string,
	) ([]map[string]any, error)

	// PendingKeys returns every `(notification, group_by_value)`
	// tuple currently holding buffered payloads. The dispatcher uses
	// this to drive the flush loop without polling every possible key.
	PendingKeys(ctx context.Context) ([]DigestKey, error)
}

// DigestKey is the bucket address for `DigestStore`.
type DigestKey struct {
	Notification string
	GroupByValue string
	// OpenedAt is when the first payload landed in the bucket; the
	// dispatcher flushes when `time.Since(OpenedAt) >= Every`.
	OpenedAt time.Time
}

// MemoryDigestStore is the in-process reference implementation.
// Safe for concurrent use; safe for `testing/synctest`. Production
// deployments swap this for an adapter-backed store via
// `@adapter.notification.digest.<store>` registry binding.
type MemoryDigestStore struct {
	mu      sync.Mutex
	buckets map[memoryBucketKey]*memoryBucket
	maxSize uint32
}

type memoryBucketKey struct {
	notification string
	groupByValue string
}

type memoryBucket struct {
	openedAt time.Time
	payloads []map[string]any
}

// NewMemoryDigestStore returns an in-process digest store with the
// supplied cap. Pass 0 for no cap; the dispatcher then relies solely
// on `Every` to flush.
func NewMemoryDigestStore(maxSize uint32) *MemoryDigestStore {
	return &MemoryDigestStore{
		buckets: make(map[memoryBucketKey]*memoryBucket),
		maxSize: maxSize,
	}
}

// Add implements DigestStore.
func (m *MemoryDigestStore) Add(
	_ context.Context,
	notification string,
	groupByValue string,
	payload map[string]any,
) (uint32, bool, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	key := memoryBucketKey{notification: notification, groupByValue: groupByValue}
	bucket, ok := m.buckets[key]
	if !ok {
		bucket = &memoryBucket{openedAt: time.Now()}
		m.buckets[key] = bucket
	}
	bucket.payloads = append(bucket.payloads, payload)
	size := uint32(len(bucket.payloads))
	full := m.maxSize > 0 && size >= m.maxSize
	if full {
		return size, true, ErrDigestFull
	}
	return size, false, nil
}

// Flush implements DigestStore.
func (m *MemoryDigestStore) Flush(
	_ context.Context,
	notification string,
	groupByValue string,
) ([]map[string]any, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	key := memoryBucketKey{notification: notification, groupByValue: groupByValue}
	bucket, ok := m.buckets[key]
	if !ok {
		return nil, nil
	}
	delete(m.buckets, key)
	return bucket.payloads, nil
}

// PendingKeys implements DigestStore.
func (m *MemoryDigestStore) PendingKeys(_ context.Context) ([]DigestKey, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	keys := make([]DigestKey, 0, len(m.buckets))
	for k, b := range m.buckets {
		keys = append(keys, DigestKey{
			Notification: k.notification,
			GroupByValue: k.groupByValue,
			OpenedAt:     b.openedAt,
		})
	}
	return keys, nil
}
