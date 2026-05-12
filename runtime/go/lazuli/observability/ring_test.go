package observability

import (
	"context"
	"fmt"
	"sync"
	"testing"
	"time"
)

func TestTraceRingSnapshotReportsRecentEventsAndDropped(t *testing.T) {
	ring := NewTraceRing(3)
	base := time.Date(2026, 5, 12, 12, 0, 0, 0, time.UTC)

	for i := 1; i <= 5; i++ {
		event := NewTraceEvent(TraceEventName(fmt.Sprintf("event-%d", i)), map[string]any{
			"index": i,
		})
		event.RecordedAt = base.Add(time.Duration(i) * time.Second)
		ring.Add(event)
	}

	snapshot := ring.Snapshot()
	if snapshot.Capacity != 3 {
		t.Fatalf("Snapshot().Capacity = %d, want 3", snapshot.Capacity)
	}
	if snapshot.Dropped != 2 {
		t.Fatalf("Snapshot().Dropped = %d, want 2", snapshot.Dropped)
	}
	if ring.Len() != 3 {
		t.Fatalf("Len() = %d, want 3", ring.Len())
	}
	if ring.Capacity() != 3 {
		t.Fatalf("Capacity() = %d, want 3", ring.Capacity())
	}
	if ring.Dropped() != 2 {
		t.Fatalf("Dropped() = %d, want 2", ring.Dropped())
	}
	traceRingAssertNames(t, snapshot.Events, []TraceEventName{"event-3", "event-4", "event-5"})

	snapshot.Events[0].Name = "mutated"
	snapshot.Events[0].Payload.Fields["index"] = 99

	next := ring.Snapshot()
	traceRingAssertNames(t, next.Events, []TraceEventName{"event-3", "event-4", "event-5"})
	if got := next.Events[0].Payload.Fields["index"]; got != 3 {
		t.Fatalf("snapshot mutation changed retained payload index = %v, want 3", got)
	}
}

func TestTraceRingZeroCapacityDropsAllEvents(t *testing.T) {
	ring := NewTraceRing(-1)

	ring.Add(NewTraceEvent("one", nil))
	ring.Add(NewTraceEvent("two", nil))

	snapshot := ring.Snapshot()
	if snapshot.Capacity != 0 {
		t.Fatalf("Snapshot().Capacity = %d, want 0", snapshot.Capacity)
	}
	if len(snapshot.Events) != 0 {
		t.Fatalf("Snapshot().Events len = %d, want 0", len(snapshot.Events))
	}
	if snapshot.Dropped != 2 {
		t.Fatalf("Snapshot().Dropped = %d, want 2", snapshot.Dropped)
	}
}

func TestTraceRingCopiesTypedPayloads(t *testing.T) {
	ring := NewTraceRing(1)
	payload := AgentRunPayload{
		Agent: "support.agent",
		Tools: []ToolCall{{
			Name:   "kb.search",
			Effect: "read",
			Status: "ok",
		}},
	}

	ring.Add(NewAgentRunTraceEvent(payload))
	payload.Tools[0].Name = "mutated"

	snapshot := ring.Snapshot()
	if len(snapshot.Events) != 1 {
		t.Fatalf("Snapshot().Events len = %d, want 1", len(snapshot.Events))
	}
	if snapshot.Events[0].Payload.AgentRun == nil {
		t.Fatal("Snapshot().Events[0].Payload.AgentRun is nil")
	}
	if got := snapshot.Events[0].Payload.AgentRun.Tools[0].Name; got != "kb.search" {
		t.Fatalf("AgentRun.Tools[0].Name = %q, want kb.search", got)
	}

	snapshot.Events[0].Payload.AgentRun.Tools[0].Name = "returned mutation"
	next := ring.Snapshot()
	if got := next.Events[0].Payload.AgentRun.Tools[0].Name; got != "kb.search" {
		t.Fatalf("snapshot mutation changed AgentRun.Tools[0].Name = %q, want kb.search", got)
	}
	if next.Events[0].RecordedAt.IsZero() {
		t.Fatal("RecordedAt is zero, want Add to stamp retained events")
	}
}

