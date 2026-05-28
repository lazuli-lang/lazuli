package lazuli

import (
	"context"

	"lazuli.dev/runtime/lazuli/jobs"
)

// ExternalCallRef is the lowered `calls <slot>.<op>` reference shared with
// jobs so integration adapters see one shape across command and job bodies.
type ExternalCallRef = jobs.ExternalCallRef

// RetryPolicy is the lowered `retry <count> backoff <strategy>` block shared
// with jobs.
type RetryPolicy = jobs.RetryPolicy

// ApprovalThen is the closed catalog of approval timeout resolutions.
type ApprovalThen string

const (
	ApprovalThenDeny     ApprovalThen = "deny"
	ApprovalThenAllow    ApprovalThen = "allow"
	ApprovalThenEscalate ApprovalThen = "escalate"
)

// ApprovalSpec is the lowered command approval contract.
//
// W4 GAP-06 widened the single approver (`By`) to an ordered approval chain
// (`Chain`) with an optional `Sequential` flag. `By` is retained for
// back-compat and always equals `Chain[0]`; the runtime gate requires each
// approver in `Chain` (in order when `Sequential`).
type ApprovalSpec struct {
	Then       ApprovalThen
	By         string
	Reason     string
	Chain      []string
	Sequential bool
}

// Approvers returns the ordered approver atoms — `Chain` when populated,
// otherwise the single `By` approver (pre-W4 single-approver shape).
func (s ApprovalSpec) Approvers() []string {
	if len(s.Chain) == 0 {
		return []string{s.By}
	}
	return s.Chain
}

// IdempotencyKey is the lowered `idempotency by <path>` directive.
type IdempotencyKey struct {
	Path string
}

// Deprecation is the lowered command deprecation marker.
type Deprecation struct {
	Since       string
	Replacement string
	Sunset      string
}

// TransitionAdvance is one edge in a lifecycle transition chain.
// Set on Command[I, O].Transitions when the .lzi declares
// `triggers transition <...>`. Runtime validates the first From
// matches the current state (FOR UPDATE), runs the mutation, then
// updates lifecycle_state to the last To.
type TransitionAdvance struct {
	From string
	To   string
}

// Command is a write operation declared by the DSL. Type parameter I is the
// input shape (the `input` block); O is the output shape (the resource the
// command creates/updates, or a custom record).
//
// Generated code populates this once per command at package init. The runtime
// dispatches HTTP requests to `Handle()` after registration.
type Command[I, O any] struct {
	// Name is the canonical command name as written in the DSL
	// (e.g. "customer.create"). Always qualified by feature.
	Name string

	// WithSource stamps the originating .lzi command onto a request context.
	// Codegen emits this hook so runtime error constructors can recover source
	// metadata without bucket adapters manually threading it.
	WithSource func(context.Context) context.Context

	// Resource is the resource the command primarily mutates. The runtime
	// uses it for tenancy enforcement, soft-delete handling, and audit
	// scope resolution.
	Resource any

	// Policy resolves which actors may invoke. Empty means the command is
	// unreachable until a policy is set (per the DSL invariant).
	Policy Policy

	// RateLimit is the throttle policy. Empty disables rate limiting and
	// triggers the strict-profile diagnostic at compile time.
	RateLimit RateLimit

	// Audit declares whether and what to record. nil means no audit.
	Audit *AuditSpec

	// Approval declares an approval gate for the command. nil means no
	// approval is required.
	Approval *ApprovalSpec

	// Validators are reusable validators called via `validate @validator.X`
	// or `let ... = @validator.X`. Order matches the DSL.
	Validators []ValidatorRef

	// Effect is the side-effect applied transactionally after policy and
	// validators pass. Exactly one effect per command in v0.
	Effect Effect

	// Transitions chain. Empty = no lifecycle gate; non-empty = pre-guard
	// on Transitions[0].From + post-update to Transitions[last].To, both
	// in the same tx as Effect.
	Transitions []TransitionAdvance

	// Emits lists the events published after the surrounding transaction
	// commits.
	Emits []EventEmit

	// EmitsTrace lists `event.trace` publications. Trace events don't enter
	// the reaction graph.
	EmitsTrace []EventTraceEmit

	// Invalidates lists the queries whose cached results become stale
	// after this command succeeds. Entries are the wire registry key
	// `<feature>.<name>` (matches Query.Name exactly). Lazuli cell B1
	// dropped the historical `.query.` infix; same-feature shorthand
	// (`query.<name>` in source) is expanded by codegen to the full
	// `<feature>.<name>` form before reaching this list.
	Invalidates []string

	// ExternalCalls lists integration calls declared by the command body.
	ExternalCalls []ExternalCallRef

	// Timeout is the duration literal (`"30s"`, `"5m"`) for command execution.
	Timeout string

	// Retry declares the attempt count + backoff strategy for external calls.
	Retry *RetryPolicy

	// Idempotency declares the request dedupe key path.
	Idempotency *IdempotencyKey

	// Deprecation declares lifecycle metadata for generated API/OpenAPI
	// surfaces. nil means the command is live.
	Deprecation *Deprecation

	// ErrorKeys binds per-command framework-error translation keys per
	// proposal §2.A (resolution-chain layer 1). Codegen emits a
	// `var <command>ErrorKeys = lazuli.ErrorKeys{ ... }` literal and
	// passes `&<command>ErrorKeys` here when the command declares
	// `policy ... when_denied @translation.<key>`. nil means no
	// per-command override — the resolver falls through to the per-
	// feature `FeatureErrorContract`, then to the built-in catalog.
	ErrorKeys *ErrorKeys

	// untouched generic erasure marker for registry storage
	_ struct{}
}

// erased returns the type-erased view used by the runtime dispatcher. Generated
// code does not call this directly.
func (c *Command[I, O]) erased() *commandErased {
	return &commandErased{
		Name:          c.Name,
		WithSource:    c.WithSource,
		Resource:      c.Resource,
		Policy:        c.Policy,
		RateLimit:     c.RateLimit,
		Audit:         c.Audit,
		Approval:      c.Approval,
		Validators:    c.Validators,
		Effect:        c.Effect,
		Transitions:   c.Transitions,
		Emits:         c.Emits,
		EmitsTrace:    c.EmitsTrace,
		Invalidates:   c.Invalidates,
		ExternalCalls: c.ExternalCalls,
		Timeout:       c.Timeout,
		Retry:         c.Retry,
		Idempotency:   c.Idempotency,
		Deprecation:   c.Deprecation,
		ErrorKeys:     c.ErrorKeys,
	}
}

// commandErased is the runtime's view of any Command[I, O].
type commandErased struct {
	Name          string
	WithSource    func(context.Context) context.Context
	Resource      any
	Policy        Policy
	RateLimit     RateLimit
	Audit         *AuditSpec
	Approval      *ApprovalSpec
	Validators    []ValidatorRef
	Effect        Effect
	Transitions   []TransitionAdvance
	Emits         []EventEmit
	EmitsTrace    []EventTraceEmit
	Invalidates   []string
	ExternalCalls []ExternalCallRef
	Timeout       string
	Retry         *RetryPolicy
	Idempotency   *IdempotencyKey
	Deprecation   *Deprecation
	ErrorKeys     *ErrorKeys
}
