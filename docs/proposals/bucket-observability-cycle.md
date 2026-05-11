# Bucket Cycle: Observability (L0→L2)

**Status**: design proposal. Stages 3–9 of the `bucket=observability`
pipeline. Implementation deferred to a separate run with
`mode=implement`.

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-10.

**Pilot bucket**: observability is bucket-piloto #4 in
`docs/roadmap.md` §0 — the last of the four pilots that prove the
L0→L2 cycle before horizontal expansion. The Cut A.8 `agent_run`
trace event (commit `ac0241d`) is the first language-side artifact
that exercises this surface; this proposal designs the rest of the
language contract the runtime team will consume.

## Contexto

The canonical fixture authors **five distinct observability axes**
today, each at a different L-level:

1. **`event.trace <name>`** — authored 4× in the fixture
   (`examples/full-capsule/full-capsule.lzi:196, 742, 808, 811`) as
   the language-level signal for telemetry events that sit outside
   the reaction graph. Parser, IR (`EventKind::Trace` at
   `crates/lazuli_ir/src/lib.rs:707-710`), doctor
   (`event_trace_trigger_diagnostics`,
   `crates/lazuli_lsp/src/lib.rs:4894-4956`), inspect
   (`--expand=events`), and LSP all cover it. **L1 complete**.

2. **`agent_run` built-in trace event** — Cut A.8 shipped commit
   `ac0241d`. IR registry at
   `crates/lazuli_ir/src/lib.rs:760-810`; two doctor diagnostics
   (`event_trace_reserved_name_diagnostics`,
   `agent_run_subscriber_payload_drift_diagnostics`); LSP-aware;
   `--expand=events` projects `built_in_trace_events[]`. **L1
   complete**, runtime instrumentation is parallel Lazuli Go runtime work.

3. **`audit` child on commands/queries/jobs/webhooks** — authored
   once in the fixture (`full-capsule.lzi:271`), specified in
   `docs/invariants.md:93-97`, IR-parsed
   (`crates/lazuli_cli/src/main.rs:3324-3360`), inspect-projected
   under `--expand=summary` (visible in
   `features[].commands[].audit`), runtime spec struct exists
   (`runtime/go/lazuli/audit.go`). **No runtime emitter exists**:
   nothing in `runtime/go/lazuli/handle.go` reads `AuditSpec` and
   writes an audit record. **L1 in language, missing L2**.

4. **`app.lzi runtime` units with `healthcheck` / `readiness`** —
   authored at `app.lzi:76-89`, IR
   (`AppRuntimeUnit { healthcheck, readiness }` at
   `crates/lazuli_ir/src/lib.rs:1356-1366`), one LSP doctor
   (`api unit needs healthcheck/readiness`, lsp:8871-8877), inspect
   exposes the paths. Runtime mounts a hardcoded `GET /healthz`
   (`runtime/go/lazuli/http.go:28-30`) but **does not read the
   declared paths**. **L1 in language, partial L2**.

5. **`capability tracing optional` + `propagate trace_id`** —
   `registry.lzi:18` declares the tracing capability; `app.lzi:72`
   propagates `trace_id`/`request_id`/`tenant`/`actor`. Runtime
   `Ctx.RequestID` / `Ctx.TraceID` exist (`ctx.go:30-33`). No
   adapter, no exporter, no span wrapping. **L0 in language, missing
   L1 cross-checks + L2**.

Beyond these five, the runtime ships **one log line per HTTP
request** (`slog.Info` in `loggingMiddleware`, http.go:178-184). No
structured spans, no metrics, no `runtime/metrics`, no
`/debug/pprof`, no panic reporter, no `agent_run` emitter yet
(Cut A.8 reserves the language slot; the runtime side is the
parallel runtime cut).

The closed-cycle criterion is the §0 8-item checklist (fixture +
check + inspect + doctor lint + generate Go + Lazuli Go runs + eval/test
+ LSP hover). Most boxes are already ticked for `event.trace` and
`agent_run`; the gaps are in **the surface that connects audit,
health, logging, and tracing to the runtime layer**.

**Boundary discipline reminder**: Lazuli core never names slog, OTel,
Sentry, Datadog, Honeycomb, Tempo, Jaeger, Zipkin, Prometheus, New
Relic. Those are `@adapter.*` / `@runtime/<name>` / `@plugin/...`
references resolved through `registry.capabilities` and
`registry.integrations`. The language declares *what* to observe and
*at what level*; the Lazuli Go runtime wires; adapters export.

## Baseline (Stages 1-2 inventory)

