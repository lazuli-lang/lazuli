# Observability Bucket Wire-Thin Audit

- **Date:** 2026-05-14
- **Auditor:** Codex
- **Source commit:** `2781f42eb7915f4e35cb20a20748c4d52d99a02d`
- **Bucket:** `runtime/go/lazuli/observability/`

---

## File Table

| File | Eff. LOC | External Imports | Verdict | Target Library | Est. Cell |
|---|---|---|---|---|---|
| `audit.go` | 46 | _none_ | **wire** | - | - |
| `health.go` | 134 | `github.com/jackc/pgx/v5/pgxpool` | **wire** | - | - |
| `logging.go` | 145 | _none_ | **questionable** | - | - |
| `panic.go` | 240 | `lazuli.dev/runtime/lazuli` | **wire** | - | - |
| `pprof_labels.go` | 23 | `lazuli.dev/runtime/lazuli` | **wire** | - | - |
| `trace_emit.go` | 107 | _none_ | **questionable** | - | - |
| `tracing.go` | 121 | `go.opentelemetry.io/otel`, `go.opentelemetry.io/otel/exporters/otlp/otlptrace/otlptracehttp`, `go.opentelemetry.io/otel/propagation`, `go.opentelemetry.io/otel/sdk/resource`, `go.opentelemetry.io/otel/sdk/trace`, `go.opentelemetry.io/otel/semconv/v1.26.0` | **wire** | - | - |

### logging.go note

`logging.go` is 145 effective LOC with zero counted external imports, but the implementation is mainly the Lazuli-side mirror of the closed `app.logging` vocabulary plus stdlib `log/slog` handler selection and field redaction. There is no clear OSS library to replace the enum/catalog bridge without moving policy out of the language contract. Verdict: **questionable but acceptable**; keep it under review if sampling, fanout, or remote exporters grow here instead of moving into `@runtime/otel` or provider plugins.

### trace_emit.go note

`trace_emit.go` is 107 effective LOC with zero counted external imports. The size comes from payload structs that mirror IR-reserved events (`agent_run`, `command_run`, `job_run`, `webhook_run`) plus a tiny process-local sink used by tests and early adapters. This is framework-specific lowering glue, not a homegrown tracing engine. Verdict: **questionable but acceptable**; any buffering/export pipeline should remain adapter wiring and not expand this file into a custom telemetry backend.

---

## Summary

**5/7 files (71.4%) are wire-clean.** No file is a clear rewrite-as-wire violation and no file is a delete-candidate. Two files are acceptable questionable cases because they are Lazuli vocabulary bridges with zero counted external imports and more than 100 effective LOC.

### Top 3 risks for downstream product ports

1. **Logging scope creep** - `logging.go` is currently a thin `slog` bridge, but product ports like Pleiades or Hostpoint will quickly want sampling, fanout, correlation, and exporter routing. Those features should wire existing handler/exporter packages or move provider-specific behavior to `@plugin/<name>`, not accumulate in this bucket.

2. **Trace event dispatch is still a stub** - `trace_emit.go` commits the generated-code payload shape but does not yet provide durable buffering or async flush. Atelier/Erudito agent-heavy paths could lose observability unless the consuming runtime binds a real OTel or event sink before relying on these events operationally.

3. **Panic envelopes are framework-coupled** - `panic.go` is wire-clean because it depends on Lazuli typed errors, but it is 240 effective LOC and owns HTTP response shaping. Hostpoint-style ports need this behavior tested across generated command/job/webhook boundaries so typed error fields, source stripping, and trace emission stay aligned.

---

## Wire-Thin Scorecard

| Metric | Value |
|---|---|
| Files audited | 7 |
| Wire-clean | 5 (71.4%) |
| Rewrite-as-wire | 0 |
| Questionable | 2 (`logging.go`, `trace_emit.go`) |
| Delete-candidate | 0 |
| Codex cells generated | 0 |
| Hostpoint-blocker risk | Low (no hard rewrite required before consumption) |
