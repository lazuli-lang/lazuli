package lazuli

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

	// Validators are reusable validators called via `validate @validator.X`
	// or `let ... = @validator.X`. Order matches the DSL.
	Validators []ValidatorRef

	// Effect is the side-effect applied transactionally after policy and
	// validators pass. Exactly one effect per command in v0.
	Effect Effect

	// Emits lists the events published after the surrounding transaction
	// commits.
	Emits []EventEmit

	// EmitsTrace lists `event.trace` publications. Trace events don't enter
	// the reaction graph.
	EmitsTrace []EventTraceEmit

	// Invalidates lists the queries whose cached results become stale after
	// this command succeeds. Entries follow the DSL form: same-feature
	// `query.<name>` short form or fully qualified `<feature>.query.<name>`.
	Invalidates []string

	// untouched generic erasure marker for registry storage
	_ struct{}
}

// erased returns the type-erased view used by the runtime dispatcher. Generated
// code does not call this directly.
func (c *Command[I, O]) erased() *commandErased {
	return &commandErased{
		Name:        c.Name,
		Resource:    c.Resource,
		Policy:      c.Policy,
		RateLimit:   c.RateLimit,
		Audit:       c.Audit,
		Validators:  c.Validators,
		Effect:      c.Effect,
		Emits:       c.Emits,
		EmitsTrace:  c.EmitsTrace,
		Invalidates: c.Invalidates,
	}
}

// commandErased is the runtime's view of any Command[I, O].
type commandErased struct {
	Name        string
	Resource    any
	Policy      Policy
	RateLimit   RateLimit
	Audit       *AuditSpec
	Validators  []ValidatorRef
	Effect      Effect
	Emits       []EventEmit
	EmitsTrace  []EventTraceEmit
	Invalidates []string
}
