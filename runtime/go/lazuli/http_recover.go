package lazuli

import (
	"fmt"
	"log/slog"
	"net/http"
	"runtime/debug"
)

// panicString renders a recovered panic value as a string for the dev-only
// detail block. errors render via Error(); everything else via %v.
func panicString(rec any) string {
	if err, ok := rec.(error); ok {
		return err.Error()
	}
	return fmt.Sprintf("%v", rec)
}

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
				stack := string(debug.Stack())
				slog.Error("lazuli: panic in handler",
					"panic", rec,
					"method", r.Method,
					"path", r.URL.Path,
					"stack", stack,
				)
				// W4-5 / panic-envelope: return the STRUCTURED error
				// envelope ({"code":"internal",...}, 500) instead of raw
				// text so clients/agents get the same shape as every other
				// 5xx. In the dev allow-list env we also surface the panic
				// value + stack in the detail block; prod stays masked.
				le := &Error{
					Status:     http.StatusInternalServerError,
					Code:       CodeInternal,
					Message:    "internal server error",
					MessageKey: CodeInternal,
				}
				payload := map[string]any{
					"code":    le.Code,
					"message": le.Message,
				}
				if devSessionEnabled() {
					payload["detail"] = map[string]any{
						"panic":   panicString(rec),
						"stack":   stack,
						"surface": SurfaceLibInternal.String(),
					}
				}
				writeJSON(w, http.StatusInternalServerError, payload)
			}
		}()

		next.ServeHTTP(w, r)
	})
}
