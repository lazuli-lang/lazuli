package migrations

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestMemoryLockStoreAcquireReleaseRequiresOwner(t *testing.T) {
	ctx := context.Background()
	store := NewMemoryLockStore()

	lease, err := store.Acquire(ctx, "deploy", time.Minute)
	if err != nil {
		t.Fatalf("Acquire returned %v", err)
	}
	if lease.Name != "deploy" {
		t.Fatalf("lease name = %q, want deploy", lease.Name)
	}
	if lease.Token == "" {
		t.Fatal("lease token is empty")
	}

	if _, err := store.Acquire(ctx, "deploy", time.Minute); !errors.Is(err, ErrLockHeld) {
		t.Fatalf("second Acquire error = %v, want ErrLockHeld", err)
	}

	wrongOwner := lease
	wrongOwner.Token = "not-owner"
	if err := store.Release(ctx, wrongOwner); !errors.Is(err, ErrLockOwnershipLost) {
		t.Fatalf("wrong-owner Release error = %v, want ErrLockOwnershipLost", err)
	}
	if _, err := store.Acquire(ctx, "deploy", time.Minute); !errors.Is(err, ErrLockHeld) {
		t.Fatalf("Acquire after wrong-owner Release error = %v, want ErrLockHeld", err)
	}

	if err := store.Release(ctx, lease); err != nil {
		t.Fatalf("Release returned %v", err)
	}

	next, err := store.Acquire(ctx, "deploy", time.Minute)
	if err != nil {
		t.Fatalf("Acquire after Release returned %v", err)
	}
	if next.Token == lease.Token {
		t.Fatal("new lease reused the previous ownership token")
	}
}

func TestMemoryLockStoreExpiresLocks(t *testing.T) {
	ctx := context.Background()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := &MemoryLockStore{now: func() time.Time { return now }}

	lease, err := store.Acquire(ctx, "deploy", time.Minute)
	if err != nil {
		t.Fatalf("Acquire returned %v", err)
	}

	now = now.Add(time.Minute)
	next, err := store.Acquire(ctx, "deploy", time.Minute)
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
	ctx := context.Background()
	now := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)
	store := &MemoryLockStore{now: func() time.Time { return now }}

	lease, err := store.Acquire(ctx, "deploy", time.Minute)
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
	if _, err := store.Acquire(ctx, "deploy", time.Minute); !errors.Is(err, ErrLockHeld) {
		t.Fatalf("Acquire before extended expiration error = %v, want ErrLockHeld", err)
	}

	now = extended.ExpiresAt
	if _, err := store.Acquire(ctx, "deploy", time.Minute); err != nil {
		t.Fatalf("Acquire at extended expiration returned %v", err)
	}
}

func TestWithLockWaitsRunsAndReleases(t *testing.T) {
	ctx := context.Background()
	store := NewMemoryLockStore()

	held, err := store.Acquire(ctx, "deploy", time.Minute)
	if err != nil {
		t.Fatalf("Acquire returned %v", err)
	}

	releaseErr := make(chan error, 1)
	go func() {
		time.Sleep(20 * time.Millisecond)
		releaseErr <- store.Release(context.Background(), held)
	}()

	var callbackLease LockLease
	err = WithLock(ctx, store, "deploy", time.Minute, 500*time.Millisecond, func(_ context.Context, lease LockLease) error {
		callbackLease = lease
		return nil
	})
	if err != nil {
		t.Fatalf("WithLock returned %v", err)
	}
	if err := <-releaseErr; err != nil {
		t.Fatalf("background Release returned %v", err)
	}
	if callbackLease.Token == "" {
		t.Fatal("WithLock callback received empty token")
	}
	if callbackLease.Token == held.Token {
		t.Fatal("WithLock callback reused the previous owner token")
	}
	if _, err := store.Acquire(ctx, "deploy", time.Minute); err != nil {
		t.Fatalf("Acquire after WithLock returned %v", err)
	}
}

func TestWithLockReturnsTimeout(t *testing.T) {
	ctx := context.Background()
	store := NewMemoryLockStore()

	held, err := store.Acquire(ctx, "deploy", time.Minute)
	if err != nil {
		t.Fatalf("Acquire returned %v", err)
	}
	defer func() {
		if err := store.Release(ctx, held); err != nil {
			t.Fatalf("Release returned %v", err)
		}
	}()

	err = WithLock(ctx, store, "deploy", time.Minute, time.Millisecond, func(context.Context, LockLease) error {
		t.Fatal("callback was called")
		return nil
	})
	if !errors.Is(err, ErrMigrationLockTimeout) {
		t.Fatalf("WithLock error = %v, want ErrMigrationLockTimeout", err)
	}
}

func TestWithLockReleasesAfterCallbackError(t *testing.T) {
	ctx := context.Background()
	store := NewMemoryLockStore()
	sentinel := errors.New("migration failed")

	err := WithLock(ctx, store, "deploy", time.Minute, 0, func(context.Context, LockLease) error {
		return sentinel
	})
	if !errors.Is(err, sentinel) {
		t.Fatalf("WithLock error = %v, want sentinel", err)
	}
	if _, err := store.Acquire(ctx, "deploy", time.Minute); err != nil {
		t.Fatalf("Acquire after callback error returned %v", err)
	}
}

func TestWithLockReleasesAfterCallbackPanic(t *testing.T) {
	ctx := context.Background()
	store := NewMemoryLockStore()

	func() {
		defer func() {
			if got := recover(); got != "boom" {
				t.Fatalf("panic = %v, want boom", got)
			}
		}()

		_ = WithLock(ctx, store, "deploy", time.Minute, 0, func(context.Context, LockLease) error {
			panic("boom")
		})
	}()

	if _, err := store.Acquire(ctx, "deploy", time.Minute); err != nil {
		t.Fatalf("Acquire after callback panic returned %v", err)
	}
}
