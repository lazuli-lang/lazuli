package jobs

import (
	"context"
	"errors"
	"testing"
	"time"
)

func TestShutdownCoordinatorStopsAcceptingCancelsAndWaitsForActiveWork(t *testing.T) {
	t.Parallel()
	var coordinator ShutdownCoordinator

	jobCtx, finish, err := coordinator.Begin(context.Background())
	if err != nil {
		t.Fatalf("Begin: %v", err)
	}
	if coordinator.Active() != 1 {
		t.Fatalf("Active = %d, want 1", coordinator.Active())
	}

	events := make(chan string, 2)
	shutdownResult := make(chan error, 1)
	go func() {
		shutdownResult <- coordinator.Shutdown(context.Background(), ShutdownOptions{
			Timeout: time.Second,
			Hooks: ShutdownHooks{
				OnStopAccepting: []ShutdownHook{
					func(context.Context) error {
						events <- "stop"
						return nil
					},
				},
				OnDrained: []ShutdownHook{
					func(context.Context) error {
						events <- "drained"
						return nil
					},
				},
			},
		})
	}()

	if got := receiveShutdownEvent(t, events); got != "stop" {
		t.Fatalf("first hook = %q, want stop", got)
	}
	if !coordinator.Stopping() {
		t.Fatal("coordinator should report stopping after Shutdown starts")
	}
	if _, _, err := coordinator.Begin(context.Background()); !errors.Is(err, ErrJobShutdown) {
		t.Fatalf("Begin during shutdown err = %v, want ErrJobShutdown", err)
	}
	select {
	case <-jobCtx.Done():
	default:
		t.Fatal("job context was not canceled by shutdown")
	}
	if !errors.Is(context.Cause(jobCtx), ErrJobShutdown) {
		t.Fatalf("context cause = %v, want ErrJobShutdown", context.Cause(jobCtx))
	}

	assertShutdownBlocked(t, shutdownResult)
	finish()
	if err := receiveShutdownResult(t, shutdownResult); err != nil {
		t.Fatalf("Shutdown: %v", err)
	}
	if got := receiveShutdownEvent(t, events); got != "drained" {
		t.Fatalf("second hook = %q, want drained", got)
	}
	if coordinator.Active() != 0 {
		t.Fatalf("Active after finish = %d, want 0", coordinator.Active())
	}
}

func TestShutdownCoordinatorTimeoutReturnsErrorAndRunsHook(t *testing.T) {
	t.Parallel()
	var coordinator ShutdownCoordinator

	jobCtx, finish, err := coordinator.Begin(context.Background())
	if err != nil {
		t.Fatalf("Begin: %v", err)
	}
	defer finish()

	timedOut := make(chan struct{}, 1)
	err = coordinator.Shutdown(context.Background(), ShutdownOptions{
		Timeout: 25 * time.Millisecond,
		Hooks: ShutdownHooks{
			OnTimeout: []ShutdownHook{
				func(context.Context) error {
					timedOut <- struct{}{}
					return nil
				},
			},
		},
	})
	if !errors.Is(err, ErrJobShutdownTimeout) {
		t.Fatalf("Shutdown err = %v, want ErrJobShutdownTimeout", err)
	}
	select {
	case <-timedOut:
	default:
		t.Fatal("timeout hook was not called")
	}
	select {
	case <-jobCtx.Done():
	default:
		t.Fatal("job context was not canceled")
	}
	if !errors.Is(context.Cause(jobCtx), ErrJobShutdown) {
		t.Fatalf("context cause = %v, want ErrJobShutdown", context.Cause(jobCtx))
	}
	if coordinator.Active() != 1 {
		t.Fatalf("Active before finish = %d, want 1", coordinator.Active())
	}
}

func TestShutdownCoordinatorJoinsHookErrors(t *testing.T) {
	t.Parallel()
	var coordinator ShutdownCoordinator
	stopErr := errors.New("stop hook failed")
	drainErr := errors.New("drain hook failed")

	err := coordinator.Shutdown(context.Background(), ShutdownOptions{
		Hooks: ShutdownHooks{
			OnStopAccepting: []ShutdownHook{
				func(context.Context) error { return stopErr },
			},
			OnDrained: []ShutdownHook{
				func(context.Context) error { return drainErr },
			},
		},
	})
	if !errors.Is(err, stopErr) {
		t.Fatalf("Shutdown err = %v, want stop hook error", err)
	}
	if !errors.Is(err, drainErr) {
		t.Fatalf("Shutdown err = %v, want drain hook error", err)
	}
	if _, _, err := coordinator.Begin(context.Background()); !errors.Is(err, ErrJobShutdown) {
		t.Fatalf("Begin after shutdown err = %v, want ErrJobShutdown", err)
	}
}

func TestShutdownCoordinatorContextCancellationStopsWaiting(t *testing.T) {
	t.Parallel()
	var coordinator ShutdownCoordinator

	jobCtx, finish, err := coordinator.Begin(context.Background())
	if err != nil {
		t.Fatalf("Begin: %v", err)
	}
	defer finish()

	ctx, cancel := context.WithCancel(context.Background())
	waiting := make(chan struct{}, 1)
	result := make(chan error, 1)
	go func() {
		result <- coordinator.Shutdown(ctx, ShutdownOptions{
			Hooks: ShutdownHooks{
				OnStopAccepting: []ShutdownHook{
					func(context.Context) error {
						waiting <- struct{}{}
						return nil
					},
				},
			},
		})
	}()

	receiveShutdownSignal(t, waiting)
	cancel()
	if err := receiveShutdownResult(t, result); !errors.Is(err, context.Canceled) {
		t.Fatalf("Shutdown err = %v, want context.Canceled", err)
	}
	select {
	case <-jobCtx.Done():
	default:
		t.Fatal("job context was not canceled")
	}
}

func assertShutdownBlocked(t *testing.T, result <-chan error) {
	t.Helper()

	select {
	case err := <-result:
		t.Fatalf("Shutdown returned before active work finished: %v", err)
	case <-time.After(25 * time.Millisecond):
	}
}

func receiveShutdownResult(t *testing.T, result <-chan error) error {
	t.Helper()

	select {
	case err := <-result:
		return err
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for Shutdown")
		return nil
	}
}

func receiveShutdownEvent(t *testing.T, events <-chan string) string {
	t.Helper()

	select {
	case event := <-events:
		return event
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for shutdown hook")
		return ""
	}
}

func receiveShutdownSignal(t *testing.T, signal <-chan struct{}) {
	t.Helper()

	select {
	case <-signal:
	case <-time.After(time.Second):
		t.Fatal("timed out waiting for shutdown signal")
	}
}
