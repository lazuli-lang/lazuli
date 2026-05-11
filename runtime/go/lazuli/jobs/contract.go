// Package jobs implements the runtime side of the Lazuli `job` block.
// The language declares the job contract (trigger, idempotency, retry,
// timeout, tenant_from, fanout, external calls, handler/declarative
// body); this package owns dispatch + retry + error contract. Concrete
// queue adapters (River, Asynq) sit in `@runtime/...` packages and bind
// via `@adapter.queue.*` resolution at boot.
//
// Phase L Tier 3 / row 33 — this file ships the typed contract shape
// the codegen emits per job. Full dispatch implementation lives in
// `dispatch.go`; retry helpers in `retry.go`.
package jobs

import (
	"context"
	"errors"
	"time"
)

// JobTrigger discriminates between event-reactor and scheduled jobs.
type JobTrigger struct {
	// Kind is `event` or `schedule`.
	Kind string
	// Event is `feature.event` when Kind == "event"; empty otherwise.
	Event string
	// Cron is the schedule expression when Kind == "schedule"; empty
	// otherwise.
	Cron string
}

// BackoffStrategy names a closed-catalog retry backoff strategy.
// `fixed` waits a constant interval between attempts; `exponential`
// doubles with jitter (adapter-specific).
type BackoffStrategy string

const (
	BackoffFixed       BackoffStrategy = "fixed"
	BackoffExponential BackoffStrategy = "exponential"
)

// RetryPolicy is the lowered `retry <count> backoff <strategy>` block.
type RetryPolicy struct {
	Count   uint32
	Backoff BackoffStrategy
}

// IdempotencyKeySpec is the lowered `idempotency by <path>` directive.
// Path is a dot-segmented expression evaluated against the job
// envelope (e.g. `payload.batch_id`, `envelope.id`).
type IdempotencyKeySpec struct {
	Path string
}

// TenantFromSpec is the lowered `tenant_from <path>` directive used by
// jobs / webhooks / notifications to pin a tenant context from the
// inbound payload.
type TenantFromSpec struct {
	Path string
}

// FanoutSpec is the lowered `fanout tenants <axis>` directive. Today
// `Scope` is always `tenants`; the field exists for forward
// compatibility with future fanout scopes.
type FanoutSpec struct {
	Scope string
	Axis  string
}

// ExternalCallRef is the lowered `calls <slot>.<op>` reference. The
// codegen emits one per call; the runtime threads them through the
// integration adapter resolver with the timeout / retry the job
// declares.
type ExternalCallRef struct {
	Slot      string
	Operation string
	Args      []string
}

// JobContract is the lowered `job <name>` shape. The codegen emits
// `var <FeatureCamel>Job<NameCamel>Contract = jobs.JobContract{...}`
// per job; dispatchers read it to wire trigger, retry, idempotency,
// tenant resolution, fanout, external-call adapters, and the handler
// callback.
type JobContract struct {
	// Feature is the owning feature name (e.g. `customer`).
	Feature string
	// Name is the job identifier.
	Name string
	// Trigger discriminates reactor vs scheduled.
	Trigger JobTrigger
	// Queue is the queue lane for event-triggered jobs. Empty means
	// run inline.
	Queue string
	// TenantFrom pins the per-execution tenant context. Nil means the
	// job is not multi-tenant scoped.
	TenantFrom *TenantFromSpec
	// Fanout declares per-tenant expansion for scheduled jobs.
	Fanout *FanoutSpec
	// Idempotency declares the dedupe key path.
	Idempotency *IdempotencyKeySpec
	// Retry declares attempt count + backoff strategy.
	Retry *RetryPolicy
	// Timeout is the per-execution timeout (`"30s"`, `"5m"`).
	Timeout time.Duration
	// ExternalCalls lists `calls <slot>.<op>` references the job body
	// dispatches through the integration adapters.
	ExternalCalls []ExternalCallRef
	// Policy is the policy reference (e.g. `@actor.system`).
	Policy string
	// HandlerPath is the `./path.go` handler reference for
	// handler-backed jobs. Empty when the body is declarative.
	HandlerPath string
	// Emits lists the events the job publishes on success.
	Emits []string
}

// Typed errors returned by the job dispatcher. Mapped to standard
// `expose client` HTTP status codes when surfaced through the
// observability/audit pipeline.
var (
	// ErrJobTimeout is returned when the job exceeds its declared
	// `timeout`.
	ErrJobTimeout = errors.New("jobs: timeout")
	// ErrJobMaxRetries is returned when the retry budget is exhausted.
	ErrJobMaxRetries = errors.New("jobs: retry budget exhausted")
	// ErrJobFanoutInvalid is returned when fanout cannot enumerate
	// tenants (axis missing, tenant lookup failed).
	ErrJobFanoutInvalid = errors.New("jobs: fanout enumeration failed")
	// ErrJobTenantUnresolved is returned when `tenant_from` can't be
	// resolved against the inbound envelope.
	ErrJobTenantUnresolved = errors.New("jobs: tenant_from unresolved")
)

// JobEnvelope is the runtime payload threaded through every job
// execution. Mirrors the canonical `envelope.id` / `payload.<fields>`
// the DSL exposes; the codegen builds it from the trigger event or
// the schedule fire context.
type JobEnvelope struct {
	ID       string
	Tenant   string
	Payload  map[string]any
	Metadata map[string]string
}

// HandlerFunc is the signature the codegen wires for handler-backed
// jobs. Returns nil on success; the runtime classifies errors against
// the retry policy.
type HandlerFunc func(ctx context.Context, envelope JobEnvelope) error
