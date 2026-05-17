// Throttle store tests — verify the rate.Limiter-backed memory store
// honours the contract's Burst/MaxPer semantics and the per-key
// isolation the dispatcher relies on. Uses `testing/synctest` to
// advance the bucket without burning wall-clock.
package notifications_test

import (
	"context"
	"errors"
	"testing"
	"testing/synctest"
	"time"

	"lazuli.dev/runtime/lazuli/notifications"
)

func TestThrottleAllowsBurstThenRejects(t *testing.T) {
	t.Parallel()

	store := notifications.NewMemoryThrottleStore()
	spec := notifications.NotificationThrottle{MaxPer: "1m", Burst: 3}
	key := notifications.ThrottleKey{
		Notification: "welcome_email",
		Recipient:    "alice@example.com",
		Channel:      notifications.ChannelEmail,
	}

	ctx := context.Background()
	for i := 0; i < 3; i++ {
		ok, _, err := store.Allow(ctx, key, spec)
		if err != nil {
			t.Fatalf("call %d expected nil error, got %v", i, err)
		}
		if !ok {
			t.Fatalf("call %d expected allow, got reject", i)
		}
	}

	ok, retryAt, err := store.Allow(ctx, key, spec)
	if ok {
		t.Fatalf("4th call expected reject, got allow")
	}
	if !errors.Is(err, notifications.ErrThrottleExceeded) {
		t.Fatalf("expected ErrThrottleExceeded, got %v", err)
	}
	if retryAt.IsZero() {
		t.Fatalf("expected non-zero retryAt on reject")
	}
}

func TestThrottleRefillsAfterWindow(t *testing.T) {
	t.Parallel()

	synctest.Test(t, func(t *testing.T) {
		store := notifications.NewMemoryThrottleStore()
		spec := notifications.NotificationThrottle{MaxPer: "60s", Burst: 1}
		key := notifications.ThrottleKey{
			Notification: "welcome_email",
			Recipient:    "alice@example.com",
			Channel:      notifications.ChannelEmail,
		}
		ctx := context.Background()

		// First call consumes the only token.
		ok, _, err := store.Allow(ctx, key, spec)
		if err != nil || !ok {
			t.Fatalf("first call: ok=%v err=%v", ok, err)
		}

		// Second call inside the window is rejected.
		ok, _, err = store.Allow(ctx, key, spec)
		if ok {
			t.Fatalf("second call inside window should be rejected")
		}
		if !errors.Is(err, notifications.ErrThrottleExceeded) {
			t.Fatalf("expected ErrThrottleExceeded; got %v", err)
		}

		// Advance virtual time past the window. rate.Limiter refills
		// smoothly, so 61s gives us at least one full token.
		time.Sleep(61 * time.Second)
		synctest.Wait()

		ok, _, err = store.Allow(ctx, key, spec)
		if err != nil || !ok {
			t.Fatalf("after window: ok=%v err=%v", ok, err)
		}
	})
}

func TestThrottleIsolatesKeys(t *testing.T) {
	t.Parallel()

	store := notifications.NewMemoryThrottleStore()
	spec := notifications.NotificationThrottle{MaxPer: "1m", Burst: 1}
	ctx := context.Background()

	keyA := notifications.ThrottleKey{
		Notification: "welcome_email",
		Recipient:    "alice@example.com",
		Channel:      notifications.ChannelEmail,
	}
	keyB := notifications.ThrottleKey{
		Notification: "welcome_email",
		Recipient:    "bob@example.com",
		Channel:      notifications.ChannelEmail,
	}

	// Both keys consume their single token.
	for _, k := range []notifications.ThrottleKey{keyA, keyB} {
		ok, _, err := store.Allow(ctx, k, spec)
		if err != nil || !ok {
			t.Fatalf("key=%v: ok=%v err=%v", k, ok, err)
		}
	}

	// Each key now exhausted independently.
	for _, k := range []notifications.ThrottleKey{keyA, keyB} {
		ok, _, err := store.Allow(ctx, k, spec)
		if ok {
			t.Fatalf("key=%v: expected reject after burst", k)
		}
		if !errors.Is(err, notifications.ErrThrottleExceeded) {
			t.Fatalf("key=%v: expected ErrThrottleExceeded; got %v", k, err)
		}
	}
}

func TestThrottleDefaultBurstIsOne(t *testing.T) {
	t.Parallel()

	store := notifications.NewMemoryThrottleStore()
	// Burst omitted → coerced to 1.
	spec := notifications.NotificationThrottle{MaxPer: "1h"}
	key := notifications.ThrottleKey{
		Notification: "digest",
		Channel:      notifications.ChannelEmail,
	}
	ctx := context.Background()

	ok, _, err := store.Allow(ctx, key, spec)
	if err != nil || !ok {
		t.Fatalf("first call: ok=%v err=%v", ok, err)
	}
	ok, _, err = store.Allow(ctx, key, spec)
	if ok {
		t.Fatalf("second call expected reject (default burst=1)")
	}
	if !errors.Is(err, notifications.ErrThrottleExceeded) {
		t.Fatalf("expected ErrThrottleExceeded; got %v", err)
	}
}

func TestThrottleInvalidDurationReturnsError(t *testing.T) {
	t.Parallel()

	store := notifications.NewMemoryThrottleStore()
	spec := notifications.NotificationThrottle{MaxPer: "garbage", Burst: 1}
	key := notifications.ThrottleKey{Notification: "n", Channel: notifications.ChannelEmail}

	ok, _, err := store.Allow(context.Background(), key, spec)
	if ok {
		t.Fatalf("invalid duration should not allow")
	}
	if !errors.Is(err, notifications.ErrInvalidDuration) {
		t.Fatalf("expected ErrInvalidDuration; got %v", err)
	}
}
