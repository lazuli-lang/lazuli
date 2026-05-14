// Package poller is the Lazuli runtime surface for "persistent cursor"
// async resolution loops authored as `poller <name>` in `.lzi`.
//
// Per docs/proposals/poller-vocab.md §6.2: the runtime is wire over
// stdlib `time.Ticker` + `pgx/v5`. No external scheduler library; no
// FSM library. The poller IS the state machine, expressed in surface
// vocabulary and lowered into pure SQL UPDATEs at the dispatcher.
//
// Codegen emits a `Spec[Row, State, Terminal, Result]` literal per
// authored `poller` and a `RegisterPollers(*Registry)` per feature that
// has any. Boot composes them all into a single `Scheduler`.
package poller

import (
	"context"
	"errors"
	"time"
)

// ── Spec ─────────────────────────────────────────────────────────────────────

// Spec captures one authored `poller` block. The four type parameters
// pin the row type, the handler's intermediate state enum, the terminal
// status enum, and the JSON result payload — codegen knows all four
// from the IR (`source` resource, `<Source>Status` synthetic enum,
// `terminal_status_field` enum, `terminal_result_field` shape).
//
// Spec is intentionally a plain struct: the dispatcher owns the
// scheduling clock, batch SELECTs, conditional UPDATEs, and quirk
// fan-out. Authors override behavior by writing the `Resolve` handler.
type Spec[Row any, State ~string, Terminal ~string, Result any] struct {
	// Name uniquely identifies the poller across the app
	// (`<feature>.<poller_name>`). Used for telemetry / structured logs.
	Name string

	// Source is the SQL table name backing the cursor resource.
	Source string

	// Cursor names the three fields the dispatcher reads/writes per row.
	Cursor Cursor

	// Retry policy (max_attempts + backoff strategy).
	Retry Retry

	// States enumerates the response space; ≥1 must be terminal.
	States []State_[State]

	// Resolve is the per-row handler. The dispatcher provides `ctx` with
	// tenant context already resolved (per TenantFrom). The handler must
	// be idempotent with respect to (row.id, row.attempts) — see
	// docs/proposals/poller-vocab.md §10 risk #1.
	Resolve ResolveFunc[Row, State, Terminal, Result]

	// TerminalStatusField is the resource field receiving the terminal
	// status enum value on resolution; "" means the dispatcher only
	// writes `resolved_at`.
	TerminalStatusField string

	// TerminalResultField is the JSON field receiving the handler's
	// terminal payload; "" means the dispatcher skips the column.
	TerminalResultField string

	// Tick controls the dispatcher loop cadence.
	Tick Tick

	// TenantFrom is the verbatim `row.<axis>_id` path the dispatcher
	// uses to derive the tenant context per row.
	TenantFrom string

	// Idempotency captures the verbatim `row.id, row.attempts`-style
	// key list. The dispatcher splits and reuses for conditional UPDATE
	// WHERE clauses.
	Idempotency string

	// Emits names the events the dispatcher publishes after a row
	// commits a terminal state. Routed via the runtime's eventbus; the
	// dispatcher does not own publishing — it surfaces the names so a
	// boot-time wire-up can connect them.
	Emits []string

	// RetryQuirks are closed-catalog pre-handler hooks. The dispatcher
	// pattern-matches on the concrete type and applies the mutation
	// before re-calling the handler.
	RetryQuirks []Quirk

	// WithSource is an optional hook the codegen wires for source-tag
	// propagation (mirrors the jobs bucket pattern). May be nil.
	WithSource func(ctx context.Context) context.Context
}

// Cursor names the three closed cursor fields on `Source`.
type Cursor struct {
	NextAtField     string
	ResolvedAtField string
	AttemptsField   string
}

// Retry captures the closed-catalog retry policy.
type Retry struct {
	MaxAttempts uint32
	Backoff     Backoff
}

// Backoff is the closed-catalog backoff interface. Concrete impls live
// below: `Fixed`, `Linear`, `Exponential`.
type Backoff interface {
	// NextDelay returns the wait before the next handler invocation
	// for a row that just bumped to `attempts` (zero-indexed counter
	// matching the cursor field's value AFTER increment).
	NextDelay(attempts uint32) time.Duration
}

// Fixed backoff: every retry waits the same duration.
type Fixed struct {
	Base time.Duration
}

// NextDelay returns Fixed.Base for every attempt.
func (f Fixed) NextDelay(_ uint32) time.Duration { return f.Base }

// Linear backoff: delay = Base * attempts (capped if Cap > 0).
type Linear struct {
	Base time.Duration
	Cap  time.Duration
}

// NextDelay returns Base*attempts, capped at Cap when > 0.
func (l Linear) NextDelay(attempts uint32) time.Duration {
	d := l.Base * time.Duration(attempts)
	if l.Cap > 0 && d > l.Cap {
		return l.Cap
	}
	return d
}

// Exponential backoff: delay = Base * 2^(attempts-1), capped at Cap.
type Exponential struct {
	Base time.Duration
	Cap  time.Duration
}

