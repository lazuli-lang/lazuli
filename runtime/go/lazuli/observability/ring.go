package observability

import (
	"sync"
	"time"
)

const (
	// DefaultTraceRingCapacity is the number of recent trace events retained by
	// the process-wide ring used by Emit*Run helpers.
	DefaultTraceRingCapacity = 1024
)

// TraceEventName identifies a trace event stream.
//
// EXPERIMENTAL: subject to change before 1.0.
type TraceEventName string

const (
	// TraceEventAgentRun is the reserved built-in agent dispatch event.
	TraceEventAgentRun TraceEventName = "agent_run"
	// TraceEventCommandRun is the reserved built-in command dispatch event.
	TraceEventCommandRun TraceEventName = "command_run"
	// TraceEventJobRun is the reserved built-in job invocation event.
	TraceEventJobRun TraceEventName = "job_run"
	// TraceEventWebhookRun is the reserved built-in webhook delivery event.
	TraceEventWebhookRun TraceEventName = "webhook_run"
)

// TraceEventPayload carries one typed built-in trace payload or a field map for
// authored/custom trace events.
//
// EXPERIMENTAL: subject to change before 1.0.
type TraceEventPayload struct {
	AgentRun   *AgentRunPayload
	CommandRun *CommandRunPayload
	JobRun     *JobRunPayload
	WebhookRun *WebhookRunPayload
	Fields     map[string]any
}

// TraceEvent is one observability trace event retained in a TraceRing.
//
// EXPERIMENTAL: subject to change before 1.0.
type TraceEvent struct {
	Name       TraceEventName
	Payload    TraceEventPayload
	RecordedAt time.Time
}

// TraceRingSnapshot is a point-in-time copy of a TraceRing.
//
// EXPERIMENTAL: subject to change before 1.0.
type TraceRingSnapshot struct {
	Events   []TraceEvent
	Dropped  uint64
	Capacity int
}

// TraceRing keeps the most recent trace events in append order.
//
// EXPERIMENTAL: subject to change before 1.0.
type TraceRing struct {
	mu sync.RWMutex

	events  []TraceEvent
	start   int
	count   int
	dropped uint64
}

var recentTraceRing = struct {
	sync.RWMutex
	ring *TraceRing
}{
	ring: NewTraceRing(DefaultTraceRingCapacity),
}

// NewTraceRing returns an empty ring with the requested capacity. Negative
// capacity is treated as zero. A zero-capacity ring records dropped counts but
// retains no events.
func NewTraceRing(capacity int) *TraceRing {
	if capacity < 0 {
		capacity = 0
	}
	return &TraceRing{
		events: make([]TraceEvent, capacity),
	}
}

// NewTraceEvent returns a generic trace event for authored/custom trace
// streams.
func NewTraceEvent(name TraceEventName, fields map[string]any) TraceEvent {
	return TraceEvent{
		Name: name,
		Payload: TraceEventPayload{
			Fields: fields,
		},
	}
}

// NewAgentRunTraceEvent returns a trace event for the built-in agent_run
// stream.
func NewAgentRunTraceEvent(payload AgentRunPayload) TraceEvent {
	return TraceEvent{
		Name: TraceEventAgentRun,
		Payload: TraceEventPayload{
			AgentRun: &payload,
		},
	}
}

// NewCommandRunTraceEvent returns a trace event for the built-in command_run
// stream.
func NewCommandRunTraceEvent(payload CommandRunPayload) TraceEvent {
	return TraceEvent{
		Name: TraceEventCommandRun,
		Payload: TraceEventPayload{
			CommandRun: &payload,
		},
	}
}

// NewJobRunTraceEvent returns a trace event for the built-in job_run stream.
func NewJobRunTraceEvent(payload JobRunPayload) TraceEvent {
	return TraceEvent{
		Name: TraceEventJobRun,
		Payload: TraceEventPayload{
			JobRun: &payload,
		},
	}
}

// NewWebhookRunTraceEvent returns a trace event for the built-in webhook_run
// stream.
func NewWebhookRunTraceEvent(payload WebhookRunPayload) TraceEvent {
	return TraceEvent{
		Name: TraceEventWebhookRun,
		Payload: TraceEventPayload{
			WebhookRun: &payload,
		},
	}
}

