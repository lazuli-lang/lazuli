// Typed emission helpers for the four built-in trace events
// reserved by the IR (`agent_run`, `command_run`, `job_run`,
// `webhook_run`). Generated code calls `Emit*Run` at the right
// boundary; the adapter (OTel / file / stdout) consumes.
//
// Each payload struct mirrors `crates/lazuli_ir::built_in_trace_events()`
// field-for-field. The runtime records recent events in-memory here;
// async flushing and adapter dispatch remain follow-up work.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.5 §6.4 and
// `docs/proposals/ai-primitives-cut-a-8.md` for the foundational
// `agent_run` precedent.

package observability

import (
	"context"
	"time"
)

// AgentRunPayload mirrors the IR-reserved `agent_run` event. Cut A.8
// (commit `ac0241d`) shipped the language slot; this struct is the
// runtime-side mirror.
type AgentRunPayload struct {
	Agent          string // qualified `<feature>.<agent_name>`
	Flow           string // populated when Cut B.flow lands
	FlowStep       string // populated when Cut B.flow lands
	Model          string // `@llm.<name>` resolved at runtime
	FinishReason   string // "stop" | "length" | "tool_calls" | "safety" | ...
	TokensInput    int64
	TokensOutput   int64
	TokensTotal    int64
	CostUSD        float64
	DurationMS     int64
	PromptVersion  string
	Tools          []ToolCall
	SafetyDecision string // "passed" | "blocked" | "redacted"
	Tenant         string
	RequestID      string
	TraceID        string
}

// ToolCall mirrors the inner record consumed by `AgentRunPayload.Tools`.
type ToolCall struct {
	Name       string // qualified tool reference
	Effect     string // "read" | "write"
	DurationMS int64
	Status     string // "ok" | "error" | "rate_limited"
	ErrorKind  string
}

// CommandRunPayload mirrors `command_run` — observability bucket
// cycle row 35.
type CommandRunPayload struct {
	Command    string // qualified `<feature>.<command>`
	Actor      string // `@actor.<kind>` resolved at runtime
	Tenant     string
	Status     string // "ok" | "denied" | "rejected" | "error"
	ErrorCode  string
	DurationMS int64
	RequestID  string
	TraceID    string
}

// JobRunPayload mirrors `job_run` — observability bucket cycle row 35.
type JobRunPayload struct {
	Job            string
	Trigger        string // "event" | "schedule" | "manual"
	Tenant         string
	Status         string // "ok" | "retrying" | "failed" | "dead_lettered"
	Attempt        int
	DurationMS     int64
	IdempotencyKey string
	ErrorCode      string
	RequestID      string
	TraceID        string
}

// WebhookRunPayload mirrors `webhook_run` — observability bucket
// cycle row 35.
type WebhookRunPayload struct {
	Webhook        string
	Tenant         string
	Status         string // "ok" | "verify_failed" | "rejected" | "error"
	SignatureValid bool
	DurationMS     int64
	IdempotencyKey string
	ErrorCode      string
	RequestID      string
	TraceID        string
}

// EmitAgentRun emits an `agent_run` trace event. Generated agent
// dispatch code calls this once per dispatch (or once per `flow`
// step when Cut B lands).
//
// Adapter dispatch remains follow-up work; the recent-event ring is
// populated synchronously.
func EmitAgentRun(ctx context.Context, payload AgentRunPayload) {
	_ = ctx
	RecordTraceEvent(NewAgentRunTraceEvent(payload))
	// TODO(runtime): hand off to the configured adapter; never block
	// the caller's hot path.
}

// EmitCommandRun emits a `command_run` trace event. Called once per
// command dispatch from generated handler code, regardless of
// HTTP/RPC/event trigger.
func EmitCommandRun(ctx context.Context, payload CommandRunPayload) {
	_ = ctx
	RecordTraceEvent(NewCommandRunTraceEvent(payload))
	// TODO(runtime): same dispatch as `EmitAgentRun`.
}

// EmitJobRun emits a `job_run` trace event. Called once per job
// invocation (success, retry, or failure) by the queue adapter
// wrapper.
func EmitJobRun(ctx context.Context, payload JobRunPayload) {
	_ = ctx
	RecordTraceEvent(NewJobRunTraceEvent(payload))
	// TODO(runtime): correlate `Attempt` with the retry chain so
	// observers can reconstruct without a separate join.
}

// EmitWebhookRun emits a `webhook_run` trace event. Called once per
// inbound webhook delivery by the verify-and-dispatch middleware.
func EmitWebhookRun(ctx context.Context, payload WebhookRunPayload) {
	_ = ctx
	RecordTraceEvent(NewWebhookRunTraceEvent(payload))
	// TODO(runtime): `SignatureValid` is populated from the HMAC
	// verifier; never block the response on adapter flush.
}

// MarkStart returns the current monotonic time. Generated code uses
// this at the entry point of each trace boundary so `DurationMS` is
// computed without a clock-skew foot-gun.
func MarkStart() time.Time {
	return time.Now()
}

// DurationMS returns the elapsed time since `start` in milliseconds.
func DurationMS(start time.Time) int64 {
	return time.Since(start).Milliseconds()
}
