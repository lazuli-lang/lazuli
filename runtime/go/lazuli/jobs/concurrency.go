package jobs

import (
	"context"
	"errors"
	"sync"
)

// ConcurrencyLimiter gates concurrent job work independently per key.
//
// The zero value is ready to use and allows one concurrent holder per key.
// Use NewConcurrencyLimiter to set a different default limit or per-key
// overrides.
type ConcurrencyLimiter struct {
	mu           sync.Mutex
	defaultLimit int
	keyLimits    map[string]int
	buckets      map[string]*concurrencyBucket
}

type concurrencyBucket struct {
	sem  chan struct{}
	refs int
}

// NewConcurrencyLimiter returns a keyed concurrency limiter.
//
// defaultLimit applies to keys not present in keyLimits. Values less than one
// are normalized to one. keyLimits is copied, so callers may reuse or mutate the
// input map after construction.
func NewConcurrencyLimiter(defaultLimit int, keyLimits map[string]int) *ConcurrencyLimiter {
	limiter := &ConcurrencyLimiter{
		defaultLimit: normalizeConcurrencyLimit(defaultLimit),
	}
	if len(keyLimits) > 0 {
		limiter.keyLimits = make(map[string]int, len(keyLimits))
		for key, limit := range keyLimits {
			limiter.keyLimits[key] = normalizeConcurrencyLimit(limit)
		}
	}
	return limiter
}

// Acquire reserves one concurrency slot for key, blocking until a slot is
// available or ctx is canceled.
//
// Call Release with the same key when the protected work completes. A nil
// context is treated as context.Background().
func (l *ConcurrencyLimiter) Acquire(ctx context.Context, key string) error {
	if ctx == nil {
		ctx = context.Background()
	}
	if err := ctx.Err(); err != nil {
		return err
	}

	bucket := l.bucketFor(key)
	select {
	case bucket.sem <- struct{}{}:
		return nil
	case <-ctx.Done():
		l.releaseRef(key, bucket)
		return ctx.Err()
	}
}

// Release returns one concurrency slot for key.
//
// Calling Release for a key with no active slot is a no-op.
func (l *ConcurrencyLimiter) Release(key string) {
	l.mu.Lock()
	bucket := l.buckets[key]
	l.mu.Unlock()
	if bucket == nil {
		return
	}

	select {
	case <-bucket.sem:
		l.releaseRef(key, bucket)
	default:
	}
}

// Run acquires a concurrency slot for key, runs fn, and releases the slot.
//
// The slot is released when fn returns an error and also when fn panics; panics
// are rethrown after releasing. A nil context is treated as context.Background().
func (l *ConcurrencyLimiter) Run(ctx context.Context, key string, fn func(context.Context) error) error {
	if fn == nil {
		return errors.New("jobs: concurrency limiter function is nil")
	}
	if ctx == nil {
		ctx = context.Background()
	}
	if err := l.Acquire(ctx, key); err != nil {
		return err
	}
	defer l.Release(key)

	return fn(ctx)
}

func (l *ConcurrencyLimiter) bucketFor(key string) *concurrencyBucket {
	l.mu.Lock()
	defer l.mu.Unlock()

	if l.buckets == nil {
		l.buckets = make(map[string]*concurrencyBucket)
	}
	if bucket := l.buckets[key]; bucket != nil {
		bucket.refs++
		return bucket
	}

	bucket := &concurrencyBucket{
		sem:  make(chan struct{}, l.limitForLocked(key)),
		refs: 1,
	}
	l.buckets[key] = bucket
	return bucket
}

func (l *ConcurrencyLimiter) releaseRef(key string, bucket *concurrencyBucket) {
	l.mu.Lock()
	defer l.mu.Unlock()

	bucket.refs--
	if bucket.refs == 0 && l.buckets[key] == bucket {
		delete(l.buckets, key)
	}
}

func (l *ConcurrencyLimiter) limitForLocked(key string) int {
	if limit, ok := l.keyLimits[key]; ok {
		return normalizeConcurrencyLimit(limit)
	}
	return normalizeConcurrencyLimit(l.defaultLimit)
}

func normalizeConcurrencyLimit(limit int) int {
	if limit < 1 {
		return 1
	}
	return limit
}