func TestEmitRunRecordsRecentTraceEvents(t *testing.T) {
	traceRingSwapRecentForTest(t, NewTraceRing(2))

	EmitCommandRun(context.Background(), CommandRunPayload{
		Command: "customer.create",
		Status:  "ok",
	})
	EmitJobRun(context.Background(), JobRunPayload{
		Job:    "billing.sync",
		Status: "failed",
	})

	snapshot := RecentTraceEvents()
	if snapshot.Dropped != 0 {
		t.Fatalf("RecentTraceEvents().Dropped = %d, want 0", snapshot.Dropped)
	}
	traceRingAssertNames(t, snapshot.Events, []TraceEventName{TraceEventCommandRun, TraceEventJobRun})
	if snapshot.Events[0].Payload.CommandRun == nil {
		t.Fatal("command_run payload is nil")
	}
	if got := snapshot.Events[0].Payload.CommandRun.Command; got != "customer.create" {
		t.Fatalf("CommandRun.Command = %q, want customer.create", got)
	}
	if snapshot.Events[1].Payload.JobRun == nil {
		t.Fatal("job_run payload is nil")
	}
	if got := snapshot.Events[1].Payload.JobRun.Job; got != "billing.sync" {
		t.Fatalf("JobRun.Job = %q, want billing.sync", got)
	}
}

func TestTraceRingConcurrentAddAndSnapshot(t *testing.T) {
	const (
		writers   = 8
		perWriter = 200
		capacity  = 64
	)

	ring := NewTraceRing(capacity)
	start := make(chan struct{})
	errs := make(chan string, writers)
	var wg sync.WaitGroup

	for worker := 0; worker < writers; worker++ {
		wg.Add(1)
		go func(worker int) {
			defer wg.Done()
			<-start
			for i := 0; i < perWriter; i++ {
				ring.Add(NewTraceEvent(TraceEventName(fmt.Sprintf("worker-%d", worker)), map[string]any{
					"seq": i,
				}))
				if i%7 == 0 {
					snapshot := ring.Snapshot()
					if len(snapshot.Events) > snapshot.Capacity {
						traceRingReportConcurrentError(errs,
							"snapshot retained %d events with capacity %d",
							len(snapshot.Events), snapshot.Capacity)
						return
					}
				}
			}
		}(worker)
	}

	close(start)
	wg.Wait()
	close(errs)

	for err := range errs {
		t.Error(err)
	}

	snapshot := ring.Snapshot()
	if len(snapshot.Events) != capacity {
		t.Fatalf("Snapshot().Events len = %d, want %d", len(snapshot.Events), capacity)
	}
	total := uint64(writers * perWriter)
	retained := uint64(len(snapshot.Events))
	if snapshot.Dropped+retained != total {
		t.Fatalf("dropped + retained = %d, want %d", snapshot.Dropped+retained, total)
	}
}

func traceRingSwapRecentForTest(t *testing.T, ring *TraceRing) {
	t.Helper()

	recentTraceRing.Lock()
	previous := recentTraceRing.ring
	recentTraceRing.ring = ring
	recentTraceRing.Unlock()

	t.Cleanup(func() {
		recentTraceRing.Lock()
		recentTraceRing.ring = previous
		recentTraceRing.Unlock()
	})
}

func traceRingAssertNames(t *testing.T, events []TraceEvent, want []TraceEventName) {
	t.Helper()

	if len(events) != len(want) {
		t.Fatalf("events len = %d, want %d", len(events), len(want))
	}
	for i := range events {
		if events[i].Name != want[i] {
			t.Fatalf("events[%d].Name = %q, want %q", i, events[i].Name, want[i])
		}
	}
}

func traceRingReportConcurrentError(errs chan<- string, format string, args ...any) {
	select {
	case errs <- fmt.Sprintf(format, args...):
	default:
	}
}