| Construct | Surface | Grammar | IR | Doctor/LSP | Codegen | Runtime | L-level |
|---|---|---|---|---|---|---|---|
| `event.trace <name>` (authored) | `full-capsule.lzi:196, 742, 808, 811` | yes (`docs/grammar.lzi.md:351-432`) | `Event` w/ `EventKind::Trace` (`lazuli_ir/src/lib.rs:707-710`) | doctor `event_trace_trigger_diagnostics` (lsp:4894-4956), inspect via `--expand=events` | none (publish-only) | `EventTraceEmit` exists (event.go:37-41), no sinks | **L1** |
| `agent_run` built-in trace event | n/a (built-in) | reserved | `built_in_trace_events()` (lazuli_ir/src/lib.rs:760-810) | 2 doctor diagnostics + LSP reserved-name | none (runtime emits) | not yet instrumented | **L1 language, L0 runtime** |
| `audit <fields>` child on cmd/qry/job/webhook | `full-capsule.lzi:271` (1×) | yes (grammar.lzi.md:279, :409, :427) | `InspectAudit` (`main.rs:3324-3360`) | partial: `InspectAudit` projected under `--expand=summary` only; no cross-fact diagnostic | none | `AuditSpec` struct (`audit.go:9-21`), **no emitter** | **L1 language, L0 runtime** |
| `app.runtime unit <name>` w/ `healthcheck`/`readiness` | `app.lzi:76-89` | yes | `AppRuntimeUnit` (`lazuli_ir/src/lib.rs:1356-1366`) | LSP `api unit needs healthcheck/readiness` warning (lsp:8871-8877); shape check | none | runtime mounts hardcoded `/healthz`, ignores declared path (http.go:28-30); no `/readyz` | **L1 language, partial L2** |
| `app.deploy rollback on_failed_healthcheck` | `app.lzi:95` | yes | `AppDeploy.rollback` (`lazuli_ir/src/lib.rs:1369-1378`) | LSP shape (lsp:8707-8717) | none | not consumed by runtime | **L1 language, L0 runtime** |
| `capability tracing <name>` in registry | `registry.lzi:18` | yes | `AppCapability` | LSP closed catalog (`tracing` allowed, lsp:8673) | none | none | **L0** |
| `communication propagate trace_id, request_id` | `app.lzi:72`, `workspace.lzi:13` | yes | typed | LSP closed catalog | none | `Ctx.RequestID`/`Ctx.TraceID` populated from headers (ctx.go:30-33) | **L1 language, partial L2** |
| `slog`-style structured logging | not authored | n/a | n/a | n/a | n/a | hardcoded `slog.Info` per HTTP request (http.go:174-186) | **language gap** |
| `metric` / `span` / `profile` | not authored | n/a | n/a | n/a | n/a | n/a | **N (runtime-only per audit §21)** |
| Health/ready endpoint binding to declared path | n/a | n/a | n/a | n/a | n/a | hardcoded `/healthz` (http.go:28); declared `/healthz` + `/readyz` ignored | **wiring gap** |
| Panic reporter | not authored | n/a | n/a | n/a | n/a | none | **DF gap (§2.2)** |
| Build info / version endpoint | `app version "0.1.0"` (`app.lzi:6`) | yes | `App.version` | LSP shape | none | none | **partial L1** |

**Cross-cutting fact**: `agent_run` (Cut A.8) is the canonical
template for built-in trace events. The IR enum `TraceFiresPer`
(`lazuli_ir/src/lib.rs:743-754`) already reserves four variants
(`AgentDispatch`, `FlowStep`, `JobInvocation`, `WebhookDelivery`) but
only `AgentDispatch` is bound today.

## Linguagem proposta (Stage 3)

Surface additions are **deliberately small**. The audit's §21
catalogue (~30 features) is overwhelmingly DF (slog, OTel, pprof,
runtime/metrics, GC metrics, panic reporting) — none of which are
language. The language adds **three thin axes**:

### 3.1 `app.logging` block — declarative log contract

Declares level, format, and per-environment override discipline. The
language fixes the **contract**; the Lazuli Go runtime picks the slog
handler; adapters export.

```lzi
app AcmeCRM
  logging
    level info
    format json
    redact pii
```

Slot rules:

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `level <name>` | optional, default `info` | identifier | **closed**: `debug`, `info`, `warn`, `error` (matches `log/slog.Level`) |
| `format <name>` | optional, default `json` | identifier | **closed**: `json`, `text` |
| `redact <strategy>` | optional, default `pii` | identifier | **closed**: `pii` (auto-redact fields tagged `@pii.*`), `none` |

Profile overrides:

```lzi
profile local
  logging
    level debug
    format text

profile production
  logging
    level info
    format json
```

Justification: `docs/roadmap.md:190` lists `log_level` as the only
language-level observability gap in §1.19. Promoting to a typed
block (instead of `app log_level info`) leaves room for `format` and
`redact` without further surface churn. The `redact pii` slot
consumes existing `@pii.*` annotations — boundary clean.

**What this is not**: it is not a logger factory, not a sink list,
not a sampler config, not a per-feature override. Sampling /
filtering / sink fanout / async batching are all runtime concerns.

### 3.2 `app.tracing` block — declarative tracing contract

Same shape as `logging`. Declares whether spans are emitted, the
sampling **intent** (not the algorithm), and what to propagate.

```lzi
app AcmeCRM
  tracing
    enabled true
    sample_rate 0.1
    propagate trace_id, request_id, tenant, actor
```

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `enabled <bool>` | optional, default `true` | bool | `true`, `false` |
| `sample_rate <float>` | optional, default `1.0` | float in `[0.0, 1.0]` | n/a |
| `propagate <list>` | optional, default `trace_id, request_id` | identifier list | **closed**: `trace_id`, `request_id`, `actor`, `tenant`, `correlation_id` |

