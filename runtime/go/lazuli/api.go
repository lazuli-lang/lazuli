package lazuli

import "context"

// Api is an HTTP endpoint declared by the DSL. Type parameter I is the
// input shape inferred from the route/path arguments; O is the output shape
// returned by the endpoint handler.
type Api[I, O any] struct {
	Name       string
	Feature    string
	Method     HttpMethod
	Path       string
	Policy     Policy
	RateLimit  RateLimit
	WithSource func(context.Context) context.Context
	Handler    func(ctx *Ctx, input I) (O, error)
	// Prelude carries every `gate behind plan.feature` / `gate quota
	// plan.limit` directive authored on the DSL `api` block. The
	// router runs `RunPrelude(ctx, a.Prelude)` before invoking the
	// handler; successful calls follow up with `RunIncrement` so
	// quota counters advance. Empty / nil slice is the no-gate fast
	// path.
	Prelude []GateRef
}

// Invoke runs the configured handler under the policy contract and
// plan-gate prelude (PG.C.2). Routers call `Invoke` instead of calling
// `Handler` directly so the authorisation contract is enforced,
// behind-gates short-circuit with the standard 402 envelope, and quota
// counters advance after a successful call.
//
// SECURITY (SEC-API-POLICY-NULL): the api dispatch surface MUST enforce
// `a.Policy` identically to the command (`Command.Handle`) and query
// (`run.go` RunList/RunLookup/RunSQL) paths — previously the emitted
// `Policy` field was dead on this path, so an `api ... policy
// @policy.authenticated` was reachable anonymously. EvalPolicyInput runs
// FIRST, before the prelude and the handler, so an unauthorised caller is
// denied (401/403) before any side effect. It fails CLOSED: an empty atom
// list (a codegen invariant violation) returns 500 rather than serving,
// and `@policy.public` resolves to `{scope, public}` which `atomMatches`
// admits — preserving intentionally-public endpoints.
func (a *Api[I, O]) Invoke(ctx *Ctx, input I) (O, error) {
	var zero O
	if a == nil || a.Handler == nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "api handler not set: " + a.Name}
	}
	// 1. policy — pass the typed input so GAP-09 input-value-predicate
	// atoms (`@policy.x when input.y = "z"`) can test their `When` guard,
	// exactly as Command.Handle does.
	if err := EvalPolicyInput(ctx, a.Policy, input); err != nil {
		return zero, err
	}
	if err := RunPrelude(ctx, a.Prelude); err != nil {
		return zero, err
	}
	out, err := a.Handler(ctx, input)
	if err == nil {
		_ = RunIncrement(ctx, a.Prelude)
	}
	return out, err
}

// HttpMethod is the HTTP verb for an Api endpoint.
type HttpMethod string

const (
	MethodGet    HttpMethod = "GET"
	MethodPost   HttpMethod = "POST"
	MethodPut    HttpMethod = "PUT"
	MethodPatch  HttpMethod = "PATCH"
	MethodDelete HttpMethod = "DELETE"
)
