// Panic recovery. Wraps HTTP handlers, job workers, and webhook
// receivers so a panic in user code emits a single typed trace event
// (`command_run` / `job_run` / `webhook_run` with `status: "error"`,
// `error_code: "internal_panic"`) and returns a 500 envelope mapped
// to `lazuli.Error`.
//
// Activates automatically when tracing is enabled. No language
// declaration; the safety net is unconditional.
//
// See `docs/proposals/bucket-observability-cycle.md` §6.6.

package observability

import (
	"context"
	"net/http"
	"runtime/debug"
)

// PanicScope names the boundary that caught the panic. Used to pick
// which `Emit*Run` helper records the trace event.
type PanicScope int

const (
	// ScopeHTTPCommand — panic inside a generated HTTP command
	// handler.
	ScopeHTTPCommand PanicScope = iota
	// ScopeJobWorker — panic inside a job runner.
	ScopeJobWorker
	// ScopeWebhookHandler — panic inside a webhook delivery
	// handler.
	ScopeWebhookHandler
)

// RecoverHTTP wraps an http.Handler so panics emit a typed
// `command_run` trace event and return 500.
//
// Stub: full implementation (slog integration, configurable
// strack-trace inclusion in non-production envs, request_id
// correlation) lands with the runtime team.
func RecoverHTTP(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		defer func() {
			if rec := recover(); rec != nil {
				stack := debug.Stack()
				_ = stack
				// TODO(runtime): build CommandRunPayload with
				// `Status: "error"`, `ErrorCode: "internal_panic"`,
				// `RequestID` from ctx; call EmitCommandRun; write
				// 500 envelope via lazuli.Error.
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusInternalServerError)
				_, _ = w.Write([]byte(`{"error":{"code":"internal_panic"}}`))
			}
		}()
		next.ServeHTTP(w, r)
	})
}

// RecoverScope runs `fn` under panic-recovery for the given scope.
// Used by job workers and webhook receivers; HTTP handlers use
// `RecoverHTTP` (which has the request/response context).
//
// Returns the recovered value as an error when a panic fires; nil
// otherwise.
func RecoverScope(ctx context.Context, scope PanicScope, fn func(context.Context) error) (panicErr error) {
	defer func() {
		if rec := recover(); rec != nil {
			stack := debug.Stack()
			_ = stack
			_ = scope
			// TODO(runtime): build the right `Emit*Run` payload per
			// scope; surface a typed error to the caller.
			panicErr = &PanicError{Recovered: rec, Stack: stack, Scope: scope}
		}
	}()
	return fn(ctx)
}

// PanicError is the typed error returned by `RecoverScope` when a
// panic is caught. Wraps the recovered value plus the stack so
// downstream handlers (queue retry, error response) can decide what
// to do.
type PanicError struct {
	Recovered any
	Stack     []byte
	Scope     PanicScope
}

// Error implements the error interface.
func (e *PanicError) Error() string {
	return "lazuli/observability: panic recovered"
}