The `propagate` list **must be a subset** of the declared
`communication propagate` list at `app.lzi:72` (cross-fact: tracing
cannot propagate a field communication doesn't carry).

Justification: today `communication propagate trace_id, request_id`
encodes propagation **for RPC calls**. Tracing reuses it but needs
its own explicit slot so adapters know the **trace context** vs.
**transport context**. Sample rate is intent-only — the adapter
implements head-vs-tail sampling. The language never names OTel,
W3C TraceContext, Jaeger, Zipkin, Datadog, or any wire format.

**Boundary check**: if a Lazuli project replaced the Lazuli Go
runtime with a Rust runtime, this block still declares the same
intent (sample 10% of
spans, propagate these four fields, on/off). The runtime materializes
it with its own tracer.

### 3.3 `audit emit_to <event_group>` — audit-stream wiring

Today, `audit actor, target.id, input.owner_id` (`full-capsule.lzi:271`)
declares **what** to record but leaves **where it goes** implicit.
The runtime invents a destination. This is exactly the magic
discovery `docs/invariants.md` rules against.

Add an optional **destination** slot:

```lzi
command reassign
  audit actor, target.id, input.owner_id
    emit_to audit_log
```

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `audit <fields...>` | optional, existing | identifier list | n/a |
| `audit emit_to <event_group>` (nested) | optional | identifier (must resolve to an `event_group` declared in the same feature **or** to one of the reserved system groups `audit_log`, `audit_stream`) | partial — system names closed |

Per-feature override at `defaults`:

```lzi
feature customer
  defaults
    audit_emit_to audit_log
```

Justification: `audit` already exists as a language primitive (cut
3, shipped). The missing axis is **typed sink**, so doctor can
cross-check that the destination event group exists and adapters know
where to publish. Without it, every audit record ends up either
ad-hoc or invisible. The reserved name `audit_log` is the analog of
`agent_run` — a built-in stream the runtime emits by default. Authors
who want a custom stream declare an `event_group audit_*` and route
to it.

**This is not** a new event_group concept. It is a **binding axis** on
the existing `audit` primitive, using existing event_group machinery.

### 3.4 `event.trace <name> level <level>` — severity hint

Authored trace events today carry payload but no severity. When an
adapter (OTel, Sentry) consumes them, it has to guess.

```lzi
event.trace customer_webhook_received
  level warn
  external_id: Text
  org_id: ID
```

| Slot | Required | Type | Closed catalog |
|---|---|---|---|
| `level <name>` | optional, default `info` | identifier | **closed**: `debug`, `info`, `warn`, `error` (mirrors §3.1 catalog — single vocabulary across logs & traces) |

Justification: severity is observational, not behavioral —
purely declarative. Removes the need for adapter-side heuristics
("events ending in `_failed` are errors"). Adapters consume the
typed `level` field; default `info` keeps backward compatibility.

### 3.5 Reserved built-in trace events (no source declaration)

Extend `built_in_trace_events()` from one entry (`agent_run`) to
**four**, all backed by the existing `TraceFiresPer` variants that
the IR already reserves (`lazuli_ir/src/lib.rs:743-754`):

| Built-in | Fires per | Reserves payload? |
|---|---|---|
| `agent_run` | `AgentDispatch` | ✓ shipped (Cut A.8) |
| `command_run` | `CommandDispatch` (new variant) | this proposal |
| `job_run` | `JobInvocation` (already reserved) | this proposal |
| `webhook_run` | `WebhookDelivery` (already reserved) | this proposal |

Each follows the **Cut A.8 pattern exactly** (IR registers name +
payload; doctor rejects authored `event.trace <reserved>`;
LSP awareness; inspect projects under
`built_in_trace_events[]`). Payload schemas:

```text
event.trace command_run
  payload
    command: Text required        # qualified "<feature>.<command>"
    actor: Text required          # @actor.<kind>
    tenant: Text optional
    status: Text required         # "ok" | "denied" | "rejected" | "error"
    error_code: Text optional
    duration_ms: Integer required
    request_id: Text optional
    trace_id: Text optional

event.trace job_run
  payload
    job: Text required
    trigger: Text required        # "event" | "schedule" | "manual"
    tenant: Text optional
    status: Text required         # "ok" | "retrying" | "failed" | "dead_lettered"
    attempt: Integer required
    duration_ms: Integer required
    idempotency_key: Text optional
    error_code: Text optional

event.trace webhook_run
  payload
    webhook: Text required
    tenant: Text optional
    status: Text required         # "ok" | "verify_failed" | "rejected" | "error"
    signature_valid: Boolean required
    duration_ms: Integer required
    idempotency_key: Text optional
    error_code: Text optional
```

Each payload is **denominated as a flat shape**, matching the A.8
contract. No nested objects beyond what `agent_run.tools[]` already
needed.

**Why these three and not more**: each maps 1:1 to an authoring
primitive that already exists in the language (`command`, `job`,
`webhook`). Reserving them now means subscriber jobs can rely on the
schema before the runtime emits — same contract-first discipline
that A.8 established. `query_run` is deliberately **out** because
queries are read-mostly and most products won't trace them per-call;
if a pilot wants `query_run`, promote later. Same for `flow_step`
(Cut B reserve, intentionally still gated).

### 3.6 Reserved namespace: `@trace.<name>`

Today event-trace names live in flat scope. As built-ins expand
(§3.5), collision risk grows. Add `@trace.<name>` as a **reference-
only** namespace for **subscriber jobs**:

```lzi
job aggregate_costs
  trigger event.trace agent_run          # still legal
  # or, equivalently:
  trigger @trace.agent_run                # canonical form going forward
```

The flat form remains supported (back-compat). The `@trace.<name>`
form is the canonical reference because it puts trace subscriptions
in the **same shape** as `@policy.*`, `@validator.*`, `@adapter.*`,
`@actor.*` — the LLM cold-read test passes more cleanly when every
external reference uses the same sigil pattern.

Add `@trace` to `is_allowed_reference_namespace` in
`crates/lazuli_lsp/src/lib.rs:2114-2135` (the closed catalog).
Completion in `trigger ` offers both `event.trace <X>` and
`@trace.<X>`.

## IR proposto (Stage 4)

### 4.1 `AppLogging` struct (new)

```rust
// crates/lazuli_ir/src/lib.rs — add after AppDeploy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppLogging {
    /// `info` | `debug` | `warn` | `error`. None = adapter default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// `json` | `text`. None = adapter default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// `pii` | `none`. None = `pii`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redact: Option<String>,
}
```

Position: between `AppDeploy` and `AppRoute`. Add to `App`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub logging: Option<AppLogging>,
```

### 4.2 `AppTracing` struct (new)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppTracing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub propagate: Vec<String>,
}
```

### 4.3 `Audit.emit_to: Option<String>` (extension on existing struct)

`InspectAudit` (`crates/lazuli_cli/src/main.rs:700`) and its IR-side
equivalent need one extra field:

```rust
pub struct InspectAudit {
    pub fields: Vec<String>,        // existing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit_to: Option<String>,    // new
}
```

Default resolution: if absent, the runtime emits to a reserved
`audit_log` stream. Doctor warns when `audit` is declared without
`emit_to` **and** the feature has no `defaults audit_emit_to <X>`.

### 4.4 `Event.level: Option<String>` for `event.trace`

`Event` struct (lazuli_ir/src/lib.rs around line 700) gains:

```rust
/// For `EventKind::Trace` only. None means `info`.
/// Rejected at doctor for `EventKind::Domain`.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub level: Option<String>,
```

### 4.5 Extended `TraceFiresPer` + built-in registry

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceFiresPer {
    AgentDispatch,
    CommandDispatch,    // new — §3.5
    FlowStep,           // existing reserve
    JobInvocation,      // existing reserve, now bound
    WebhookDelivery,    // existing reserve, now bound
}

pub fn built_in_trace_events() -> Vec<BuiltInTraceEvent> {
    vec![
        BuiltInTraceEvent {
            name: "agent_run".to_owned(),
            fires_per: TraceFiresPer::AgentDispatch,
            payload: agent_run_payload(),
        },
        BuiltInTraceEvent {
            name: "command_run".to_owned(),
            fires_per: TraceFiresPer::CommandDispatch,
            payload: command_run_payload(),
        },
        BuiltInTraceEvent {
            name: "job_run".to_owned(),
            fires_per: TraceFiresPer::JobInvocation,
            payload: job_run_payload(),
        },
        BuiltInTraceEvent {
            name: "webhook_run".to_owned(),
            fires_per: TraceFiresPer::WebhookDelivery,
            payload: webhook_run_payload(),
        },
    ]
}
```

`is_reserved_trace_event_name()` now matches four names instead of
one.

### 4.6 Inspect JSON shape

```json
{
  "app": {
    "logging": { "level": "info", "format": "json", "redact": "pii" },
    "tracing": {
      "enabled": true,
      "sample_rate": 0.1,
      "propagate": ["trace_id", "request_id", "tenant", "actor"]
    }
  },
  "features": [
    {
      "name": "customer",
      "commands": [
        {
          "name": "reassign",
          "audit": {
            "fields": ["actor", "target.id", "input.owner_id"],
            "emit_to": "audit_log"
          }
        }
      ],
      "events": [
        {
          "name": "customer_webhook_received",
          "kind": "trace",
          "level": "warn",
          "payload": [/* ... */]
        }
      ]
    }
  ]
}
```

`--expand=events` extended `built_in_trace_events[]`:

```json
{
  "built_in_trace_events": [
    { "name": "agent_run",   "fires_per": "agent_dispatch",   "payload": [/*…*/] },
    { "name": "command_run", "fires_per": "command_dispatch", "payload": [/*…*/] },
    { "name": "job_run",     "fires_per": "job_invocation",   "payload": [/*…*/] },
    { "name": "webhook_run", "fires_per": "webhook_delivery", "payload": [/*…*/] }
  ]
}
```

### 4.7 New cross-refs the analyzer must register

| Edge | Source | Target | Resolution |
|---|---|---|---|
| `app.tracing.propagate` | `AppTracing.propagate[]` | `app.communication.propagate[]` | must be subset; emit `app_tracing_propagate_unknown` for unmatched entries |
| `audit.emit_to` | `InspectAudit.emit_to` | `event_group <name>` in same feature **or** reserved `audit_log` / `audit_stream` | doctor `audit_emit_to_unknown` for unmatched |
| `event.trace.level` | `Event.level` | closed catalog `debug/info/warn/error` | doctor `event_trace_level_unknown` |
| `event.trace agent_run` etc. (4 reserved names) | source token | `is_reserved_trace_event_name()` | doctor `event_trace_reserved_name` (already exists; now matches 4 names) |
| `trigger @trace.<name>` | trigger ref | `built_in_trace_events()` + authored `event.trace` of same name | doctor `trigger_trace_unknown` |
| `app.logging.level` | `AppLogging.level` | closed catalog `debug/info/warn/error` | doctor `app_logging_level_unknown` |
| `profile.logging.*` | profile override | must match base shape | existing profile-override machinery |

### 4.8 Diagnostics list (Stage 8 anchors below; one row each)

| Code | Severity |
|---|---|
| `app_logging_level_unknown` | error |
| `app_logging_format_unknown` | error |
| `app_logging_redact_unknown` | error |
| `app_tracing_sample_rate_range` | error |
| `app_tracing_propagate_unknown` | error |
| `app_tracing_propagate_not_in_communication` | error |
| `audit_emit_to_unknown` | error |
| `audit_missing_emit_to_no_default` | warning |
| `event_trace_level_unknown` | error |
| `event_trace_level_on_domain_event` | error |
| `event_trace_reserved_name` (existing; extended) | error |
| `trigger_trace_unknown` | error |

12 diagnostics total. 8 are net-new; 1 extends an existing one; 3
are profile-aware additions on the closed-catalog axes.

## Codegen proposto (Stage 5)

The codegen surface is **thin** because most observability is
runtime-side adapter wiring. Three generated artifacts:

### 5.1 `dist/go/app/observability.gen.go`

```go
// path: dist/go/app/observability.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package app

import (
    "github.com/lazuli/runtime/go/lazuli"
    "github.com/lazuli/runtime/go/lazuli/observability"
)

// LoggingContract is the lowered `app.logging` block from app.lzi.
var LoggingContract = observability.LoggingContract{
    Level:  observability.LogLevelInfo,
    Format: observability.LogFormatJSON,
    Redact: observability.RedactPII,
}

// TracingContract is the lowered `app.tracing` block from app.lzi.
var TracingContract = observability.TracingContract{
    Enabled:    true,
    SampleRate: 0.1,
    Propagate:  []string{"trace_id", "request_id", "tenant", "actor"},
}

// HealthProbes is the lowered set of runtime-unit probes from
// `app.runtime unit api { healthcheck ... readiness ... }`.
var HealthProbes = observability.HealthProbeSet{
    Liveness:  "/healthz",       // from app.lzi runtime api healthcheck
    Readiness: "/readyz",        // from app.lzi runtime api readiness
}
```

### 5.2 `dist/go/<feature>/trace_subscribers.gen.go` (per feature)

For each `job <name>` with `trigger event.trace <reserved>` or
`trigger @trace.<name>`:

```go
// path: dist/go/customer/trace_subscribers.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer

import (
    "github.com/lazuli/runtime/go/lazuli"
    "github.com/lazuli/runtime/go/lazuli/observability"
)

// AggregateCostsTrigger binds the aggregate_costs job to agent_run
// emissions. The payload schema is the canonical agent_run schema;
// field references in the job body are validated at codegen time.
var AggregateCostsTrigger = observability.TraceSubscriber{
    Name:        "customer.aggregate_costs",
    EventName:   "agent_run",
    Payload:     observability.PayloadAgentRun, // canonical schema marker
}
```

### 5.3 `dist/go/<feature>/audit.gen.go` (per feature with audited ops)

```go
// path: dist/go/customer/audit.gen.go
// Code generated by Lazuli. DO NOT EDIT.
package customer

import "github.com/lazuli/runtime/go/lazuli"

// ReassignAuditSpec mirrors `audit actor, target.id, input.owner_id`
// on command `customer.reassign`. The emit_to slot is "audit_log"
// (reserved built-in stream).
var ReassignAuditSpec = &lazuli.AuditSpec{
    Fields: []string{"actor", "target.id", "input.owner_id"},
    EmitTo: "audit_log",
}
```

The codegen surface is intentionally just declarative constants. All
mechanics — handler init, slog setup, OTel exporter, panic recovery,
audit row insertion — live in `runtime/go/lazuli/observability/`.

## Runtime proposto (Stage 6)

Six new files under `runtime/go/lazuli/observability/`. The boundary
is firm: the language declares **what** to observe; the Lazuli Go
runtime wires **how**; adapters export to concrete sinks.

### 6.1 `runtime/go/lazuli/observability/logging.go`

- **Capability**: configure `log/slog` from the declared
  `LoggingContract`. Build the right `slog.Handler` (text vs JSON),
  level filter, and PII redaction `Handler` (auto-strips fields
  annotated `@pii.*` at codegen time — a `redactingHandler` that
  consults a generated `PiiFieldRegistry`).
- **Lifecycle**: boot-time. Singleton `slog.Logger` shared across
  the process.
- **Config**: `LoggingContract` from `dist/go/app/observability.gen.go`.
  Profile overrides applied at boot.
- **Dependency**: `log/slog` (stdlib).
- **Typed errors**: none — logging never fails the request.

### 6.2 `runtime/go/lazuli/observability/tracing.go`

- **Capability**: tracer initialization, span propagation through
  `Ctx`. Reads `TracingContract`. If
  `TracingContract.Enabled == false`, every span call is a no-op.
- **Lifecycle**: boot-time tracer init; per-request span allocation
  in middleware.
- **Config**: `TracingContract`. Adapter selection
  (`@adapter.tracing` or `@runtime/otel`) lives in
  `registry.integrations`; this file dispatches based on the
  declared adapter.
- **Dependency**: zero in core (interface only). Adapters
  (`@runtime/otel`, `@plugin/datadog/tracer`, etc.) bring
  `go.opentelemetry.io/otel`.
- **Typed errors**:
  - `ErrTracerNotConfigured` (compile-time prevented when
    `app.tracing.enabled true` and no adapter resolves).

### 6.3 `runtime/go/lazuli/observability/health.go`

- **Capability**: mount the declared `HealthProbes.Liveness` /
  `Readiness` paths. Replaces the hardcoded `GET /healthz` in
  `runtime/go/lazuli/http.go:28`. Readiness probes registered
  dependencies (DB pool, event-bus connection, etc.) and returns
  503 when any fail.
- **Lifecycle**: per-request handler.
- **Config**: `HealthProbes` from
  `dist/go/app/observability.gen.go`.
- **Typed errors**:
  - `ErrReadinessUnhealthy` — 503 envelope with a list of failing
    probes.

### 6.4 `runtime/go/lazuli/observability/trace_emit.go`

- **Capability**: emit built-in trace events (`agent_run`,
  `command_run`, `job_run`, `webhook_run`) with the canonical
  payload. Each authoring primitive (agent dispatch, command handle,
  job execute, webhook receive) calls a typed `Emit*Run` helper at
  the right boundary.
- **Lifecycle**: per-emission. The emit is asynchronous (buffered
  channel + background flusher) so it never adds latency to the
  hot path.
- **Config**: the four payload schemas are compile-time constants
  generated from `built_in_trace_events()`.
- **Dependency**: interface; adapters export.
- **Typed errors**: none in fast path; emissions are best-effort.

### 6.5 `runtime/go/lazuli/observability/audit.go` (consumes existing `AuditSpec`)

- **Capability**: read the lowered `AuditSpec.EmitTo`; when
  non-empty, write an audit row at the end of a successful command/
  job/webhook execution. The actor/tenant/timestamp come from
  `Ctx`; the field list comes from `AuditSpec.Fields`; the
  destination from `AuditSpec.EmitTo`.
- **Lifecycle**: hook in `runtime/go/lazuli/handle.go:27` `withTx`
  after the transaction commits.
- **Config**: from generated `AuditSpec` constants.
- **Dependency**: adapter — `@runtime/audit_log` (default) or
  `@plugin/<x>` for SIEM forwarding.
- **Typed errors**:
  - `ErrAuditEmitFailed` — 500 envelope only if a strict
    `strict_audit` runtime mode is set; otherwise logged + retried.

### 6.6 `runtime/go/lazuli/observability/panic.go`

- **Capability**: recover panics in HTTP handlers, jobs, webhooks.
  Emit a single `command_run` / `job_run` / `webhook_run` trace
  with `status: "error"` + `error_code: "internal_panic"`. Return
  500 envelope mapped to `lazuli.Error`.
- **Lifecycle**: middleware (HTTP) + worker wrapper (jobs).
- **Config**: none required; activates automatically when tracing
  is enabled.
- **Dependency**: stdlib `runtime`, `runtime/debug`.

### 6.7 What the Lazuli Go runtime does NOT do

Per the audit (§21 + §32 + §33), the following are **explicitly out
of scope** for this proposal:

- `runtime/metrics`, GC metrics, container-aware GOMAXPROCS,
  scheduler metrics — Lazuli Go stdlib pickups, not surfaced in
  language.
- `/debug/pprof`, `runtime/pprof`, `runtime/trace.FlightRecorder`,
  goroutine-leak profiling — adapter-mounted under
  `@runtime/pprof`, gated by `enable pprof` flag (future).
- OTel exporter logic, Sentry forwarding, Datadog client config —
  adapter `@runtime/otel`, `@plugin/sentry/exporter`, etc.
- Log sampling, log fanout, async batching — adapter detail behind
  the `LoggingContract.Format`/`Level` interface.

The language commits to **three intent axes** (logging, tracing,
audit); the Lazuli Go runtime commits to a stable interface; adapters fill the
implementations. Same boundary discipline as auth/storage/jobs.

## Evals/Testes propostos (Stage 7)

### 7.1 Golden eval — agent_run subscription

`tests/golden/observability/agent_run_subscriber.jsonl`:

```jsonl
{
  "name": "aggregate_costs_consumes_agent_run",
  "subscriber": "customer.aggregate_costs",
  "trace": "agent_run",
  "preconditions": {
    "emissions": [
      { "agent": "customer.summarize_customer", "tokens_total": 1500,
        "cost_usd": 0.0045, "duration_ms": 820, "finish_reason": "stop" },
      { "agent": "customer.classify_intent",    "tokens_total": 80,
        "cost_usd": 0.0001, "duration_ms": 110, "finish_reason": "stop" }
    ]
  },
  "expect": {
    "subscriber_invoked": 2,
    "fields_observed": ["agent", "tokens_total", "cost_usd"],
    "no_missing_field_error": true
  }
}
```

### 7.2 Golden eval — audit emission

`tests/golden/observability/audit_emit.jsonl`:

```jsonl
{
  "name": "reassign_emits_audit_with_named_fields",
  "command": "customer.reassign",
  "input": { "id": 1, "owner_id": 42 },
  "preconditions": {
    "actor": "user:7",
    "target": { "id": 1, "owner_id": 5 }
  },
  "expect": {
    "audit_stream": "audit_log",
    "audit_payload": {
      "actor": "user:7",
      "target.id": 1,
      "input.owner_id": 42
    },
    "after_commit": true
  }
}
```

### 7.3 Go sync test — health probe wiring

`runtime/go/lazuli/observability/health_test.go` using
`testing/synctest`:

- Build a runtime with `HealthProbes.Liveness = "/healthz"`,
  `Readiness = "/readyz"`.
- Hit `/healthz` → 200 unconditionally.
- Hit `/readyz` with a healthy DB → 200.
- Set the DB pool to fail Ping → `/readyz` returns 503 with the
  failing probe name in the body.
- `/healthz` remains 200 (liveness is not readiness).

### 7.4 Doctor fixture — audit emit_to unknown

`crates/lazuli_cli/tests/fixtures/observability/audit_emit_unknown.lzi`:

```lzi
feature customer
  domain
    resource Customer
      id: ID required

  command archive
    policy @policy.delete
    audit actor, target.id
      emit_to nonexistent_stream
    deletes Customer
```

Asserts doctor emits exactly one `audit_emit_to_unknown` diagnostic
at the `emit_to nonexistent_stream` line.

### 7.5 LSP test — tracing propagation completion

`crates/lazuli_lsp/tests/observability.rs`:

- Hover on `tracing` keyword in `app.lzi` shows the contract
  summary (level, sample rate, propagation).
- Completion at column after `propagate ` offers exactly:
  `trace_id`, `request_id`, `actor`, `tenant`, `correlation_id`.
- Hover on `propagate tenant` while `app.communication propagate`
  list does NOT include `tenant` → warning hint mirroring
  `app_tracing_propagate_not_in_communication`.

## Doctor/LSP propostos (Stage 8)

### Diagnostic table

| Code | Severity | Message | Trigger | Test fixture |
|---|---|---|---|---|
| `app_logging_level_unknown` | error | "app.logging.level `<X>` must be one of: `debug`, `info`, `warn`, `error`." | `AppLogging.level` not in closed catalog | `logging_level_bad.lzi` |
| `app_logging_format_unknown` | error | "app.logging.format `<X>` must be `json` or `text`." | `AppLogging.format` not in catalog | minimal `.lzi` with `format yaml` |
| `app_logging_redact_unknown` | error | "app.logging.redact `<X>` must be `pii` or `none`." | `AppLogging.redact` not in catalog | minimal `.lzi` |
| `app_tracing_sample_rate_range` | error | "app.tracing.sample_rate `<X>` must be a float in `[0.0, 1.0]`." | `AppTracing.sample_rate` out of `[0,1]` | minimal `.lzi` with `sample_rate 2.5` |
| `app_tracing_propagate_unknown` | error | "app.tracing.propagate `<X>` is not in the closed catalog (`trace_id`, `request_id`, `actor`, `tenant`, `correlation_id`)." | propagate entry not in catalog | minimal `.lzi` |
| `app_tracing_propagate_not_in_communication` | error | "app.tracing.propagate `<X>` must also appear in `app.communication.propagate`." | propagate not in communication.propagate | minimal `.lzi` with mismatched lists |
| `audit_emit_to_unknown` | error | "audit.emit_to `<X>` does not resolve to an event_group in feature `<feature>` or to the reserved streams (`audit_log`, `audit_stream`)." | `Audit.emit_to` does not resolve | `audit_emit_unknown.lzi` above |
| `audit_missing_emit_to_no_default` | warning | "command/job/webhook declares `audit` without `emit_to`, and feature `<feature>` has no `defaults audit_emit_to`. The runtime falls back to `audit_log`; declare the stream explicitly to avoid drift." | `audit` present, `emit_to` absent, feature defaults silent | minimal `.lzi` |
| `event_trace_level_unknown` | error | "event.trace.level `<X>` must be one of: `debug`, `info`, `warn`, `error`." | `Event.level` not in catalog when `EventKind::Trace` | minimal `.lzi` |
| `event_trace_level_on_domain_event` | error | "`level` is only valid on `event.trace`, not on domain `event`." | `Event.level.is_some()` while `EventKind::Domain` | minimal `.lzi` |
| `event_trace_reserved_name` (existing, extended) | error | (existing message updated to list all 4 reserved names) | author wrote `event.trace <command_run\|job_run\|webhook_run\|agent_run>` | extend existing test |
| `trigger_trace_unknown` | error | "`trigger @trace.<X>` does not resolve. Built-in trace events: `agent_run`, `command_run`, `job_run`, `webhook_run`. Authored trace events in scope: `<list>`." | trigger ref resolves nowhere | minimal `.lzi` |

12 diagnostics. All register under existing doctor + LSP pipelines
(no new cross-feature pass needed except the audit-emit one, which
reuses the existing `event_group` resolver).

### LSP hovers (new entries)

Add to `KEYWORD_HOVER`:

| Keyword | Hover summary |
|---|---|
| `logging` | "App logging contract: `level` (debug/info/warn/error), `format` (json/text), `redact` (pii/none). Profile-aware." |
| `tracing` | "App tracing contract: `enabled`, `sample_rate ∈ [0,1]`, `propagate` (subset of `app.communication.propagate`). Adapter selected via `registry.capabilities tracing`." |
| `level` | "Severity hint. Closed catalog: `debug`, `info`, `warn`, `error`. Used by `app.logging.level` and `event.trace <name> level`." |
| `format` | "Log format. Closed catalog: `json` (default), `text` (dev-friendly)." |
| `redact` | "Redaction policy. Closed catalog: `pii` (strip fields tagged `@pii.*`), `none`." |
| `sample_rate` | "Tracing sample rate, float in `[0.0, 1.0]`. 1.0 = full capture." |
| `propagate` (in tracing context) | "Trace context fields propagated through the request graph. Must be a subset of `app.communication.propagate`." |
| `emit_to` (under audit) | "Audit destination stream. Reserved: `audit_log`, `audit_stream`. Or an `event_group <name>` declared in the same feature." |

### Closed-catalog completions to add

- `app.logging level ` → `debug`, `info`, `warn`, `error`.
- `app.logging format ` → `json`, `text`.
- `app.logging redact ` → `pii`, `none`.
- `app.tracing propagate ` → `trace_id`, `request_id`, `actor`,
  `tenant`, `correlation_id`.
- `event.trace … level ` → same as logging level.
- `audit … emit_to ` → completion offers `audit_log`,
  `audit_stream`, plus authored `event_group` names from the
  surrounding feature.
- `trigger @trace.` → completion offers `agent_run`, `command_run`,
  `job_run`, `webhook_run`, plus authored `event.trace` names in
  scope.

### Namespaces (`is_allowed_reference_namespace`)

**One** new namespace required: **`@trace`**. Reference-only;
applies inside `trigger @trace.<name>`. Add to
`crates/lazuli_lsp/src/lib.rs:2114-2135`.

### Highlighting (`editors/vscode/syntaxes/lazuli.tmLanguage.json`)

Add to keyword scope: `logging`, `tracing`, `level`, `format`,
`redact`, `sample_rate`, `enabled`, `emit_to`. The catalog literals
(`json`, `text`, `pii`, `none`, `debug`, `info`, `warn`, `error`,
`trace_id`, `request_id`, `actor`, `tenant`, `correlation_id`) hit
existing identifier scope. The `@trace.` prefix matches the
existing `@<namespace>.` scope rule.

## Critério de "ciclo fechado"

- [ ] Fixture exercises every authored axis: `app.logging`,
      `app.tracing`, `audit emit_to`, `event.trace <name> level`,
      `trigger @trace.<reserved>` (extend
      `examples/full-capsule/app.lzi` + the existing fixture trace
      events).
- [ ] `lazuli check examples/full-capsule` accepts the syntax with
      the new blocks.
- [ ] `lazuli inspect --format=json --expand=events
      examples/full-capsule` shows `built_in_trace_events[]` with
      all four entries and per-feature `audit.emit_to` populated.
- [ ] `lazuli doctor examples/full-capsule` emits zero new errors
      on the happy-path fixture and exactly the 12 named
      diagnostics on the matching negative fixtures.
- [ ] `lazuli generate` produces `dist/go/app/observability.gen.go`
      and per-feature `trace_subscribers.gen.go` / `audit.gen.go`
      that compile against `runtime/go/lazuli/observability/`.
- [ ] Lazuli Go runtime emits `agent_run` / `command_run` / `job_run` /
      `webhook_run` on the four authoring primitives; mounts
      `HealthProbes.Liveness` / `Readiness`; threads `slog` with
      the declared level/format/redact; threads a tracer with the
      declared sample rate. **Runtime-team deliverable.**
- [ ] Golden evals + the `synctest` health-probe test pass.
- [ ] LSP hovers + completion cover all new keywords + closed
      catalogs from Stage 8.

## Próximo passo

Human approval of this proposal + a separate `mode=implement` run.
Implementation **ordering** matters:

1. **IR extensions first** (no schema-breaking changes; all
   additive): `AppLogging`, `AppTracing`, `Audit.emit_to`,
   `Event.level`, extended `TraceFiresPer` + 3 new built-in trace
   events.
2. **Parser slice + analyzer** (lower `logging` / `tracing` blocks;
   extend `audit` block lowering; extend `event.trace` lowering for
   `level`).
3. **Doctor + LSP** (12 new diagnostics + 8 new hovers + 1 new
   namespace `@trace`).
4. **Inspect projection** (additive — `app.logging`, `app.tracing`,
   extended `built_in_trace_events[]`, audit `emit_to`).
5. **Codegen** (3 generated artifacts).
6. **Runtime** (parallel Lazuli Go runtime work — 6 new files under
   `runtime/go/lazuli/observability/`).
7. **Highlighting** + docs (`docs/invariants.md` adds the contracts
   as normative).

The cycle closes when the runtime-team observability cut lands and
the closed-cycle criterion checklist all green.

## Rows sugeridas para `docs/next-checklist.md`

Three additions, formatted to match the existing table style
(continuing from row 25 / 26):

```
| 26 | Observability bucket cycle — app.logging + app.tracing blocks | planned | New `AppLogging` + `AppTracing` IR structs on `App`; profile-aware overrides; 6 doctor diagnostics (`app_logging_*`, `app_tracing_*`); LSP hovers + completion for `level/format/redact/sample_rate/propagate`. Closed-catalog `level ∈ {debug,info,warn,error}` shared with §3.4. See `docs/proposals/bucket-observability-cycle.md` §Linguagem §IR. |
| 27 | Observability bucket cycle — 3 new built-in trace events (command_run / job_run / webhook_run) + `@trace.<name>` namespace | planned | Extends `built_in_trace_events()` from 1 entry (`agent_run`, A.8) to 4. New `TraceFiresPer::CommandDispatch`. New reference namespace `@trace.<name>` for subscriber jobs. 2 doctor diagnostics (`event_trace_reserved_name` extended; `trigger_trace_unknown` new). Runtime instrumentation parallel runtime-team work, same pattern as A.8. See `docs/proposals/bucket-observability-cycle.md` §3.5. |
| 28 | Observability bucket cycle — audit `emit_to` + `event.trace level` + health probe wiring | planned | `Audit.emit_to: Option<String>` resolves to feature event_group or reserved `audit_log`/`audit_stream`. `Event.level` on trace events (closed catalog). Runtime mounts declared `HealthProbes.Liveness`/`Readiness` paths (replaces hardcoded `/healthz` in `runtime/go/lazuli/http.go:28`). 4 doctor diagnostics. The runtime team owns `runtime/go/lazuli/observability/` package (6 files). See `docs/proposals/bucket-observability-cycle.md` §3.3 §3.4 §Runtime. |
```
