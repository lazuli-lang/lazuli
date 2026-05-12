package lazuli

import (
	"log/slog"
	"net/http"
	"runtime/debug"
)

// RecoverMiddleware catches panics in downstream handlers, logs them with a
// stack trace via slog, and returns 500 Internal Server Error. The original
// request stays in the log for correlation.
//
// /healthz is intentionally not recovered so liveness panics surface to the
// orchestrator instead of creating noisy panic loops.
func RecoverMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path == "/healthz" {
			next.ServeHTTP(w, r)
			return
		}

		defer func() {
			if rec := recover(); rec != nil {
				slog.Error("lazuli: panic in handler",
					"panic", rec,
					"method", r.Method,
					"path", r.URL.Path,
					"stack", string(debug.Stack()),
				)
				http.Error(w, "internal server error", http.StatusInternalServerError)
			}
		}()

		next.ServeHTTP(w, r)
	})
}
