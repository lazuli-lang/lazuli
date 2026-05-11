// Tracing contract stubs. The language declares `app.tracing` intent
// (sample rate, propagation, exporter slot); the runtime owns span
// allocation, context propagation, and adapter dispatch. Adapter
// packages (`@runtime/otel`, `@plugin/datadog/tracer`, etc.) ship the
// concrete exporter wiring.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.2 §6.2.

package observability

import (
	"context"
	"errors"
)

// TracingContract is the lowered `app.tracing` block from `app.lzi`.
// Codegen emits `var TracingContract = observability.TracingContract{...}`
// per app.
type TracingContract struct {
	// Propagate toggles trace-context propagation through downstream
	// calls. `false` keeps span capture but disables context export.
	Propagate bool
	// SampleRate is the head sampling rate in `[0.0, 1.0]`.
	SampleRate float64
	// Exporter is the resolved adapter slot name from
	// `registry.capabilities <name>: tracing`. Empty means "no
	// adapter; runtime picks a default (no-op or stdout)".
	Exporter string
}

// SpanContract is the per-span control surface returned by
// `StartSpan`. Generated code calls `End()` to close the span and
// `SetError(err)` to mark the span as failed.
type SpanContract interface {
	End()
	SetError(err error)
	SetAttribute(key string, value any)
}

// TracerContract is the per-process tracer materialised at boot from
// `TracingContract`. Generated code receives one via `NewTracer`.
type TracerContract struct {
	// Contract is the source-of-truth shape.
	Contract TracingContract
	// Enabled mirrors `SampleRate > 0.0`; allows hot-path skip-checks
	// without rereading the float.
	Enabled bool
}

// Typed errors.
var (
	// ErrTracerNotConfigured fires when `TracingContract.Exporter`
	// names an adapter slot that doesn't resolve at boot. Doctor's
	// `app_tracing_exporter_unbound_diagnostics` closes the gap at
	// compile time; this is the runtime safety net.
	ErrTracerNotConfigured = errors.New("lazuli/observability: tracer_not_configured")
)

// NewTracer materialises a `TracerContract` from a `TracingContract`.
// Stub: full implementation (resolve adapter slot, build OTel
// `TracerProvider`, register propagator) lands with the runtime team.
func NewTracer(ctx context.Context, contract TracingContract) (TracerContract, error) {
	_ = ctx
	// TODO(runtime): resolve the exporter adapter, build the
	// `TracerProvider`, register the propagator, and wrap the
	// returned tracer with sampling.
	return TracerContract{
		Contract: contract,
		Enabled:  contract.SampleRate > 0.0,
	}, nil
}

// StartSpan allocates a span on the active tracer. Returns a noop
// span if the tracer is disabled.
//
// Stub: full implementation lands with the runtime team.
func StartSpan(ctx context.Context, tracer TracerContract, name string) (context.Context, SpanContract) {
	_ = tracer
	_ = name
	// TODO(runtime): if disabled, return a typed noop span. Otherwise
	// call the adapter tracer.
	return ctx, noopSpan{}
}

// Trace is a convenience for `StartSpan` + `End()` around a closure.
// Generated code uses this for per-handler tracing without a manual
// `defer span.End()` site.
func Trace(ctx context.Context, tracer TracerContract, name string, fn func(context.Context) error) error {
	ctx, span := StartSpan(ctx, tracer, name)
	defer span.End()
	if err := fn(ctx); err != nil {
		span.SetError(err)
		return err
	}
	return nil
}

// noopSpan is the typed disabled-span returned when tracing is off.
// Concrete adapters supply their own implementation.
type noopSpan struct{}

func (noopSpan) End()                       {}
func (noopSpan) SetError(err error)         { _ = err }
func (noopSpan) SetAttribute(_ string, _ any) {}
