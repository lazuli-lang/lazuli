// Package observability implements the runtime side of the Lazuli
// observability bucket cycle (rows 35-37). The language declares
// `app.logging`, `app.tracing`, audit `emit_to`, `event.trace` levels,
// and reserves 4 built-in trace events (`agent_run`, `command_run`,
// `job_run`, `webhook_run`); this package owns the dispatchers and
// typed errors. Concrete exporters (OpenTelemetry, Datadog, slog
// handlers) sit in `@runtime/...` or `@lazuli/plugin-...` adapter packages
// and bind via `registry.capabilities` resolution at boot.
//
// This file ships the logging contract: signatures + struct contracts
// + sentinels, plus the stdlib slog handler wiring used by the Go
// runtime. Export fanout and sampling remain adapter/runtime concerns.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.1 §6.1.
package observability

import (
	"context"
	"errors"
	"log/slog"
	"os"
	"strings"
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

var defaultRedactKeys = []string{
	"password",
	"secret",
	"token",
	"api_key",
	"authorization",
	"cookie",
}

// LoggingContract is the lowered `app.logging` block from `app.lzi`.
// Codegen emits a `var LoggingContract = observability.LoggingContract{...}`
// per app.
type LoggingContract struct {
	// Level is the minimum severity captured. Records below `Level`
	// are dropped at handler entry.
	Level LogLevel
	// Format selects the slog handler family.
	Format LogFormat
	// Redact controls log-field redaction. Generated code currently
	// emits a RedactStrategy; runtime callers may pass []string to add
	// custom redacted keys.
	Redact any
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

// NewJSONHandler returns a slog JSON handler writing to stdout with
// Lazuli's default sensitive keys plus caller-provided keys redacted.
func NewJSONHandler(level slog.Level, redactKeys []string) slog.Handler {
	return slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{
		Level:       level,
		ReplaceAttr: redactingReplaceAttr(redactKeys),
	})
}

// Configure installs the process-wide default slog logger described by
// contract.
func Configure(contract LoggingContract) {
	slog.SetDefault(slog.New(newHandler(contract)))
}

// NewLogger materialises a `LoggerContract` from a `LoggingContract`.
func NewLogger(ctx context.Context, contract LoggingContract) (LoggerContract, error) {
	_ = ctx
	return LoggerContract{
		Logger:   slog.New(newHandler(contract)),
		Contract: contract,
	}, nil
}

func newHandler(contract LoggingContract) slog.Handler {
	level := contract.Level.SlogLevel()
	redactKeys, redactEnabled := redactKeysFromContract(contract.Redact)
	options := &slog.HandlerOptions{Level: level}
	if redactEnabled {
		options.ReplaceAttr = redactingReplaceAttr(redactKeys)
	}

	switch contract.Format {
	case LogFormatText:
		return slog.NewTextHandler(os.Stdout, options)
	default:
		if redactEnabled {
			return NewJSONHandler(level, redactKeys)
		}
		return slog.NewJSONHandler(os.Stdout, options)
	}
}

func redactKeysFromContract(redact any) ([]string, bool) {
	switch v := redact.(type) {
	case nil:
		return nil, true
	case RedactStrategy:
		if v == RedactNone {
			return nil, false
		}
		return nil, true
	case []string:
		return v, true
	default:
		return nil, true
	}
}

func redactingReplaceAttr(redactKeys []string) func([]string, slog.Attr) slog.Attr {
	redact := make(map[string]struct{}, len(defaultRedactKeys)+len(redactKeys))
	for _, key := range defaultRedactKeys {
		addRedactKey(redact, key)
	}
	for _, key := range redactKeys {
		addRedactKey(redact, key)
	}

	return func(groups []string, a slog.Attr) slog.Attr {
		_ = groups
		if _, ok := redact[strings.ToLower(a.Key)]; ok {
			a.Value = slog.StringValue("[REDACTED]")
			return a
		}
		// TODO(@lazuli/plugin-pii-scan): once the root lazuli -> observability import
		// cycle is split, wrap direct hot sites too: http.go writeError,
		// handle_db_errors.go classifyDBError, and eventbus.go subscriber errors.
		switch a.Value.Kind() {
		case slog.KindString:
			a.Value = slog.StringValue(Active().Redact(a.Value.String()))
		case slog.KindAny:
			if err, ok := a.Value.Any().(error); ok {
				a.Value = slog.StringValue(Active().Redact(err.Error()))
			}
		}
		return a
	}
}

func addRedactKey(redact map[string]struct{}, key string) {
	key = strings.ToLower(strings.TrimSpace(key))
	if key == "" {
		return
	}
	redact[key] = struct{}{}
}
