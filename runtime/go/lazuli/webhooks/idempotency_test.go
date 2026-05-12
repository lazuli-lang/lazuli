package webhooks

import (
	"context"
	"errors"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestMemoryIdempotencyStoreClaimRejectsDuplicateWithinTTL(t *testing.T) {
	store := NewMemoryIdempotencyStore()
	now := time.Unix(1_700_000_000, 0)
	store.nowFunc = func() time.Time { return now }

	if err := store.Claim(context.Background(), "evt_123", time.Minute); err != nil {
		t.Fatalf("first Claim error = %v", err)
	}
	if err := store.Claim(context.Background(), "evt_123", time.Minute); !errors.Is(err, ErrWebhookIdempotent) {
		t.Fatalf("second Claim error = %v, want ErrWebhookIdempotent", err)
	}
}

func TestMemoryIdempotencyStoreClaimAllowsAfterTTL(t *testing.T) {
	store := NewMemoryIdempotencyStore()
	now := time.Unix(1_700_000_000, 0)
	store.nowFunc = func() time.Time { return now }

	if err := store.Claim(context.Background(), "evt_123", time.Second); err != nil {
		t.Fatalf("first Claim error = %v", err)
	}

	now = now.Add(time.Second)
	if err := store.Claim(context.Background(), "evt_123", time.Second); err != nil {
		t.Fatalf("Claim after TTL error = %v", err)
	}
}

func TestMemoryIdempotencyStoreClaimIsAtomic(t *testing.T) {
	store := NewMemoryIdempotencyStore()

	const callers = 64
	var (
		successes  atomic.Int32
		duplicates atomic.Int32
		failures   atomic.Int32
		wg         sync.WaitGroup
	)

	wg.Add(callers)
	for i := 0; i < callers; i++ {
		go func() {
			defer wg.Done()
			err := store.Claim(context.Background(), "evt_concurrent", time.Minute)
			switch {
			case err == nil:
				successes.Add(1)
			case errors.Is(err, ErrWebhookIdempotent):
				duplicates.Add(1)
			default:
				failures.Add(1)
			}
		}()
	}
	wg.Wait()

	if failures.Load() != 0 {
		t.Fatalf("unexpected Claim failures = %d", failures.Load())
	}
	if successes.Load() != 1 {
		t.Fatalf("successful claims = %d, want 1", successes.Load())
	}
	if duplicates.Load() != callers-1 {
		t.Fatalf("duplicate claims = %d, want %d", duplicates.Load(), callers-1)
	}
}
