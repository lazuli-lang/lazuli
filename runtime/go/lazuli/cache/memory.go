package cache

import (
	"context"
	"strings"
	"sync"
	"time"
)

const memoryDefaultTTL = time.Minute

// MemoryBackendConfig configures an in-process memory cache backend.
type MemoryBackendConfig struct {
	// DefaultTTL is used when Put receives a zero TTL. A zero value uses
	// the runtime default. A negative value disables expiry for zero-TTL puts.
	DefaultTTL time.Duration
}

// MemoryBackendOption configures a MemoryBackend.
type MemoryBackendOption func(*MemoryBackendConfig)

// WithMemoryDefaultTTL sets the TTL used when Put receives a zero TTL.
func WithMemoryDefaultTTL(ttl time.Duration) MemoryBackendOption {
	return func(config *MemoryBackendConfig) {
		config.DefaultTTL = ttl
	}
}

// MemoryBackend is an in-process cache backend.
type MemoryBackend struct {
	mu sync.RWMutex

	entries map[string]memoryEntry
	tags    map[string]map[string]struct{}

	defaultTTL time.Duration
	now        func() time.Time

	hits   uint64
	misses uint64
	evicts uint64
}

type memoryEntry struct {
	value     []byte
	tags      []string
	expiresAt time.Time
}

var _ Backend = (*MemoryBackend)(nil)

// NewMemoryBackend returns an empty in-process cache backend.
func NewMemoryBackend(options ...MemoryBackendOption) *MemoryBackend {
	config := MemoryBackendConfig{DefaultTTL: memoryDefaultTTL}
	for _, option := range options {
		if option != nil {
			option(&config)
		}
	}
	if config.DefaultTTL == 0 {
		config.DefaultTTL = memoryDefaultTTL
	}

	return &MemoryBackend{
		entries:    make(map[string]memoryEntry),
		tags:       make(map[string]map[string]struct{}),
		defaultTTL: config.DefaultTTL,
		now:        time.Now,
	}
}

// Get returns a defensive copy of the cached value for key.
func (b *MemoryBackend) Get(ctx context.Context, key string) ([]byte, bool, error) {
	if err := ctx.Err(); err != nil {
		return nil, false, err
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	entry, ok := b.entries[key]
	if !ok {
		b.misses++
		return nil, false, nil
	}
	if b.isExpired(entry) {
		b.deleteLocked(key)
		b.misses++
		b.evicts++
		return nil, false, nil
	}

	b.hits++
	return cloneBytes(entry.value), true, nil
}

// Put stores a defensive copy of value under key.
func (b *MemoryBackend) Put(ctx context.Context, key string, value []byte, ttl time.Duration, tags []string) error {
	if err := ctx.Err(); err != nil {
		return err
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	if _, ok := b.entries[key]; ok {
		b.deleteLocked(key)
	}

	entry := memoryEntry{
		value:     cloneBytes(value),
		tags:      cleanTags(tags),
		expiresAt: b.expiresAt(ttl),
	}
	b.entries[key] = entry
	for _, tag := range entry.tags {
		keys := b.tags[tag]
		if keys == nil {
			keys = make(map[string]struct{})
			b.tags[tag] = keys
		}
		keys[key] = struct{}{}
	}
	return nil
}

// InvalidateQueries deletes entries whose keys start with each query name
// followed by "|".
func (b *MemoryBackend) InvalidateQueries(ctx context.Context, names []string) (int, error) {
	if err := ctx.Err(); err != nil {
		return 0, err
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	b.pruneExpiredLocked()
	var deleted int
	for _, name := range names {
		if name == "" {
			continue
		}
		prefix := name + "|"
		for key := range b.entries {
			if strings.HasPrefix(key, prefix) {
				b.deleteLocked(key)
				deleted++
				b.evicts++
			}
		}
	}
	return deleted, nil
}

// InvalidateTags deletes entries associated with any of labels.
func (b *MemoryBackend) InvalidateTags(ctx context.Context, labels []string) (int, error) {
	if err := ctx.Err(); err != nil {
		return 0, err
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	b.pruneExpiredLocked()
	var deleted int
	for _, label := range labels {
		if label == "" {
			continue
		}
		keys := b.tags[label]
		for key := range keys {
			if _, ok := b.entries[key]; ok {
				b.deleteLocked(key)
				deleted++
				b.evicts++
			}
		}
		delete(b.tags, label)
	}
	return deleted, nil
}

// Stats returns a point-in-time snapshot for this backend instance.
func (b *MemoryBackend) Stats(ctx context.Context) (QueryStats, error) {
	if err := ctx.Err(); err != nil {
		return QueryStats{}, err
	}

	b.mu.Lock()
	defer b.mu.Unlock()

	b.pruneExpiredLocked()
	return QueryStats{
		Entries: uint64(len(b.entries)),
		Hits:    b.hits,
		Misses:  b.misses,
		Evicts:  b.evicts,
	}, nil
}

func (b *MemoryBackend) expiresAt(ttl time.Duration) time.Time {
	switch {
	case ttl < 0:
		return time.Time{}
	case ttl == 0:
		if b.defaultTTL < 0 {
			return time.Time{}
		}
		return b.now().Add(b.defaultTTL)
	default:
		return b.now().Add(ttl)
	}
}

func (b *MemoryBackend) isExpired(entry memoryEntry) bool {
	return !entry.expiresAt.IsZero() && !b.now().Before(entry.expiresAt)
}

func (b *MemoryBackend) pruneExpiredLocked() {
	for key, entry := range b.entries {
		if b.isExpired(entry) {
			b.deleteLocked(key)
			b.evicts++
		}
	}
}

func (b *MemoryBackend) deleteLocked(key string) {
	entry, ok := b.entries[key]
	if !ok {
		return
	}
	delete(b.entries, key)
	for _, tag := range entry.tags {
		keys := b.tags[tag]
		if keys == nil {
			continue
		}
		delete(keys, key)
		if len(keys) == 0 {
			delete(b.tags, tag)
		}
	}
}

func cleanTags(tags []string) []string {
	if len(tags) == 0 {
		return nil
	}

	seen := make(map[string]struct{}, len(tags))
	cleaned := make([]string, 0, len(tags))
	for _, tag := range tags {
		if tag == "" {
			continue
		}
		if _, ok := seen[tag]; ok {
			continue
		}
		seen[tag] = struct{}{}
		cleaned = append(cleaned, tag)
	}
	return cleaned
}

func cloneBytes(value []byte) []byte {
	if value == nil {
		return nil
	}
	return append([]byte(nil), value...)
}
