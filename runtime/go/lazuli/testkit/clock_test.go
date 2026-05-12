package testkit

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestClockAfterFiresOnlyWhenAdvanced(t *testing.T) {
	start := time.Date(2026, time.May, 12, 10, 30, 0, 0, time.UTC)
	clock := NewClock(start)

	ch := clock.After(10 * time.Second)
	assertNoClockTime(t, ch)

	clock.Advance(9 * time.Second)
	assertNoClockTime(t, ch)

	clock.Advance(time.Second)
	if got, want := receiveClockTime(t, ch), start.Add(10*time.Second); !got.Equal(want) {
		t.Fatalf("After() delivered %s, want %s", got, want)
	}

	clock.Advance(time.Hour)
	assertNoClockTime(t, ch)
}

func TestClockAfterDeliversScheduledVirtualTime(t *testing.T) {
	start := time.Date(2026, time.May, 12, 10, 30, 0, 0, time.UTC)
	clock := NewClock(start)

	first := clock.After(2 * time.Second)
	second := clock.After(5 * time.Second)

	clock.Advance(10 * time.Second)

	if got, want := receiveClockTime(t, first), start.Add(2*time.Second); !got.Equal(want) {
		t.Fatalf("first timer delivered %s, want %s", got, want)
	}
	if got, want := receiveClockTime(t, second), start.Add(5*time.Second); !got.Equal(want) {
		t.Fatalf("second timer delivered %s, want %s", got, want)
	}
	if got, want := clock.Now(), start.Add(10*time.Second); !got.Equal(want) {
		t.Fatalf("Now() = %s, want %s", got, want)
	}
}

func TestClockSleepReturnsWhenAdvanced(t *testing.T) {
	start := time.Date(2026, time.May, 12, 10, 30, 0, 0, time.UTC)
	clock := NewClock(start)
	errc := make(chan error, 1)

	go func() {
		errc <- clock.Sleep(context.Background(), 5*time.Second)
	}()
	waitForPendingClockTimers(t, clock, 1)

	clock.Advance(5 * time.Second)
	if err := receiveClockError(t, errc); err != nil {
		t.Fatalf("Sleep() error = %v, want nil", err)
	}
	waitForPendingClockTimers(t, clock, 0)
}

func TestClockSleepReturnsContextCancellationAndRemovesTimer(t *testing.T) {
	start := time.Date(2026, time.May, 12, 10, 30, 0, 0, time.UTC)
	clock := NewClock(start)
	ctx, cancel := context.WithCancel(context.Background())
	errc := make(chan error, 1)

	go func() {
		errc <- clock.Sleep(ctx, time.Hour)
	}()
	waitForPendingClockTimers(t, clock, 1)

	cancel()
	if err := receiveClockError(t, errc); !errors.Is(err, context.Canceled) {
		t.Fatalf("Sleep() error = %v, want %v", err, context.Canceled)
	}
	waitForPendingClockTimers(t, clock, 0)
}

func TestClockSleepHonorsAlreadyCanceledContext(t *testing.T) {
	clock := NewClock(time.Date(2026, time.May, 12, 10, 30, 0, 0, time.UTC))
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if err := clock.Sleep(ctx, time.Hour); !errors.Is(err, context.Canceled) {
		t.Fatalf("Sleep() error = %v, want %v", err, context.Canceled)
	}
	waitForPendingClockTimers(t, clock, 0)
}

func receiveClockTime(t *testing.T, ch <-chan time.Time) time.Time {
	t.Helper()

	select {
	case value := <-ch:
		return value
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for clock timer")
	}

	return time.Time{}
}

func receiveClockError(t *testing.T, ch <-chan error) error {
	t.Helper()

	select {
	case err := <-ch:
		return err
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for clock sleep")
	}

	return nil
}

func assertNoClockTime(t *testing.T, ch <-chan time.Time) {
	t.Helper()

	select {
	case value := <-ch:
		t.Fatalf("received unexpected clock time %s", value)
	default:
	}
}

func waitForPendingClockTimers(t *testing.T, clock *Clock, want int) {
	t.Helper()

	deadline := time.Now().Add(time.Second)
	for time.Now().Before(deadline) {
		if got := pendingClockTimers(clock); got == want {
			return
		}
		time.Sleep(time.Millisecond)
	}
	t.Fatalf("pending clock timers = %d, want %d", pendingClockTimers(clock), want)
}

func pendingClockTimers(clock *Clock) int {
	clock.mu.Lock()
	defer clock.mu.Unlock()
	return len(clock.timers)
}
