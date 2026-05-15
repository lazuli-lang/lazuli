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

// Invoke runs the configured handler under the plan-gate prelude
// (PG.C.2). Routers can call `Invoke` instead of calling `Handler`
// directly so behind-gates short-circuit with the standard 402
// envelope and quota counters advance after a successful call.
func (a *Api[I, O]) Invoke(ctx *Ctx, input I) (O, error) {
	var zero O
	if a == nil || a.Handler == nil {
		return zero, &Error{Status: 500, Code: CodeInternal,
			Message: "api handler not set: " + a.Name}
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
