package jobs

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestConcurrencyLimiterAcquireReleaseDefaultLimit(t *testing.T) {
	t.Parallel()
	limiter := NewConcurrencyLimiter(1, nil)

	if err := limiter.Acquire(context.Background(), "sync"); err != nil {
		t.Fatalf("first Acquire: %v", err)
	}

	acquired := make(chan error, 1)
	go func() {
		acquired <- limiter.Acquire(context.Background(), "sync")
	}()
	assertAcquireBlocked(t, acquired)

	limiter.Release("sync")
	if err := receiveAcquireResult(t, acquired); err != nil {
		t.Fatalf("second Acquire after Release: %v", err)
	}
	limiter.Release("sync")
}

func TestConcurrencyLimiterZeroValue(t *testing.T) {
	t.Parallel()
	var limiter ConcurrencyLimiter

	if err := limiter.Acquire(context.Background(), "sync"); err != nil {
		t.Fatalf("Acquire: %v", err)
	}
	if err := canceledAcquire(&limiter, "sync"); !errors.Is(err, context.Canceled) {
		t.Fatalf("second Acquire err = %v, want context.Canceled", err)
	}
	limiter.Release("sync")
}

func TestConcurrencyLimiterKeysAreIndependent(t *testing.T) {
	t.Parallel()
	limiter := NewConcurrencyLimiter(1, nil)

	if err := limiter.Acquire(context.Background(), "tenant-a"); err != nil {
		t.Fatalf("Acquire tenant-a: %v", err)
	}
	if err := limiter.Acquire(context.Background(), "tenant-b"); err != nil {
		t.Fatalf("Acquire tenant-b: %v", err)
	}

	limiter.Release("tenant-a")
	limiter.Release("tenant-b")
}

func TestConcurrencyLimiterPerKeyLimitOverridesDefault(t *testing.T) {
	t.Parallel()
	keyLimits := map[string]int{"bulk": 2}
	limiter := NewConcurrencyLimiter(1, keyLimits)
	keyLimits["bulk"] = 1

	if err := limiter.Acquire(context.Background(), "bulk"); err != nil {
		t.Fatalf("bulk Acquire 1: %v", err)
	}
	if err := limiter.Acquire(context.Background(), "bulk"); err != nil {
		t.Fatalf("bulk Acquire 2: %v", err)
	}
	if err := canceledAcquire(limiter, "bulk"); !errors.Is(err, context.Canceled) {
		t.Fatalf("bulk Acquire 3 err = %v, want context.Canceled", err)
	}

	if err := limiter.Acquire(context.Background(), "default"); err != nil {
		t.Fatalf("default Acquire 1: %v", err)
	}
	if err := canceledAcquire(limiter, "default"); !errors.Is(err, context.Canceled) {
		t.Fatalf("default Acquire 2 err = %v, want context.Canceled", err)
	}

	limiter.Release("bulk")
	limiter.Release("bulk")
	limiter.Release("default")
}

func TestConcurrencyLimiterCanceledAcquireDoesNotLeak(t *testing.T) {
	t.Parallel()
	limiter := NewConcurrencyLimiter(1, nil)

	if err := limiter.Acquire(context.Background(), "sync"); err != nil {
		t.Fatalf("first Acquire: %v", err)
	}

	ctx, cancel := context.WithCancel(context.Background())
	acquired := make(chan error, 1)
	go func() {
		acquired <- limiter.Acquire(ctx, "sync")
	}()
	assertAcquireBlocked(t, acquired)

	cancel()
	if err := receiveAcquireResult(t, acquired); !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled Acquire err = %v, want context.Canceled", err)
	}

	limiter.Release("sync")
	if err := limiter.Acquire(context.Background(), "sync"); err != nil {
		t.Fatalf("Acquire after cancelled waiter: %v", err)
	}
	limiter.Release("sync")
}

func TestConcurrencyLimiterAlreadyCanceledContext(t *testing.T) {
	t.Parallel()
	limiter := NewConcurrencyLimiter(1, nil)
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if err := limiter.Acquire(ctx, "sync"); !errors.Is(err, context.Canceled) {
		t.Fatalf("Acquire err = %v, want context.Canceled", err)
	}
	if err := limiter.Acquire(context.Background(), "sync"); err != nil {
		t.Fatalf("Acquire after canceled context: %v", err)
	}
	limiter.Release("sync")
}

func TestConcurrencyLimiterRunReleasesOnError(t *testing.T) {
	t.Parallel()
	limiter := NewConcurrencyLimiter(1, nil)
	boom := errors.New("boom")

	err := limiter.Run(context.Background(), "sync", func(context.Context) error {
		return boom
	})
	if !errors.Is(err, boom) {
		t.Fatalf("Run err = %v, want boom", err)
	}

	if err := limiter.Acquire(context.Background(), "sync"); err != nil {
		t.Fatalf("Acquire after Run error: %v", err)
	}
	limiter.Release("sync")
}

func TestConcurrencyLimiterRunReleasesOnPanic(t *testing.T) {
	t.Parallel()
	limiter := NewConcurrencyLimiter(1, nil)

	panicValue := catchConcurrencyPanic(t, func() {
		_ = limiter.Run(context.Background(), "sync", func(context.Context) error {
			panic("boom")
		})
	})
	if panicValue != "boom" {
		t.Fatalf("Run panic = %v, want boom", panicValue)
	}

	if err := limiter.Acquire(context.Background(), "sync"); err != nil {
		t.Fatalf("Acquire after Run panic: %v", err)
	}
	limiter.Release("sync")
}

func TestConcurrencyLimiterRunRejectsNilFunction(t *testing.T) {
	t.Parallel()
	limiter := NewConcurrencyLimiter(1, nil)

	if err := limiter.Run(context.Background(), "sync", nil); err == nil {
		t.Fatal("expected nil Run function to fail")
	}
	if err := limiter.Acquire(context.Background(), "sync"); err != nil {
		t.Fatalf("Acquire after nil Run function: %v", err)
	}
	limiter.Release("sync")
}

func canceledAcquire(limiter *ConcurrencyLimiter, key string) error {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	return limiter.Acquire(ctx, key)
}

func assertAcquireBlocked(t *testing.T, acquired <-chan error) {
	t.Helper()

	select {
	case err := <-acquired:
		t.Fatalf("Acquire returned while the key was still at its limit: %v", err)
	case <-time.After(25 * time.Millisecond):
	}
}

func receiveAcquireResult(t *testing.T, acquired <-chan error) error {
	t.Helper()

	select {
	case err := <-acquired:
		return err
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for Acquire")
		return nil
	}
}

func catchConcurrencyPanic(t *testing.T, fn func()) (panicValue any) {
	t.Helper()

	defer func() {
		panicValue = recover()
		if panicValue == nil {
			t.Fatal("function did not panic")
		}
	}()

	fn()
	return nil
}
