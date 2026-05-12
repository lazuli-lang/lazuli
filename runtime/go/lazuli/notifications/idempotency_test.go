package notifications_test

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"

	"lazuli.dev/runtime/lazuli/notifications"
)

var _ notifications.IdempotencyStore = (*notifications.MemoryIdempotencyStore)(nil)

func TestMemoryIdempotencyStoreRejectsDuplicateClaim(t *testing.T) {
	t.Parallel()

	store := notifications.NewMemoryIdempotencyStore()
	key := notifications.IdempotencyKey{
		Notification: "billing.invoice_due",
		Tenant:       "tenant-1",
		Key:          "invoice-123",
	}

	if err := store.Claim(context.Background(), key, time.Hour); err != nil {
		t.Fatalf("first Claim failed: %v", err)
	}
	err := store.Claim(context.Background(), key, time.Hour)
	if !errors.Is(err, notifications.ErrNotificationIdempotent) {
		t.Fatalf("expected ErrNotificationIdempotent, got %v", err)
	}
}

func TestMemoryIdempotencyStoreExpiresClaims(t *testing.T) {
	t.Parallel()

	now := time.Unix(1_700_000_000, 0)
	store := notifications.NewMemoryIdempotencyStore()
	store.Clock = func() time.Time { return now }
	key := notifications.IdempotencyKey{
		Notification: "billing.invoice_due",
		Tenant:       "tenant-1",
		Key:          "invoice-123",
	}

	if err := store.Claim(context.Background(), key, time.Hour); err != nil {
		t.Fatalf("first Claim failed: %v", err)
	}

	now = now.Add(59 * time.Minute)
	if err := store.Claim(context.Background(), key, time.Hour); !errors.Is(err, notifications.ErrNotificationIdempotent) {
		t.Fatalf("expected active claim to reject duplicate, got %v", err)
	}

	now = now.Add(time.Minute)
	if err := store.Claim(context.Background(), key, time.Hour); err != nil {
		t.Fatalf("Claim after expiry failed: %v", err)
	}
}

func TestMemoryIdempotencyStoreScopesClaims(t *testing.T) {
	t.Parallel()

	store := notifications.NewMemoryIdempotencyStore()
	base := notifications.IdempotencyKey{
		Notification: "billing.invoice_due",
		Tenant:       "tenant-1",
		Key:          "invoice-123",
	}

	if err := store.Claim(context.Background(), base, time.Hour); err != nil {
		t.Fatalf("base Claim failed: %v", err)
	}

	independent := []notifications.IdempotencyKey{
		{Notification: "billing.invoice_overdue", Tenant: "tenant-1", Key: "invoice-123"},
		{Notification: "billing.invoice_due", Tenant: "tenant-2", Key: "invoice-123"},
		{Notification: "billing.invoice_due", Tenant: "tenant-1", Key: "invoice-456"},
	}
	for _, key := range independent {
		if err := store.Claim(context.Background(), key, time.Hour); err != nil {
			t.Fatalf("independent Claim for %+v failed: %v", key, err)
		}
	}
}

func TestMemoryIdempotencyStoreAllowsOnlyOneConcurrentClaim(t *testing.T) {
	t.Parallel()

	store := notifications.NewMemoryIdempotencyStore()
	key := notifications.IdempotencyKey{
		Notification: "billing.invoice_due",
		Tenant:       "tenant-1",
		Key:          "invoice-123",
	}

	const workers = 64
	start := make(chan struct{})
	errs := make(chan error, workers)
	var wg sync.WaitGroup
	wg.Add(workers)
	for i := 0; i < workers; i++ {
		go func() {
			defer wg.Done()
			<-start
			errs <- store.Claim(context.Background(), key, time.Hour)
		}()
	}

	close(start)
	wg.Wait()
	close(errs)

	successes := 0
	duplicates := 0
	for err := range errs {
		switch {
		case err == nil:
			successes++
		case errors.Is(err, notifications.ErrNotificationIdempotent):
			duplicates++
		default:
			t.Fatalf("unexpected Claim error: %v", err)
		}
	}
	if successes != 1 {
		t.Fatalf("successes = %d, want 1", successes)
	}
	if duplicates != workers-1 {
		t.Fatalf("duplicates = %d, want %d", duplicates, workers-1)
	}
}

func TestMemoryIdempotencyStoreRejectsInvalidTTL(t *testing.T) {
	t.Parallel()

	store := notifications.NewMemoryIdempotencyStore()
	err := store.Claim(context.Background(), notifications.IdempotencyKey{
		Notification: "billing.invoice_due",
		Tenant:       "tenant-1",
		Key:          "invoice-123",
	}, 0)
	if !errors.Is(err, notifications.ErrInvalidDuration) {
		t.Fatalf("expected ErrInvalidDuration, got %v", err)
	}
}
