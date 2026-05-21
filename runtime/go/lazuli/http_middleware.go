package lazuli

import "net/http"

// Middleware wraps an http.Handler with extra behavior, such as auth, rate
// limiting, recovery, or logging.
type Middleware func(http.Handler) http.Handler

// Chain applies the middlewares in declaration order. The first listed
// middleware wraps the outside entry point for the request, and the last listed
// middleware is closest to the inner handler.
//
//	handler := lazuli.Chain(
//		lazuli.RequestIDMiddleware,
//		lazuli.RecoverMiddleware,
//		lazuli.RateLimitMiddleware(lazuli.RateLimitFromDefault("60 per minute per ip")),
//	)(routerHandler)
func Chain(mws ...Middleware) Middleware {
	return func(next http.Handler) http.Handler {
		for i := len(mws) - 1; i >= 0; i-- {
			next = mws[i](next)
		}
		return next
	}
}

// MiddlewareFunc adapts a func(http.Handler) http.Handler to Middleware.
type MiddlewareFunc func(http.Handler) http.Handler
