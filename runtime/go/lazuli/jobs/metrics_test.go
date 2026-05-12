package jobs

import (
	"errors"
	"sync"
	"testing"
	"time"
)

func TestMetricsCollectorRecordsByKind(t *testing.T) {
	t.Parallel()

	var collector MetricsCollector
	collector.RecordStarted("billing.settle")
	collector.RecordStarted("billing.settle")
	collector.RecordRetry("billing.settle")
	collector.RecordFinished("billing.settle", 10*time.Millisecond, nil)
	collector.RecordFinished("billing.settle", 25*time.Millisecond, errors.New("boom"))
	collector.RecordFinished("email.send", -time.Second, nil)

	snapshot := collector.Snapshot()
	billing := snapshot["billing.settle"]
	if billing.Started != 2 {
		t.Fatalf("Started = %d, want 2", billing.Started)
	}
	if billing.Finished != 2 {
		t.Fatalf("Finished = %d, want 2", billing.Finished)
	}
	if billing.Failures != 1 {
		t.Fatalf("Failures = %d, want 1", billing.Failures)
	}
	if billing.Retries != 1 {
		t.Fatalf("Retries = %d, want 1", billing.Retries)
	}
	if billing.Running != 0 {
		t.Fatalf("Running = %d, want 0", billing.Running)
	}
	if billing.DurationTotal != 35*time.Millisecond {
		t.Fatalf("DurationTotal = %v, want 35ms", billing.DurationTotal)
	}

	email := snapshot["email.send"]
	if email.Finished != 1 {
		t.Fatalf("email Finished = %d, want 1", email.Finished)
	}
	if email.Running != 0 {
		t.Fatalf("email Running = %d, want 0", email.Running)
	}
	if email.DurationTotal != 0 {
		t.Fatalf("email DurationTotal = %v, want 0", email.DurationTotal)
	}
}

func TestMetricsCollectorSnapshotReturnsCopy(t *testing.T) {
	t.Parallel()

	var collector MetricsCollector
	collector.RecordStarted("customer.send_welcome")

	snapshot := collector.Snapshot()
	snapshot["customer.send_welcome"] = JobMetricsSnapshot{Started: 99}

	got := collector.Snapshot()["customer.send_welcome"]
	if got.Started != 1 {
		t.Fatalf("Started = %d, want 1", got.Started)
	}
}

func TestMetricsCollectorConcurrentUse(t *testing.T) {
	t.Parallel()

	var collector MetricsCollector
	const workers = 16
	const iterations = 1000

	var wg sync.WaitGroup
	wg.Add(workers)
	for i := 0; i < workers; i++ {
		go func() {
			defer wg.Done()
			for j := 0; j < iterations; j++ {
				collector.RecordStarted("sync.reconcile")
				collector.RecordRetry("sync.reconcile")
				collector.RecordFinished("sync.reconcile", time.Millisecond, nil)
			}
		}()
	}
	wg.Wait()

	got := collector.Snapshot()["sync.reconcile"]
	want := uint64(workers * iterations)
	if got.Started != want {
		t.Fatalf("Started = %d, want %d", got.Started, want)
	}
	if got.Finished != want {
		t.Fatalf("Finished = %d, want %d", got.Finished, want)
	}
	if got.Retries != want {
		t.Fatalf("Retries = %d, want %d", got.Retries, want)
	}
	if got.Running != 0 {
		t.Fatalf("Running = %d, want 0", got.Running)
	}
	if got.DurationTotal != time.Duration(workers*iterations)*time.Millisecond {
		t.Fatalf("DurationTotal = %v, want %v",
			got.DurationTotal, time.Duration(workers*iterations)*time.Millisecond)
	}
}
