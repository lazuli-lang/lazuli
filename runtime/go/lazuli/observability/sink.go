package observability

import (
	"context"
	"sync"
	"sync/atomic"
	"time"
)

const defaultMemoryTraceSinkCapacity = 1024

// TraceEventName is one of the runtime-reserved built-in trace event names.
//
// EXPERIMENTAL: subject to change before 1.0
type TraceEventName string

const (
	// TraceEventAgentRun is emitted once per agent dispatch.
	TraceEventAgentRun TraceEventName = "agent_run"
	// TraceEventCommandRun is emitted once per command dispatch.
	TraceEventCommandRun TraceEventName = "command_run"
	// TraceEventJobRun is emitted once per job invocation.
	TraceEventJobRun TraceEventName = "job_run"
	// TraceEventWebhookRun is emitted once per inbound webhook delivery.
	TraceEventWebhookRun TraceEventName = "webhook_run"
)

// TraceEvent is the runtime envelope for one built-in trace event.
//
// Exactly one payload pointer should be populated and Name should match it.
// Constructors below produce that shape for the four reserved trace events.
//
// EXPERIMENTAL: subject to change before 1.0
type TraceEvent struct {
	Name       TraceEventName
	RecordedAt time.Time

	AgentRun   *AgentRunPayload
	CommandRun *CommandRunPayload
	JobRun     *JobRunPayload
	WebhookRun *WebhookRunPayload
}

// TraceSink accepts trace events without blocking the caller's hot path.
//
// EXPERIMENTAL: subject to change before 1.0
type TraceSink interface {
	// TryEmit attempts to enqueue event and returns whether it was accepted.
	// A false result is counted by Dropped.
	TryEmit(ctx context.Context, event TraceEvent) bool
	// Dropped returns the number of events this sink declined to accept.
	Dropped() uint64
}

// NewAgentRunTraceEvent returns an agent_run trace event envelope.
//
// EXPERIMENTAL: subject to change before 1.0
func NewAgentRunTraceEvent(payload AgentRunPayload) TraceEvent {
	payload = cloneAgentRunPayload(payload)
	return TraceEvent{
		Name:     TraceEventAgentRun,
		AgentRun: &payload,
	}
}

// NewCommandRunTraceEvent returns a command_run trace event envelope.
//
// EXPERIMENTAL: subject to change before 1.0
func NewCommandRunTraceEvent(payload CommandRunPayload) TraceEvent {
	return TraceEvent{
		Name:       TraceEventCommandRun,
		CommandRun: &payload,
	}
}

// NewJobRunTraceEvent returns a job_run trace event envelope.
//
// EXPERIMENTAL: subject to change before 1.0
func NewJobRunTraceEvent(payload JobRunPayload) TraceEvent {
	return TraceEvent{
		Name:   TraceEventJobRun,
		JobRun: &payload,
	}
}

// NewWebhookRunTraceEvent returns a webhook_run trace event envelope.
//
// EXPERIMENTAL: subject to change before 1.0
func NewWebhookRunTraceEvent(payload WebhookRunPayload) TraceEvent {
	return TraceEvent{
		Name:       TraceEventWebhookRun,
		WebhookRun: &payload,
	}
}

// MemoryTraceSink is a bounded in-memory TraceSink safe for concurrent use.
//
// The zero value is ready to use. When the sink is full or briefly busy,
// TryEmit drops the new event and increments Dropped instead of waiting.
//
// EXPERIMENTAL: subject to change before 1.0
type MemoryTraceSink struct {
	// Clock returns the current time. Defaults to time.Now when nil.
	Clock func() time.Time

	mu       sync.RWMutex
	capacity int
	events   []TraceEvent
	dropped  atomic.Uint64
}

var _ TraceSink = (*MemoryTraceSink)(nil)

// NewMemoryTraceSink returns an empty bounded in-memory trace sink.
//
// A non-positive capacity uses a small default capacity.
//
// EXPERIMENTAL: subject to change before 1.0
func NewMemoryTraceSink(capacity int) *MemoryTraceSink {
	return &MemoryTraceSink{capacity: capacity}
}

// TryEmit stores event when capacity and lock availability allow it.
//
// EXPERIMENTAL: subject to change before 1.0
func (s *MemoryTraceSink) TryEmit(ctx context.Context, event TraceEvent) bool {
	if s == nil {
		return false
	}
	if traceSinkContextDone(ctx) {
		s.dropped.Add(1)
		return false
	}
	if !s.mu.TryLock() {
		s.dropped.Add(1)
		return false
	}
	defer s.mu.Unlock()

	if len(s.events) >= s.capacityLocked() {
		s.dropped.Add(1)
		return false
	}

	event = cloneTraceEvent(event)
	if event.RecordedAt.IsZero() {
		event.RecordedAt = s.nowLocked().UTC()
	}
	s.events = append(s.events, event)
	return true
}

// Events returns a snapshot of accepted trace events in insertion order.
//
// EXPERIMENTAL: subject to change before 1.0
func (s *MemoryTraceSink) Events() []TraceEvent {
	if s == nil {
		return nil
	}

	s.mu.RLock()
	defer s.mu.RUnlock()

	out := make([]TraceEvent, len(s.events))
	for i, event := range s.events {
		out[i] = cloneTraceEvent(event)
	}
	return out
}

// Dropped returns the number of events this sink declined to accept.
//
// EXPERIMENTAL: subject to change before 1.0
func (s *MemoryTraceSink) Dropped() uint64 {
	if s == nil {
		return 0
	}
	return s.dropped.Load()
}

func (s *MemoryTraceSink) capacityLocked() int {
	if s.capacity > 0 {
		return s.capacity
	}
	return defaultMemoryTraceSinkCapacity
}

func (s *MemoryTraceSink) nowLocked() time.Time {
	if s.Clock != nil {
		return s.Clock()
	}
	return time.Now()
}

func traceSinkContextDone(ctx context.Context) bool {
	if ctx == nil {
		return false
	}
	select {
	case <-ctx.Done():
		return true
	default:
		return false
	}
}

func cloneTraceEvent(event TraceEvent) TraceEvent {
	if event.AgentRun != nil {
		payload := cloneAgentRunPayload(*event.AgentRun)
		event.AgentRun = &payload
	}
	if event.CommandRun != nil {
		payload := *event.CommandRun
		event.CommandRun = &payload
	}
	if event.JobRun != nil {
		payload := *event.JobRun
		event.JobRun = &payload
	}
	if event.WebhookRun != nil {
		payload := *event.WebhookRun
		event.WebhookRun = &payload
	}
	return event
}

func cloneAgentRunPayload(payload AgentRunPayload) AgentRunPayload {
	if payload.Tools == nil {
		return payload
	}
	tools := make([]ToolCall, len(payload.Tools))
	copy(tools, payload.Tools)
	payload.Tools = tools
	return payload
}
