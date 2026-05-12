package jobs

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestMemoryLockStoreAcquireReleaseRequiresOwner(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	store := NewMemoryLockStore()

	lease, err := store.Acquire(ctx, "jobs:daily-sync", time.Minute)
	if err != nil {
		t.Fatalf("Acquire returned %v", err)
	}
	if lease.Name != "jobs:daily-sync" {
		t.Fatalf("lease name = %q, want jobs:daily-sync", lease.Name)
	}
	if lease.Token == "" {
		t.Fatal("lease token is empty")
	}

	if _, err := store.Acquire(ctx, "jobs:daily-sync", time.Minute); !errors.Is(err, ErrLockHeld) {
		t.Fatalf("second Acquire error = %v, want ErrLockHeld", err)
	}

	wrongOwner := lease
	wrongOwner.Token = "not-owner"
	if err := store.Release(ctx, wrongOwner); !errors.Is(err, ErrLockOwnershipLost) {
		t.Fatalf("wrong-owner Release error = %v, want ErrLockOwnershipLost", err)
	}
	if _, err := store.Acquire(ctx, "jobs:daily-sync", time.Minute); !errors.Is(err, ErrLockHeld) {
		t.Fatalf("Acquire after wrong-owner Release error = %v, want ErrLockHeld", err)
	}

	if err := store.Release(ctx, lease); err != nil {
		t.Fatalf("Release returned %v", err)
	}

	next, err := store.Acquire(ctx, "jobs:daily-sync", time.Minute)
	if err != nil {
		t.Fatalf("Acquire after Release returned %v", err)
	}
	if next.Token == lease.Token {
		t.Fatal("new lease reused the previous ownership token")
	}
}

func TestMemoryLockStoreExpiresLocks(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := &MemoryLockStore{Clock: func() time.Time { return now }}

	lease, err := store.Acquire(ctx, "jobs:tenant-fanout", time.Minute)
	if err != nil {
		t.Fatalf("Acquire returned %v", err)
	}

	now = now.Add(time.Minute)
	next, err := store.Acquire(ctx, "jobs:tenant-fanout", time.Minute)
	if err != nil {
		t.Fatalf("Acquire after expiration returned %v", err)
	}
	if next.Token == lease.Token {
		t.Fatal("expired lock reacquire reused the previous ownership token")
	}
	if err := store.Release(ctx, lease); !errors.Is(err, ErrLockOwnershipLost) {
		t.Fatalf("old lease Release error = %v, want ErrLockOwnershipLost", err)
	}
}

func TestMemoryLockStoreExtendRequiresOwner(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := &MemoryLockStore{Clock: func() time.Time { return now }}

	lease, err := store.Acquire(ctx, "jobs:billing-rollup", time.Minute)
	if err != nil {
		t.Fatalf("Acquire returned %v", err)
	}
	originalExpiration := lease.ExpiresAt

	wrongOwner := lease
	wrongOwner.Token = "not-owner"
	if _, err := store.Extend(ctx, wrongOwner, 2*time.Minute); !errors.Is(err, ErrLockOwnershipLost) {
		t.Fatalf("wrong-owner Extend error = %v, want ErrLockOwnershipLost", err)
	}

	extended, err := store.Extend(ctx, lease, 2*time.Minute)
	if err != nil {
		t.Fatalf("Extend returned %v", err)
	}
	if extended.Token != lease.Token {
		t.Fatal("Extend changed the ownership token")
	}
	if !extended.ExpiresAt.Equal(now.Add(2 * time.Minute)) {
		t.Fatalf("extended expiration = %v, want %v", extended.ExpiresAt, now.Add(2*time.Minute))
	}

	now = originalExpiration.Add(time.Second)
	if _, err := store.Acquire(ctx, "jobs:billing-rollup", time.Minute); !errors.Is(err, ErrLockHeld) {
		t.Fatalf("Acquire before extended expiration error = %v, want ErrLockHeld", err)
	}

	now = extended.ExpiresAt
	if _, err := store.Acquire(ctx, "jobs:billing-rollup", time.Minute); err != nil {
		t.Fatalf("Acquire at extended expiration returned %v", err)
	}
}

func TestMemoryLockStoreRejectsInvalidInputs(t *testing.T) {
	t.Parallel()
	ctx := context.Background()
	store := NewMemoryLockStore()

	if _, err := store.Acquire(ctx, "", time.Minute); !errors.Is(err, ErrLockNameRequired) {
		t.Fatalf("Acquire empty name error = %v, want ErrLockNameRequired", err)
	}
	if _, err := store.Acquire(ctx, "jobs:sync", 0); !errors.Is(err, ErrLockTTLRequired) {
		t.Fatalf("Acquire zero ttl error = %v, want ErrLockTTLRequired", err)
	}
	if _, err := store.Extend(ctx, LockLease{Name: "jobs:sync"}, -time.Second); !errors.Is(err, ErrLockTTLRequired) {
		t.Fatalf("Extend negative ttl error = %v, want ErrLockTTLRequired", err)
	}
	if err := store.Release(ctx, LockLease{}); !errors.Is(err, ErrLockNameRequired) {
		t.Fatalf("Release empty name error = %v, want ErrLockNameRequired", err)
	}
}

func TestMemoryLockStoreZeroValueAndContext(t *testing.T) {
	t.Parallel()

	var store MemoryLockStore
	lease, err := store.Acquire(nil, "jobs:zero-value", time.Minute)
	if err != nil {
		t.Fatalf("zero-value Acquire returned %v", err)
	}
	if err := store.Release(nil, lease); err != nil {
		t.Fatalf("zero-value Release returned %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if _, err := store.Acquire(ctx, "jobs:canceled", time.Minute); !errors.Is(err, context.Canceled) {
		t.Fatalf("Acquire canceled context error = %v, want context.Canceled", err)
	}
	if _, err := store.Extend(ctx, lease, time.Minute); !errors.Is(err, context.Canceled) {
		t.Fatalf("Extend canceled context error = %v, want context.Canceled", err)
	}
	if err := store.Release(ctx, lease); !errors.Is(err, context.Canceled) {
		t.Fatalf("Release canceled context error = %v, want context.Canceled", err)
	}
}
