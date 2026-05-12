package migrations

import (
	"context"
	"errors"
	"strconv"
	"sync"
	"time"
)

const lockAcquirePollInterval = 10 * time.Millisecond

var (
	errNilLockStore = errors.New("migrations: LockStore is required")
	errNilLockFunc  = errors.New("migrations: lock callback is required")

	// ErrLockNameRequired is returned when a lock operation is called without
	// a lock name.
	ErrLockNameRequired = errors.New("migrations: lock name required")
	// ErrLockTTLRequired is returned when a lock acquisition or extension uses
	// a non-positive TTL.
	ErrLockTTLRequired = errors.New("migrations: lock ttl must be positive")
	// ErrLockHeld is returned by non-blocking Acquire calls when another owner
	// currently holds the named lock.
	ErrLockHeld = errors.New("migrations: lock held")
	// ErrLockOwnershipLost is returned when Release or Extend is called with a
	// stale, expired, or non-owner lease.
	ErrLockOwnershipLost = errors.New("migrations: lock ownership lost")
)

// LockToken is an opaque ownership token for a migration lock lease.
type LockToken string

// LockLease proves ownership of a named migration lock until ExpiresAt.
type LockLease struct {
	// Name is the provider-neutral lock name.
	Name string
	// Token is the opaque ownership token required to release or extend the
	// lease.
	Token LockToken
	// ExpiresAt is the provider's current lease expiration time.
	ExpiresAt time.Time
}

// LockStore is the provider-neutral migration lock contract.
//
// Acquire is non-blocking: implementations return ErrLockHeld when another
// live owner holds the named lock. Release and Extend must verify the ownership
// token from the lease before mutating the lock. Extend renews the same
// ownership token and returns the lease with its updated expiration.
type LockStore interface {
	Acquire(ctx context.Context, name string, ttl time.Duration) (LockLease, error)
	Release(ctx context.Context, lease LockLease) error
	Extend(ctx context.Context, lease LockLease, ttl time.Duration) (LockLease, error)
}

// MemoryLockStore stores migration locks in process memory.
//
// The zero value is ready to use. It is intended for tests, local runtimes, and
// single-process adapters; distributed providers should implement LockStore
// against their own durable or advisory lock backend.
type MemoryLockStore struct {
	mu    sync.Mutex
	locks map[string]memoryLock
	next  uint64
	now   func() time.Time
}

var _ LockStore = (*MemoryLockStore)(nil)

type memoryLock struct {
	token     LockToken
	expiresAt time.Time
}

// NewMemoryLockStore returns an empty in-memory migration lock store.
func NewMemoryLockStore() *MemoryLockStore {
	return &MemoryLockStore{}
}

// Acquire takes ownership of name for ttl and returns its lease. It returns
// ErrLockHeld when name is held by a non-expired lease.
func (s *MemoryLockStore) Acquire(ctx context.Context, name string, ttl time.Duration) (LockLease, error) {
	if err := validateLockInputs(ctx, name, ttl); err != nil {
		return LockLease{}, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	s.ensureLocked()
	now := s.nowLocked()
	if current, ok := s.locks[name]; ok && now.Before(current.expiresAt) {
		return LockLease{}, ErrLockHeld
	}

	token := s.nextTokenLocked()
	lease := LockLease{Name: name, Token: token, ExpiresAt: now.Add(ttl)}
	s.locks[name] = memoryLock{token: token, expiresAt: lease.ExpiresAt}
	return lease, nil
}

// Release releases lease when its token still owns the named lock.
func (s *MemoryLockStore) Release(ctx context.Context, lease LockLease) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if lease.Name == "" {
		return ErrLockNameRequired
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	if !s.ownsLocked(lease) {
		return ErrLockOwnershipLost
	}
	delete(s.locks, lease.Name)
	return nil
}

// Extend renews lease for ttl when its token still owns the named lock.
func (s *MemoryLockStore) Extend(ctx context.Context, lease LockLease, ttl time.Duration) (LockLease, error) {
	if err := validateLockInputs(ctx, lease.Name, ttl); err != nil {
		return LockLease{}, err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	if !s.ownsLocked(lease) {
		return LockLease{}, ErrLockOwnershipLost
	}

	lease.ExpiresAt = s.nowLocked().Add(ttl)
	s.locks[lease.Name] = memoryLock{token: lease.Token, expiresAt: lease.ExpiresAt}
	return lease, nil
}

func (s *MemoryLockStore) ensureLocked() {
	if s.locks == nil {
		s.locks = make(map[string]memoryLock)
	}
}

func (s *MemoryLockStore) nowLocked() time.Time {
	if s.now != nil {
		return s.now()
	}
	return time.Now()
}

func (s *MemoryLockStore) nextTokenLocked() LockToken {
	s.next++
	return LockToken(strconv.FormatUint(s.next, 36))
}

func (s *MemoryLockStore) ownsLocked(lease LockLease) bool {
	s.ensureLocked()
	current, ok := s.locks[lease.Name]
	if !ok {
		return false
	}
	now := s.nowLocked()
	expired := !now.Before(current.expiresAt)
	if expired || lease.Token != current.token || lease.Token == "" {
		if expired {
			delete(s.locks, lease.Name)
		}
		return false
	}
	return true
}

func validateLockInputs(ctx context.Context, name string, ttl time.Duration) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	if name == "" {
		return ErrLockNameRequired
	}
	if ttl <= 0 {
		return ErrLockTTLRequired
	}
	return nil
}

// WithLock acquires a migration lock, runs fn, and releases the lock.
//
// Acquisition is retried until the lock is available, ctx is canceled, or
// timeout elapses. A non-positive timeout performs a single acquisition attempt.
// When the helper cannot acquire the lock before timeout, it returns
// ErrMigrationLockTimeout.
func WithLock(
	ctx context.Context,
	store LockStore,
	name string,
	ttl time.Duration,
	timeout time.Duration,
	fn func(context.Context, LockLease) error,
) (err error) {
	if store == nil {
		return errNilLockStore
	}
	if fn == nil {
		return errNilLockFunc
	}

	lease, err := acquireWithTimeout(ctx, store, name, ttl, timeout)
	if err != nil {
		return err
	}
	defer func() {
		err = errors.Join(err, store.Release(context.WithoutCancel(ctx), lease))
	}()

	return fn(ctx, lease)
}

func acquireWithTimeout(ctx context.Context, store LockStore, name string, ttl, timeout time.Duration) (LockLease, error) {
	acquireCtx := ctx
	var cancel context.CancelFunc
	if timeout > 0 {
		acquireCtx, cancel = context.WithTimeout(ctx, timeout)
		defer cancel()
	}

	for {
		lease, err := store.Acquire(acquireCtx, name, ttl)
		if err == nil {
			return lease, nil
		}
		if !errors.Is(err, ErrLockHeld) {
			if acquireCtx.Err() != nil && ctx.Err() == nil {
				return LockLease{}, ErrMigrationLockTimeout
			}
			return LockLease{}, err
		}
		if timeout <= 0 {
			return LockLease{}, ErrMigrationLockTimeout
		}
		if err := waitForNextAcquireAttempt(ctx, acquireCtx); err != nil {
			return LockLease{}, err
		}
	}
}

func waitForNextAcquireAttempt(ctx, acquireCtx context.Context) error {
	timer := time.NewTimer(lockAcquirePollInterval)
	defer timer.Stop()

	select {
	case <-timer.C:
		return nil
	case <-acquireCtx.Done():
		if err := ctx.Err(); err != nil {
			return err
		}
		return ErrMigrationLockTimeout
	}
}
