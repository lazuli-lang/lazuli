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
