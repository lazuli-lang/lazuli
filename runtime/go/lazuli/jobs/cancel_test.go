package jobs

import (
	"context"
	"fmt"
	"sync"
	"testing"
)

func TestCancellationRegistryCancelRecordsReasonAndCancelsContext(t *testing.T) {
	t.Parallel()
	registry := NewCancellationRegistry()
	ctx := registry.Start(context.Background(), "job-1")

	if registry.IsCancelled("job-1") {
		t.Fatal("job should not start cancelled")
	}
	if reason := registry.Reason("job-1"); reason != "" {
		t.Fatalf("Reason = %q, want empty", reason)
	}
	if !registry.Cancel("job-1", "user requested") {
		t.Fatal("Cancel returned false for active job")
	}

	select {
	case <-ctx.Done():
	default:
		t.Fatal("job context was not cancelled")
	}
	if got := context.Cause(ctx); got == nil || got.Error() != "user requested" {
		t.Fatalf("context cause = %v, want user requested", got)
	}
	if !registry.IsCancelled("job-1") {
		t.Fatal("IsCancelled returned false after Cancel")
	}
	if reason := registry.Reason("job-1"); reason != "user requested" {
		t.Fatalf("Reason = %q, want user requested", reason)
	}

	registry.Finish("job-1")
	if registry.IsCancelled("job-1") {
		t.Fatal("job should not be registered after Finish")
	}
	if reason := registry.Reason("job-1"); reason != "" {
		t.Fatalf("Reason after Finish = %q, want empty", reason)
	}
	if registry.Cancel("job-1", "late") {
		t.Fatal("Cancel returned true after Finish")
	}
}

func TestCancellationRegistryReusesActiveContextUntilFinish(t *testing.T) {
	t.Parallel()
	registry := NewCancellationRegistry()

	first := registry.Start(context.Background(), "job-1")
	second := registry.Start(context.Background(), "job-1")
	if first != second {
		t.Fatal("Start should return existing context for active id")
	}

	registry.Cancel("job-1", "first run")
	third := registry.Start(context.Background(), "job-1")
	if third != first {
		t.Fatal("Start should keep returning the active cancelled context until Finish")
	}
	if !registry.IsCancelled("job-1") {
		t.Fatal("cancelled active id should stay marked until Finish")
	}

	registry.Finish("job-1")
	fresh := registry.Start(context.Background(), "job-1")
	if fresh == first {
		t.Fatal("Start after Finish should create a fresh context")
	}
	select {
	case <-fresh.Done():
		t.Fatal("fresh context should not inherit prior cancellation")
	default:
	}
	registry.Finish("job-1")
}

func TestCancellationRegistryParentCancellationDoesNotSetRegistryReason(t *testing.T) {
	t.Parallel()
	registry := NewCancellationRegistry()
	parent, cancel := context.WithCancel(context.Background())
	ctx := registry.Start(parent, "job-1")

	cancel()
	<-ctx.Done()
	if registry.IsCancelled("job-1") {
		t.Fatal("parent cancellation should not be reported as registry cancellation")
	}
	if reason := registry.Reason("job-1"); reason != "" {
		t.Fatalf("Reason = %q, want empty", reason)
	}
	registry.Finish("job-1")
}

func TestCancellationRegistryZeroValueAndConcurrentAccess(t *testing.T) {
	t.Parallel()
	var registry CancellationRegistry
	var wg sync.WaitGroup

	for i := 0; i < 100; i++ {
		i := i
		wg.Add(1)
		go func() {
			defer wg.Done()
			id := fmt.Sprintf("job-%d", i%10)
			ctx := registry.Start(context.Background(), id)
			if i%2 == 0 {
				registry.Cancel(id, "stop")
			}
			_ = registry.IsCancelled(id)
			_ = registry.Reason(id)
			select {
			case <-ctx.Done():
			default:
			}
			if i%3 == 0 {
				registry.Finish(id)
			}
		}()
	}
	wg.Wait()

	for i := 0; i < 10; i++ {
		id := fmt.Sprintf("job-%d", i)
		registry.Finish(id)
		if registry.IsCancelled(id) {
			t.Fatalf("%s should be removed after Finish", id)
		}
	}
}
