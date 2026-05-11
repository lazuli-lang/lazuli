// Package observability implements the runtime side of the Lazuli
// observability bucket cycle (rows 35-37). The language declares
// `app.logging`, `app.tracing`, audit `emit_to`, `event.trace` levels,
// and reserves 4 built-in trace events (`agent_run`, `command_run`,
// `job_run`, `webhook_run`); this package owns the dispatchers and
// typed errors. Concrete exporters (OpenTelemetry, Datadog, slog
// handlers) sit in `@runtime/...` or `@plugin/...` adapter packages
// and bind via `registry.capabilities` resolution at boot.
//
// This file ships the logging contract: signatures + struct contracts
// + sentinels. Full implementation (slog handler stack, PII
// redaction, sampling) is owned by the runtime team — these stubs
// only commit to the contract shape the codegen will call.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.1 §6.1.
package observability

import (
	"context"
	"errors"
	"log/slog"
)

// LogLevel mirrors the `app.logging.level` closed catalog. The
// language enforces the catalog at compile time via
// `app_logging_level_invalid_diagnostics`; this enum is the runtime-
// side mirror so generated code references stable Go constants
// instead of raw strings.
type LogLevel int

const (
	// LogLevelDebug — verbose tracing, local development default.
	LogLevelDebug LogLevel = iota
	// LogLevelInfo — production default.
	LogLevelInfo
	// LogLevelWarn — recoverable errors and degraded conditions.
	LogLevelWarn
	// LogLevelError — request failures and exceptions.
	LogLevelError
)

// String renders the level using the canonical Lazuli token.
func (l LogLevel) String() string {
	switch l {
	case LogLevelDebug:
		return "debug"
	case LogLevelInfo:
		return "info"
	case LogLevelWarn:
		return "warn"
	case LogLevelError:
		return "error"
	default:
		return "unknown"
	}
}

// SlogLevel maps the Lazuli level to a `slog.Level`. The runtime
// team's implementation reads this when configuring the handler
// stack.
func (l LogLevel) SlogLevel() slog.Level {
	switch l {
	case LogLevelDebug:
		return slog.LevelDebug
	case LogLevelInfo:
		return slog.LevelInfo
	case LogLevelWarn:
		return slog.LevelWarn
	case LogLevelError:
		return slog.LevelError
	default:
		return slog.LevelInfo
	}
}

// LogFormat mirrors the `app.logging.format` closed catalog.
type LogFormat int

const (
	// LogFormatJSON — machine-parseable single-line JSON.
	LogFormatJSON LogFormat = iota
	// LogFormatText — human-readable for local development.
	LogFormatText
)

// RedactStrategy mirrors the `app.logging.redact` closed catalog.
// `RedactPII` auto-strips fields tagged `@pii.*` at codegen time;
// `RedactNone` disables auto-redaction (adapters may still redact).
type RedactStrategy int

const (
	// RedactPII — strip fields tagged `@pii.*`. Default.
	RedactPII RedactStrategy = iota
	// RedactNone — adapter decides.
	RedactNone
)

// LoggingContract is the lowered `app.logging` block from `app.lzi`.
// Codegen emits a `var LoggingContract = observability.LoggingContract{...}`
// per app.
type LoggingContract struct {
	// Level is the minimum severity captured. Records below `Level`
	// are dropped at handler entry.
	Level LogLevel
	// Format selects the slog handler family.
	Format LogFormat
	// Redact controls auto-redaction of `@pii.*` fields.
	Redact RedactStrategy
	// SampleRate is the float `[0.0, 1.0]` from `app.logging.sample_rate`.
	// `0.0` disables capture; `1.0` captures every record. The runtime
	// turns this into a sampling handler.
	SampleRate float64
}

// LoggerContract is the per-call view of `LoggingContract` after the
// adapter has resolved the underlying `slog.Logger`. Generated code
// receives one of these via `NewLogger` and emits records through it.
type LoggerContract struct {
	// Logger is the resolved slog.Logger with the handler stack
	// (format, level, sampling, redaction) already applied.
	Logger *slog.Logger
	// Contract is the source-of-truth shape that produced this logger
	// (kept for introspection / hot-reload).
	Contract LoggingContract
}

// Typed errors. Logging is best-effort; the only failure modes the
// runtime surfaces are configuration mistakes that doctor should
// have caught.
var (
	// ErrLogRedactedField indicates a generated codegen site
	// attempted to log a field that the runtime's PII registry
	// rejected at observation time. Defensive — doctor's
	// `app_logging_redact_unknown_diagnostics` and
	// `agent_run_subscriber_payload_drift_diagnostics` close the gap
	// at compile time, but the runtime keeps the safety net.
	ErrLogRedactedField = errors.New("lazuli/observability: log_redacted_field")
)

// NewLogger materialises a `LoggerContract` from a `LoggingContract`.
// Stub: full implementation (handler chain, sampling middleware, PII
// redaction handler keyed by `PiiFieldRegistry`) lands with the
// runtime team.
//
// Signature is committed; callers in generated code may import
// without waiting for the body.
func NewLogger(ctx context.Context, contract LoggingContract) (LoggerContract, error) {
	_ = ctx
	// TODO(runtime): build slog.Handler stack:
	//   - base handler picks `Format` (slog.NewJSONHandler vs
	//     slog.NewTextHandler).
	//   - wrapping `slog.LevelVar` reads `contract.Level`.
	//   - wrapping `redactingHandler` consults the generated
	//     `PiiFieldRegistry` to strip `@pii.*` fields when
	//     `contract.Redact == RedactPII`.
	//   - wrapping `samplingHandler` honors `contract.SampleRate`.
	return LoggerContract{Contract: contract}, nil
}