// Add appends event to r. When the ring is full, Add overwrites the oldest
// retained event and increments the dropped count.
func (r *TraceRing) Add(event TraceEvent) {
	if r == nil {
		return
	}
	if event.RecordedAt.IsZero() {
		event.RecordedAt = time.Now().UTC()
	}
	event = traceRingCloneEvent(event)

	r.mu.Lock()
	defer r.mu.Unlock()

	if len(r.events) == 0 {
		r.dropped++
		return
	}
	if r.count < len(r.events) {
		r.events[(r.start+r.count)%len(r.events)] = event
		r.count++
		return
	}

	r.events[r.start] = event
	r.start = (r.start + 1) % len(r.events)
	r.dropped++
}

// Snapshot returns retained events in oldest-to-newest order plus the number of
// events dropped because the ring did not have capacity.
func (r *TraceRing) Snapshot() TraceRingSnapshot {
	if r == nil {
		return TraceRingSnapshot{}
	}

	r.mu.RLock()
	defer r.mu.RUnlock()

	events := make([]TraceEvent, r.count)
	for i := range events {
		events[i] = traceRingCloneEvent(r.events[(r.start+i)%len(r.events)])
	}
	return TraceRingSnapshot{
		Events:   events,
		Dropped:  r.dropped,
		Capacity: len(r.events),
	}
}

// Len returns the number of events currently retained.
func (r *TraceRing) Len() int {
	if r == nil {
		return 0
	}

	r.mu.RLock()
	defer r.mu.RUnlock()
	return r.count
}

// Capacity returns the maximum number of events retained by r.
func (r *TraceRing) Capacity() int {
	if r == nil {
		return 0
	}

	r.mu.RLock()
	defer r.mu.RUnlock()
	return len(r.events)
}

// Dropped returns how many events were not retained because the ring had no
// free capacity.
func (r *TraceRing) Dropped() uint64 {
	if r == nil {
		return 0
	}

	r.mu.RLock()
	defer r.mu.RUnlock()
	return r.dropped
}

// RecordTraceEvent appends event to the process-wide recent trace ring.
func RecordTraceEvent(event TraceEvent) {
	recentTraceRing.RLock()
	ring := recentTraceRing.ring
	recentTraceRing.RUnlock()
	if ring == nil {
		return
	}
	ring.Add(event)
}

// RecentTraceEvents returns a snapshot of the process-wide recent trace ring.
func RecentTraceEvents() TraceRingSnapshot {
	recentTraceRing.RLock()
	ring := recentTraceRing.ring
	recentTraceRing.RUnlock()
	if ring == nil {
		return TraceRingSnapshot{}
	}
	return ring.Snapshot()
}

func traceRingCloneEvent(event TraceEvent) TraceEvent {
	event.Payload = traceRingClonePayload(event.Payload)
	return event
}

func traceRingClonePayload(payload TraceEventPayload) TraceEventPayload {
	if payload.AgentRun != nil {
		agentRun := *payload.AgentRun
		agentRun.Tools = append([]ToolCall(nil), agentRun.Tools...)
		payload.AgentRun = &agentRun
	}
	if payload.CommandRun != nil {
		commandRun := *payload.CommandRun
		payload.CommandRun = &commandRun
	}
	if payload.JobRun != nil {
		jobRun := *payload.JobRun
		payload.JobRun = &jobRun
	}
	if payload.WebhookRun != nil {
		webhookRun := *payload.WebhookRun
		payload.WebhookRun = &webhookRun
	}
	payload.Fields = traceRingCloneFields(payload.Fields)
	return payload
}

func traceRingCloneFields(fields map[string]any) map[string]any {
	if fields == nil {
		return nil
	}

	cloned := make(map[string]any, len(fields))
	for key, value := range fields {
		cloned[key] = traceRingCloneValue(value)
	}
	return cloned
}

func traceRingCloneValue(value any) any {
	switch v := value.(type) {
	case map[string]any:
		return traceRingCloneFields(v)
	case []any:
		cloned := make([]any, len(v))
		for i := range v {
			cloned[i] = traceRingCloneValue(v[i])
		}
		return cloned
	case []byte:
		return append([]byte(nil), v...)
	default:
		return v
	}
}
