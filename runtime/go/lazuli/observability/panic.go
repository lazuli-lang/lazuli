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
	"sync"
	"time"
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

// PanicReport is the payload sent to a configured panic reporter when
// Lazuli recovers a panic at a runtime boundary.
type PanicReport struct {
	// Recovered is the value returned by recover().
	Recovered any
	// Stack is the goroutine stack captured at the recovery site.
	Stack []byte
	// Scope names the runtime boundary that recovered the panic.
	Scope PanicScope
	// RequestPath is the HTTP request path when the panic came from
	// RecoverHTTP. It is empty for non-HTTP scopes.
	RequestPath string
	// RequestMethod is the HTTP method when the panic came from
	// RecoverHTTP. It is empty for non-HTTP scopes.
	RequestMethod string
	// Time is when the panic was recovered.
	Time time.Time
}

// PanicReporter receives recovered panic reports. Implementations
// should return quickly and must not rely on being called
// asynchronously.
type PanicReporter interface {
	ReportPanic(context.Context, PanicReport)
}

// PanicReporterFunc adapts a function to PanicReporter.
type PanicReporterFunc func(context.Context, PanicReport)

// ReportPanic implements PanicReporter.
func (f PanicReporterFunc) ReportPanic(ctx context.Context, report PanicReport) {
	if f == nil {
		return
	}
	f(ctx, report)
}

var panicReporterRegistry struct {
	sync.RWMutex
	reporter PanicReporter
}

// SetPanicReporter configures the process-wide panic reporter. Passing
// nil disables reporting. Reporter panics are ignored so recovery
// behavior remains unchanged.
func SetPanicReporter(reporter PanicReporter) {
	panicReporterRegistry.Lock()
	panicReporterRegistry.reporter = reporter
	panicReporterRegistry.Unlock()
}

// ResetPanicReporter clears the process-wide panic reporter.
func ResetPanicReporter() {
	SetPanicReporter(nil)
}

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
				ctx := context.Background()
				report := newPanicReport(rec, stack, ScopeHTTPCommand)
				if r != nil {
					ctx = r.Context()
					report.RequestMethod = r.Method
					if r.URL != nil {
						report.RequestPath = r.URL.Path
					}
				}
				reportPanic(ctx, report)
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
			report := newPanicReport(rec, stack, scope)
			reportPanic(ctx, report)
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

func newPanicReport(rec any, stack []byte, scope PanicScope) PanicReport {
	return PanicReport{
		Recovered: rec,
		Stack:     append([]byte(nil), stack...),
		Scope:     scope,
		Time:      time.Now().UTC(),
	}
}

func reportPanic(ctx context.Context, report PanicReport) {
	panicReporterRegistry.RLock()
	reporter := panicReporterRegistry.reporter
	panicReporterRegistry.RUnlock()
	if reporter == nil {
		return
	}
	if ctx == nil {
		ctx = context.Background()
	}
	defer func() {
		_ = recover()
	}()
	reporter.ReportPanic(ctx, report)
}
