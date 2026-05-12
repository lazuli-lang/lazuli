package lazuli

import (
	"net/http"
	"time"
)

// TimeoutMiddleware returns a middleware that limits downstream handler
// execution time to timeout. When the timeout elapses, the client receives
// 503 Service Unavailable with message as the response body, using the
// standard library's timeout handling.
//
// The wrapped handler receives a request context with the same deadline. Writes
// attempted after the timeout fail with http.ErrHandlerTimeout.
//
// A timeout value of zero or less disables the middleware and leaves requests
// untouched.
func TimeoutMiddleware(timeout time.Duration, message string) Middleware {
	return func(next http.Handler) http.Handler {
		if timeout <= 0 {
			return next
		}
		return http.TimeoutHandler(next, timeout, message)
	}
}
