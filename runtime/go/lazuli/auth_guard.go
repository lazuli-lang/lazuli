package lazuli

import "context"

const CodeUnauthenticated = "unauthenticated"

// AuthGuard returns the canonical typed unauthenticated envelope for handlers.
func AuthGuard(ctx *Ctx) error {
	if ctx == nil || ctx.User == nil {
		return unauthenticatedError()
	}
	return nil
}

func unauthenticatedError() *Error {
	return &Error{
		Status:     401,
		Code:       CodeUnauthenticated,
		Message:    "authentication required",
		MessageKey: CodeUnauthenticated,
		Base: ErrorBase{
			Status:  401,
			Code:    CodeUnauthenticated,
			Message: "authentication required",
		},
	}
}

func ctxContext(ctx *Ctx) context.Context {
	if ctx != nil && ctx.Context != nil {
		return ctx.Context
	}
	return context.Background()
}
