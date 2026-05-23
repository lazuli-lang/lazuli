// Tracing contract stubs. The language declares `app.tracing` intent
// (sample rate, propagation, exporter slot); the runtime owns span
// allocation, context propagation, and adapter dispatch. Adapter
// packages (`@runtime/otel`, `@lazuli/plugin-datadog/tracer`, etc.) ship the
// concrete exporter wiring.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.2 §6.2.

package observability

import (
	"context"
	"errors"
	"fmt"
	"math"
	"strings"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracehttp"
	"go.opentelemetry.io/otel/propagation"
	"go.opentelemetry.io/otel/sdk/resource"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	semconv "go.opentelemetry.io/otel/semconv/v1.26.0"
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

// ConfigureTracing wires the global TracerProvider from the contract.
// Call once at boot. Returns a shutdown fn the caller defers.
func ConfigureTracing(ctx context.Context, contract TracingContract, serviceName string) (shutdown func(context.Context) error, err error) {
	if isNoopTracingExporter(contract.Exporter) {
		tp := sdktrace.NewTracerProvider(sdktrace.WithSampler(sdktrace.NeverSample()))
		configureTracingPropagation(contract.Propagate)
		otel.SetTracerProvider(tp)
		return tp.Shutdown, nil
	}
	if !isOTLPHTTPTracingExporter(contract.Exporter) {
		return nil, fmt.Errorf("%w: %s", ErrTracerNotConfigured, contract.Exporter)
	}

	exporter, err := otlptracehttp.New(ctx)
	if err != nil {
		return nil, err
	}

	tp := sdktrace.NewTracerProvider(
		sdktrace.WithBatcher(exporter),
		sdktrace.WithResource(resource.NewWithAttributes(
			semconv.SchemaURL,
			semconv.ServiceName(serviceName),
		)),
		sdktrace.WithSampler(tracingSampler(contract.SampleRate)),
	)
	configureTracingPropagation(contract.Propagate)
	otel.SetTracerProvider(tp)
	return tp.Shutdown, nil
}

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

func (noopSpan) End()                         {}
func (noopSpan) SetError(err error)           { _ = err }
func (noopSpan) SetAttribute(_ string, _ any) {}

func configureTracingPropagation(propagate bool) {
	if !propagate {
		otel.SetTextMapPropagator(propagation.NewCompositeTextMapPropagator())
		return
	}
	otel.SetTextMapPropagator(propagation.NewCompositeTextMapPropagator(
		propagation.TraceContext{},
		propagation.Baggage{},
	))
}

func tracingSampler(sampleRate float64) sdktrace.Sampler {
	switch {
	case math.IsNaN(sampleRate) || sampleRate <= 0:
		return sdktrace.NeverSample()
	case sampleRate >= 1:
		return sdktrace.AlwaysSample()
	default:
		return sdktrace.TraceIDRatioBased(sampleRate)
	}
}

func isNoopTracingExporter(exporter string) bool {
	switch normalizeTracingExporter(exporter) {
	case "", "noop":
		return true
	default:
		return false
	}
}

func isOTLPHTTPTracingExporter(exporter string) bool {
	switch normalizeTracingExporter(exporter) {
	case "otel", "opentelemetry", "otlp", "otlp-http", "otlphttp", "otlptracehttp":
		return true
	default:
		return false
	}
}

func normalizeTracingExporter(exporter string) string {
	return strings.ToLower(strings.TrimSpace(exporter))
}
