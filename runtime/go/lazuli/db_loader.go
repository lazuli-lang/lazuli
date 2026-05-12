package lazuli

import (
	"context"
	"errors"
	"sync"
)

var (
	errNilDBLoader          = errors.New("lazuli: nil db loader")
	errNilDBLoaderBatchFunc = errors.New("lazuli: nil db loader batch function")
)

// DBLoaderBatchFunc loads values for a de-duplicated set of missing keys.
//
// Batch functions should return values keyed by the original key. Keys omitted
// from the returned map are cached as misses for the lifetime of the loader.
type DBLoaderBatchFunc[K comparable, V any] func(context.Context, []K) (map[K]V, error)

// DBLoader is a request-scoped keyed loader for generated repositories.
//
// Create one loader set per request, command, query, job, or webhook execution.
// The cache is intentionally in-memory and local to this loader; it is not a
// process-global query cache. The loader cache is safe for concurrent use.
// Batch functions may be invoked concurrently by concurrent cache misses.
type DBLoader[K comparable, V any] struct {
	batch DBLoaderBatchFunc[K, V]

	mu    sync.Mutex
	cache map[K]dbLoaderEntry[V]
}

type dbLoaderEntry[V any] struct {
	value V
	ok    bool
}

// NewDBLoader returns a request-scoped loader backed by batch.
func NewDBLoader[K comparable, V any](batch DBLoaderBatchFunc[K, V]) *DBLoader[K, V] {
	return &DBLoader[K, V]{
		batch: batch,
		cache: make(map[K]dbLoaderEntry[V]),
	}
}

// Load resolves one key, returning ok=false when the batch function did not
// return a value for key.
func (l *DBLoader[K, V]) Load(ctx context.Context, key K) (V, bool, error) {
	values, err := l.LoadMany(ctx, key)
	if err != nil {
		var zero V
		return zero, false, err
	}
	value, ok := values[key]
	return value, ok, nil
}

// LoadMany resolves keys in one batch for cache misses.
//
// Keys already cached by this loader are returned without calling the batch
// function. Missing keys are de-duplicated in first-seen order before the batch
// function is called. The returned map contains only loaded values; cached
// misses remain absent.
func (l *DBLoader[K, V]) LoadMany(ctx context.Context, keys ...K) (map[K]V, error) {
	if l == nil {
		return nil, errNilDBLoader
	}
	ctx = dbLoaderContext(ctx)
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	values := make(map[K]V, len(keys))
	if len(keys) == 0 {
		return values, nil
	}

	missing := l.readDBLoaderCache(keys, values)
	if len(missing) == 0 {
		return values, nil
	}
	if l.batch == nil {
		return nil, errNilDBLoaderBatchFunc
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	loaded, err := l.batch(ctx, append([]K(nil), missing...))
	if err != nil {
		return nil, err
	}
	if err := ctx.Err(); err != nil {
		return nil, err
	}

	l.writeDBLoaderCache(missing, loaded, values)
	return values, nil
}

// Preload loads and caches keys without returning the loaded values.
func (l *DBLoader[K, V]) Preload(ctx context.Context, keys ...K) error {
	_, err := l.LoadMany(ctx, keys...)
	return err
}

// Prime stores value for key in this loader's request cache.
func (l *DBLoader[K, V]) Prime(key K, value V) *DBLoader[K, V] {
	if l == nil {
		return nil
	}

	l.mu.Lock()
	defer l.mu.Unlock()

	l.ensureDBLoaderCacheLocked()
	l.cache[key] = dbLoaderEntry[V]{value: value, ok: true}
	return l
}

// Clear removes key from this loader's request cache.
func (l *DBLoader[K, V]) Clear(key K) *DBLoader[K, V] {
	if l == nil {
		return nil
	}

	l.mu.Lock()
	defer l.mu.Unlock()

	delete(l.cache, key)
	return l
}

// ClearAll removes every entry from this loader's request cache.
func (l *DBLoader[K, V]) ClearAll() *DBLoader[K, V] {
	if l == nil {
		return nil
	}

	l.mu.Lock()
	defer l.mu.Unlock()

	l.cache = make(map[K]dbLoaderEntry[V])
	return l
}

func (l *DBLoader[K, V]) readDBLoaderCache(keys []K, values map[K]V) []K {
	l.mu.Lock()
	defer l.mu.Unlock()

	seen := make(map[K]struct{}, len(keys))
	missing := make([]K, 0, len(keys))
	for _, key := range keys {
		if _, ok := seen[key]; ok {
			continue
		}
		seen[key] = struct{}{}

		entry, cached := l.cache[key]
		if !cached {
			missing = append(missing, key)
			continue
		}
		if entry.ok {
			values[key] = entry.value
		}
	}
	return missing
}

func (l *DBLoader[K, V]) writeDBLoaderCache(keys []K, loaded map[K]V, values map[K]V) {
	l.mu.Lock()
	defer l.mu.Unlock()

	l.ensureDBLoaderCacheLocked()
	for _, key := range keys {
		if entry, cached := l.cache[key]; cached {
			if entry.ok {
				values[key] = entry.value
				continue
			}
		}

		value, ok := loaded[key]
		l.cache[key] = dbLoaderEntry[V]{value: value, ok: ok}
		if ok {
			values[key] = value
		}
	}
}

func (l *DBLoader[K, V]) ensureDBLoaderCacheLocked() {
	if l.cache == nil {
		l.cache = make(map[K]dbLoaderEntry[V])
	}
}

func dbLoaderContext(ctx context.Context) context.Context {
	if ctx == nil {
		return context.Background()
	}
	return ctx
}
