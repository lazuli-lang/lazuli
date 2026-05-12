package observability

import (
	"context"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestMemoryTraceSinkStoresBuiltInPayloads(t *testing.T) {
	t.Parallel()

	now := time.Date(2026, 5, 12, 18, 30, 0, 0, time.UTC)
	sink := NewMemoryTraceSink(4)
	sink.Clock = func() time.Time { return now }

	if !sink.TryEmit(context.Background(), NewAgentRunTraceEvent(AgentRunPayload{
		Agent:        "support.triage",
		Model:        "@llm.fast",
		TokensInput:  12,
		TokensOutput: 8,
		Tools:        []ToolCall{{Name: "crm.lookup", Effect: "read"}},
		TraceID:      "trace-agent",
	})) {
		t.Fatal("TryEmit(agent_run) = false, want true")
	}
	if !sink.TryEmit(context.Background(), NewCommandRunTraceEvent(CommandRunPayload{
		Command:   "customer.reassign",
		Actor:     "@actor.user",
		Status:    "ok",
		RequestID: "req-command",
	})) {
		t.Fatal("TryEmit(command_run) = false, want true")
	}
	if !sink.TryEmit(context.Background(), NewJobRunTraceEvent(JobRunPayload{
		Job:     "billing.settle",
		Trigger: "schedule",
		Status:  "failed",
		Attempt: 2,
	})) {
		t.Fatal("TryEmit(job_run) = false, want true")
	}
	if !sink.TryEmit(context.Background(), NewWebhookRunTraceEvent(WebhookRunPayload{
		Webhook:        "stripe.invoice",
		Status:         "verify_failed",
		SignatureValid: false,
	})) {
		t.Fatal("TryEmit(webhook_run) = false, want true")
	}

	events := sink.Events()
	if len(events) != 4 {
		t.Fatalf("Events() len = %d, want 4", len(events))
	}
	traceSinkAssertEvent(t, events[0], TraceEventAgentRun, now)
	if events[0].AgentRun == nil || events[0].AgentRun.Agent != "support.triage" {
		t.Fatalf("agent_run payload = %+v, want support.triage", events[0].AgentRun)
	}
	if got := events[0].AgentRun.Tools[0].Name; got != "crm.lookup" {
		t.Fatalf("agent_run tool = %q, want crm.lookup", got)
	}
	traceSinkAssertEvent(t, events[1], TraceEventCommandRun, now)
	if events[1].CommandRun == nil || events[1].CommandRun.Command != "customer.reassign" {
		t.Fatalf("command_run payload = %+v, want customer.reassign", events[1].CommandRun)
	}
	traceSinkAssertEvent(t, events[2], TraceEventJobRun, now)
	if events[2].JobRun == nil || events[2].JobRun.Job != "billing.settle" {
		t.Fatalf("job_run payload = %+v, want billing.settle", events[2].JobRun)
	}
	traceSinkAssertEvent(t, events[3], TraceEventWebhookRun, now)
	if events[3].WebhookRun == nil || events[3].WebhookRun.Webhook != "stripe.invoice" {
		t.Fatalf("webhook_run payload = %+v, want stripe.invoice", events[3].WebhookRun)
	}
	if got := sink.Dropped(); got != 0 {
		t.Fatalf("Dropped() = %d, want 0", got)
	}
}

func TestMemoryTraceSinkDropsWhenFull(t *testing.T) {
	t.Parallel()

	sink := NewMemoryTraceSink(1)
	if !sink.TryEmit(context.Background(), NewCommandRunTraceEvent(CommandRunPayload{
		Command: "customer.create",
		Status:  "ok",
	})) {
		t.Fatal("first TryEmit = false, want true")
	}
	if sink.TryEmit(context.Background(), NewCommandRunTraceEvent(CommandRunPayload{
		Command: "customer.update",
		Status:  "ok",
	})) {
		t.Fatal("second TryEmit = true, want false after capacity is full")
	}

	events := sink.Events()
	if len(events) != 1 {
		t.Fatalf("Events() len = %d, want 1", len(events))
	}
	if events[0].CommandRun.Command != "customer.create" {
		t.Fatalf("stored command = %q, want customer.create", events[0].CommandRun.Command)
	}
	if got := sink.Dropped(); got != 1 {
		t.Fatalf("Dropped() = %d, want 1", got)
	}
}

func TestMemoryTraceSinkDropsCanceledContext(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	sink := NewMemoryTraceSink(1)

	if sink.TryEmit(ctx, NewJobRunTraceEvent(JobRunPayload{Job: "billing.settle"})) {
		t.Fatal("TryEmit with canceled context = true, want false")
	}
	if events := sink.Events(); len(events) != 0 {
		t.Fatalf("Events() len = %d, want 0", len(events))
	}
	if got := sink.Dropped(); got != 1 {
		t.Fatalf("Dropped() = %d, want 1", got)
	}
}

func TestMemoryTraceSinkDropsWhenBusy(t *testing.T) {
	t.Parallel()

	sink := NewMemoryTraceSink(1)
	sink.mu.Lock()
	defer sink.mu.Unlock()

	if sink.TryEmit(context.Background(), NewWebhookRunTraceEvent(WebhookRunPayload{
		Webhook: "stripe.invoice",
		Status:  "ok",
	})) {
		t.Fatal("TryEmit while sink lock is held = true, want false")
	}
	if got := sink.Dropped(); got != 1 {
		t.Fatalf("Dropped() = %d, want 1", got)
	}
}

func TestMemoryTraceSinkReturnsDefensiveCopies(t *testing.T) {
	t.Parallel()

	sink := NewMemoryTraceSink(1)
	payload := AgentRunPayload{
		Agent: "support.triage",
		Tools: []ToolCall{{Name: "crm.lookup", Effect: "read"}},
	}
	event := NewAgentRunTraceEvent(payload)

	if !sink.TryEmit(context.Background(), event) {
		t.Fatal("TryEmit = false, want true")
	}
	payload.Tools[0].Name = "mutated-original"
	event.AgentRun.Agent = "mutated-event"
	event.AgentRun.Tools[0].Name = "mutated-event-tool"

	events := sink.Events()
	if got := events[0].AgentRun.Agent; got != "support.triage" {
		t.Fatalf("stored Agent = %q, want support.triage", got)
	}
	if got := events[0].AgentRun.Tools[0].Name; got != "crm.lookup" {
		t.Fatalf("stored tool = %q, want crm.lookup", got)
	}

	events[0].AgentRun.Agent = "mutated-snapshot"
	events[0].AgentRun.Tools[0].Name = "mutated-snapshot-tool"
	again := sink.Events()
	if got := again[0].AgentRun.Agent; got != "support.triage" {
		t.Fatalf("stored Agent after snapshot mutation = %q, want support.triage", got)
	}
	if got := again[0].AgentRun.Tools[0].Name; got != "crm.lookup" {
		t.Fatalf("stored tool after snapshot mutation = %q, want crm.lookup", got)
	}
}

func TestMemoryTraceSinkConcurrentAccess(t *testing.T) {
	t.Parallel()

	const count = 128
	sink := NewMemoryTraceSink(count)
	var accepted atomic.Uint64
	var wg sync.WaitGroup

	for i := 0; i < count; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if sink.TryEmit(context.Background(), NewCommandRunTraceEvent(CommandRunPayload{
				Command: "customer.sync",
				Status:  "ok",
			})) {
				accepted.Add(1)
			}
		}()
	}
	wg.Wait()

	events := sink.Events()
	if got, want := len(events), int(accepted.Load()); got != want {
		t.Fatalf("Events() len = %d, want accepted count %d", got, want)
	}
	if got, want := sink.Dropped()+accepted.Load(), uint64(count); got != want {
		t.Fatalf("Dropped()+accepted = %d, want %d", got, want)
	}
}

func traceSinkAssertEvent(t *testing.T, event TraceEvent, name TraceEventName, recordedAt time.Time) {
	t.Helper()

	if event.Name != name {
		t.Fatalf("event.Name = %q, want %q", event.Name, name)
	}
	if !event.RecordedAt.Equal(recordedAt) {
		t.Fatalf("event.RecordedAt = %v, want %v", event.RecordedAt, recordedAt)
	}
}