// NextDelay returns Base * 2^(attempts-1), capped at Cap when > 0.
// `attempts == 0` returns 0 (first call has no preceding delay).
//
// Implementation note: a left-shift on `time.Duration` overflows fast
// (Duration is int64 nanoseconds; ~63 shifts wipes the sign bit). We
// shift inside a manual loop and bail out the moment we either reach
// `attempts-1` shifts or exceed Cap.
func (e Exponential) NextDelay(attempts uint32) time.Duration {
	if attempts == 0 {
		return 0
	}
	d := e.Base
	for i := uint32(1); i < attempts; i++ {
		if e.Cap > 0 && d >= e.Cap {
			return e.Cap
		}
		d *= 2
		// Defensive against runaway durations (overflow guard).
		if d < 0 {
			if e.Cap > 0 {
				return e.Cap
			}
			return time.Duration(1<<62)
		}
	}
	if e.Cap > 0 && d > e.Cap {
		return e.Cap
	}
	return d
}

// State_ pairs a state name (typed via `State ~string`) with its kind.
// Underscore suffix avoids the unsuffixed `State` collision with the
// generic param.
type State_[State ~string] struct {
	Name State
	Kind StateKind
}

// StateKind is the closed catalog of state classifications.
type StateKind uint8

const (
	// Initial — exactly one state in `States` carries this kind. When
	// no state carries it, the first listed state is initial.
	Initial StateKind = iota
	// Intermediate — handler-returned intermediate states cause the
	// dispatcher to recompute `next_check_at` via backoff and re-enqueue.
	Intermediate
	// Terminal — handler-returned terminal states are absorbing; the
	// dispatcher writes `resolved_at = NOW()` and freezes the row.
	Terminal
)

// Tick controls the dispatcher loop cadence.
type Tick struct {
	Every time.Duration
	Batch uint32
}

// ResolveFunc is the per-row handler. The author writes the body. The
// runtime calls it once per eligible row per tick. The handler MUST be
// idempotent against re-invocation with the same `(row.id, attempts)`
// pair (scheduler crash recovery — §10 risk #1).
type ResolveFunc[Row any, State ~string, Terminal ~string, Result any] func(
	ctx context.Context,
	row Row,
) (ResolveResult[State, Terminal, Result], error)

// ResolveResult is the discriminated union the handler returns. Set
// `Terminal` to commit a final state; set `Pending` to schedule another
// check. Both nil is an error.
type ResolveResult[State ~string, Terminal ~string, Result any] struct {
	Terminal *TerminalResult[Terminal, Result]
	Pending  *PendingResult[State]
}

// TerminalResult carries a final-state outcome.
type TerminalResult[Terminal ~string, Result any] struct {
	Status Terminal
	Result Result
}

// PendingResult carries an in-progress state observation. Fields are
// optional overrides on the dispatcher's defaults.
type PendingResult[State ~string] struct {
	Status      State
	NextCheckAt *time.Time
	ConsultID   *string
}

// Quirk is the closed-catalog pre-handler hook interface. The
// dispatcher type-switches on the concrete value and applies the
// mutation when the predicate evaluator returns true.
type Quirk interface {
	// kind returns the catalog name for telemetry. Closed-catalog
	// discipline: adding a new Quirk requires extending the dispatcher's
	// type switch.
	kind() string
}

// GenderFlipOnce flips the row's gender field once when `When` matches
// and `CounterField` is below 1. After the mutation the handler is
// re-called immediately (no backoff).
//
// `Predicate` is a closed-form predicate matching the lifecycle/poller
// catalog (e.g. `row.status_v8 == "gender_ambiguous"`). The dispatcher
// evaluates it via the predicate package (out of scope here; the v0
// stub recognises only the equality form needed by the anchoring V8
// example — see Eval in dispatcher.go).
type GenderFlipOnce struct {
	When         string
	CounterField string
	GenderField  string
}

func (GenderFlipOnce) kind() string { return "gender_flip_once" }

// ── Registry ─────────────────────────────────────────────────────────────────

// Registry is the runtime-wide collection of registered Spec values.
// Codegen emits `RegisterPollers(*Registry)` per feature; boot composes
// them all before the dispatcher starts.
//
// Specs are stored as opaque `any` because Go generics can't slot
// differently-parameterised Specs into one homogeneous list. The
// dispatcher type-switches the stored value when fetching to dispatch.
type Registry struct {
	specs []registered
}

// registered carries the opaque Spec value plus a name-stable lookup key.
type registered struct {
	name string
	any  any
}

// Register stores a Spec for later dispatch. Codegen calls one Register
// per authored poller. Idempotent: the dispatcher iterates `specs` in
// registration order at start time.
func Register[Row any, S ~string, T ~string, R any](r *Registry, spec Spec[Row, S, T, R]) {
	if r == nil {
		return
	}
	r.specs = append(r.specs, registered{name: spec.Name, any: spec})
}

// NewRegistry returns an empty Registry ready for `Register` calls.
func NewRegistry() *Registry { return &Registry{} }

// All returns the registered specs in insertion order, as opaque
// `any` values. Callers (typically the dispatcher) type-switch each
// element to its concrete `Spec[Row, S, T, R]`.
func (r *Registry) All() []any {
	out := make([]any, len(r.specs))
	for i, e := range r.specs {
		out[i] = e.any
	}
	return out
}

// ── errors ────────────────────────────────────────────────────────────────────

// ErrResolveResultEmpty fires when a handler returns a ResolveResult
// with neither Terminal nor Pending set. The dispatcher treats it as a
// retryable failure (counter still increments).
var ErrResolveResultEmpty = errors.New("poller: handler returned empty ResolveResult")
