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
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"runtime/debug"

	"lazuli.dev/runtime/lazuli"
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
				if !lazuli.PanicRecoverFromContext(r.Context()) {
					panic(rec)
				}
				base, typedErr := typedPanicError(rec)
				emitPanicTrace(r.Context(), ScopeHTTPCommand, base, typedErr, debug.Stack())
				envelope := buildHTTPEnvelope(r.Context(), base, typedErr)
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(envelope.Status)
				_ = json.NewEncoder(w).Encode(envelope.Body)
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
			if !lazuli.PanicRecoverFromContext(ctx) {
				panic(rec)
			}
			stack := debug.Stack()
			base, typedErr := typedPanicError(rec)
			emitPanicTrace(ctx, scope, base, typedErr, stack)
			panicErr = &PanicError{Recovered: rec, Stack: stack, Scope: scope, Err: typedErr}
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
	Err       error
}

// Error implements the error interface.
func (e *PanicError) Error() string {
	return "lazuli/observability: panic recovered"
}

// Unwrap returns the typed lazuli error constructed from the panic.
func (e *PanicError) Unwrap() error { return e.Err }

type httpEnvelope struct {
	Status int
	Body   map[string]interface{}
}

func typedPanicError(rec any) (lazuli.ErrorBase, error) {
	switch v := rec.(type) {
	case *lazuli.Error:
		return normalizeBase(v.Base, v.Error()), v
	case *lazuli.FieldError:
		return normalizeBase(v.Base, v.Error()), v
	case *lazuli.PolicyError:
		return normalizeBase(v.Base, v.Error()), v
	case *lazuli.TenantError:
		return normalizeBase(v.Base, v.Error()), v
	case *lazuli.AdapterError:
		return normalizeBase(v.Base, v.Error()), v
	case *lazuli.LibBugError:
		return normalizeBase(v.Base, v.Error()), v
	case error:
		if base, typed := typedFromErrorChain(v); typed != nil {
			return normalizeBase(base, typed.Error()), typed
		}
		base := lazuli.ErrorBase{
			Code:    "internal_panic",
			Surface: lazuli.SurfaceLibInternal,
			Status:  http.StatusInternalServerError,
			Message: v.Error(),
		}
		return base, &lazuli.LibBugError{
			Base:      base,
			Component: "unknown",
			Invariant: v.Error(),
		}
	default:
		base := lazuli.ErrorBase{
			Code:    "internal_panic",
			Surface: lazuli.SurfaceLibInternal,
			Status:  http.StatusInternalServerError,
			Message: "non-error panic value",
		}
		return base, &lazuli.LibBugError{
			Base:      base,
			Component: "unknown",
			Invariant: fmt.Sprintf("panic value was not an error: %T", rec),
		}
	}
}

func typedFromErrorChain(err error) (lazuli.ErrorBase, error) {
	var lz *lazuli.Error
	if errors.As(err, &lz) {
		return lz.Base, lz
	}
	var fe *lazuli.FieldError
	if errors.As(err, &fe) {
		return fe.Base, fe
	}
	var pe *lazuli.PolicyError
	if errors.As(err, &pe) {
		return pe.Base, pe
	}
	var te *lazuli.TenantError
	if errors.As(err, &te) {
		return te.Base, te
	}
	var ae *lazuli.AdapterError
	if errors.As(err, &ae) {
		return ae.Base, ae
	}
	var lb *lazuli.LibBugError
	if errors.As(err, &lb) {
		return lb.Base, lb
	}
	return lazuli.ErrorBase{}, nil
}

func normalizeBase(base lazuli.ErrorBase, fallbackMessage string) lazuli.ErrorBase {
	if base.Code == "" {
		base.Code = "internal_panic"
	}
	if base.Status == 0 {
		base.Status = http.StatusInternalServerError
	}
	if base.Message == "" {
		base.Message = fallbackMessage
	}
	return base
}

func emitPanicTrace(ctx context.Context, scope PanicScope, base lazuli.ErrorBase, typedErr error, stack []byte) {
	EmitTraceEvent(ctx, scopeTraceEventName(scope), map[string]interface{}{
		"capsule": base.Capsule,
		"feature": base.Feature,
		"kind":    base.Kind,
		"op":      base.Op,
		"source":  base.Source,
		"surface": base.Surface.String(),
		"stack":   string(stack),
	})

	if base.Surface == lazuli.SurfaceLibInternal || base.Surface == lazuli.SurfaceCodegenBug {
		issueURL := ""
		var lb *lazuli.LibBugError
		if errors.As(typedErr, &lb) {
			issueURL = lb.IssueURL
		}
		EmitTraceEvent(ctx, "lazuli_internal_panic", map[string]interface{}{
			"surface":   base.Surface.String(),
			"component": componentFromTyped(typedErr),
			"issue_url": issueURL,
		})
	}
}

func scopeTraceEventName(scope PanicScope) string {
	switch scope {
	case ScopeJobWorker:
		return "job_run.panic"
	case ScopeWebhookHandler:
		return "webhook_run.panic"
	default:
		return "command_run.panic"
	}
}

func buildHTTPEnvelope(ctx context.Context, base lazuli.ErrorBase, typed error) httpEnvelope {
	status := base.Status
	if status == 0 {
		status = http.StatusInternalServerError
	}
	errorBody := map[string]interface{}{
		"code":    base.Code,
		"message": base.Message,
		"surface": base.Surface.String(),
	}
	if shouldIncludeSource(ctx) && base.Source != "" {
		errorBody["source"] = base.Source
	}

	var fe *lazuli.FieldError
	if errors.As(typed, &fe) {
		errorBody["field"] = fe.Field
		errorBody["path"] = fe.Path
		errorBody["reason"] = fe.Reason.String()
	}
	var pe *lazuli.PolicyError
	if errors.As(typed, &pe) {
		errorBody["rule"] = pe.Rule
		errorBody["subject"] = pe.Subject
		errorBody["resource"] = pe.Resource
		errorBody["tenant"] = pe.Tenant
	}
	var te *lazuli.TenantError
	if errors.As(typed, &te) {
		errorBody["axis"] = te.Axis
		errorBody["expected"] = te.Expected
		errorBody["actual"] = te.Actual
	}
	var ae *lazuli.AdapterError
	if errors.As(typed, &ae) {
		errorBody["adapter"] = ae.Adapter
		errorBody["adapter_op"] = ae.Op
		errorBody["retry_budget_consumed"] = ae.RetryBudgetConsumed
		errorBody["retry_budget_max"] = ae.RetryBudgetMax
	}

	return httpEnvelope{
		Status: status,
		Body: map[string]interface{}{
			"error": errorBody,
		},
	}
}

func shouldIncludeSource(ctx context.Context) bool {
	env := lazuli.EnvironmentFromContext(ctx)
	allowed := lazuli.ObservabilityErrorSourcesFromContext(ctx)
	for _, a := range allowed {
		if a == env {
			return true
		}
	}
	return false
}

func componentFromTyped(err error) string {
	var lb *lazuli.LibBugError
	if errors.As(err, &lb) {
		return lb.Component
	}
	return "unknown"
}
