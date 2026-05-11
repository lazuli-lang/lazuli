# Proposal: Cut A.8 — `agent_run` trace event (canonical observability)

**Status**: Draft proposal. Depends on Cut A
(`docs/proposals/ai-primitives-v0.md`) `Agent` IR; coordinates with
the runtime team's observability work.

**Owner**: TBD. **Target version**: `LZI_LANG` minor bump after Cut A
ships and runtime instrumentation lands.

## Motivation

Agent runs are the most-observed surface in any AI product:
prompt + completion + tokens + duration + cost + model + tool
calls per turn. Every product invents the same `event_group` and
rebuilds the same dashboards.

`docs/capability-layering.md:246` already classifies tracing as
**language-light + runtime + adapter** — language declares
`event.trace`, the runtime instruments, adapters (OpenTelemetry,
Datadog, etc.) export. The shape of the agent observability
event is the missing language piece.

Cut A.8 declares `agent_run` as a built-in trace event with a
canonical payload. Every Lazuli agent emits it on every dispatch.
The runtime instruments; adapters export. Authors get
observability for free.

The pressure was identified in the AI-first roadmap audit
(`docs/proposals/ai-first-roadmap.md` Pressure 3, Tier 1
Candidate B).

## Scope

- Built-in trace event `agent_run` with canonical payload.
- Auto-emitted by the runtime per agent dispatch (and per
  sub-agent step, when Cut B `flow` lands).
- No source declaration required; visible in `lazuli inspect
  --expand=events` as a built-in.
- IR registers `agent_run` as a system trace event, distinct
  from authored ones.
- Layer: **language-light** for the contract, **runtime** for the
  emission, **adapter** for the export.

## Promotion gate

Cut A.8 lands when **the runtime team is ready to instrument agent
dispatch** and **at least one adapter (OpenTelemetry, console,
file) consumes the event**. The gate is operational, not
evidentiary: the language contract has clear demand (the audit
identified it; every AI product reinvents it), and the value
appears the moment the runtime fires the event.

Until the runtime instruments, declaring the event in language
adds zero observability. Coordinate with the runtime team's
observability cut to land both halves at once.

## Payload shape

```text
event.trace agent_run
  payload
    agent: Text required           # qualified name "<feature>.<agent_name>"
    flow: Text optional            # populated when Cut B.flow lands
    flow_step: Text optional       # populated when Cut B.flow lands
    model: Text required           # @llm.<name> resolved at runtime
    finish_reason: Text required   # "stop" | "length" | "tool_calls" | "safety" | ...
    tokens_input: Integer required
    tokens_output: Integer required
    tokens_total: Integer required # derived (input + output); kept required for consumer convenience
    cost_usd: Decimal optional     # cents-as-Decimal; null when adapter does not report
    duration_ms: Integer required
    prompt_version: Text optional  # reserved for future prompt-rollout correlation; runtime may populate
    tools: ToolCall* optional      # repeated; one per tool dispatched in this turn
    safety_decision: Text optional # "passed" | "blocked" | "redacted"
    tenant: Text optional          # @actor / tenant context propagation
    request_id: Text optional      # trace correlation
    trace_id: Text optional        # trace correlation
```

Inner record:

```text
record ToolCall
  name: Text required              # qualified tool ref
  effect: Text required            # "read" | "write"
  duration_ms: Integer required
  status: Text required            # "ok" | "error" | "rate_limited"
  error_kind: Text optional
```

### Why these fields

Each is load-bearing for at least one of: cost reporting, capacity
planning, latency SLOs, safety auditing, prompt-engineering
iteration loops. Stripping any field would force every product
that needs it to model `agent_run` themselves — exactly the
duplication this proposal removes.

The fields are deliberately **flat** (no nested objects beyond
`tools[]`). Trace events ship through OTel/JSON serializers;
deep nesting forces every adapter to flatten anyway.

## Rules (normative)

- **No source declaration required**. `agent_run` is a built-in
  trace event registered by the IR. Authoring `event.trace
  agent_run` in source is rejected with
  `event_trace_reserved_name_diagnostics`.
- **Inspect**: `lazuli inspect --expand=events` lists `agent_run`
  with a `built_in: true` marker and the canonical payload
  schema.
- **Subscribers**: jobs may `trigger event.trace agent_run` to
  react to runs (e.g., a job that aggregates per-tenant cost into
  a billing pack). Unlike domain events, trace events do not
  participate in the typed reaction graph for doctor checks —
  they are observability-only by design (`docs/design-decisions
  .md` Decision 1: `event.trace` is outside the reaction graph).
- **Per-agent emission**: the runtime emits one `agent_run` per
  agent dispatch. When Cut B `flow` lands, each step emits its
  own `agent_run` with `flow` and `flow_step` populated; the
  flow itself does *not* emit a parent event (avoids double-
  counting cost).
- **Tool calls**: `tools[]` is empty for agents that didn't
  dispatch any tool, populated otherwise.
- **`cost_usd` representation**: `Decimal` (not `@semantic.Money`,
  to avoid currency-conversion ambiguity at trace time). Adapters
  that bill in non-USD currencies are responsible for converting
  at observation. Document this explicitly because authors will
  ask "why not Money?".

## Layer placement (language-light)

The language owns:

- The event name `agent_run`.
- The payload schema (this proposal).
- The IR registration as a built-in trace event.
- The reservation of the name (cannot be authored as a domain
  event).
- The `--expand=events` projection that includes it.

The runtime owns:

- Instrumenting agent dispatch and emitting the event.
- Capturing tokens / duration / cost from the LLM provider's
  response.
- Buffering and flushing to adapters.
- Trace context propagation (`request_id`, `trace_id`).

Adapters own:

- OTel span export (if OTel is configured).
- File / stdout JSON export (default fallback).
- Datadog / Honeycomb / Grafana Tempo / etc. provider-specific
  exports.

This split matches `docs/capability-layering.md:246` exactly.

## IR delta

Two changes:

1. A new IR registry of built-in trace events:

   ```rust
   // crates/lazuli_ir/src/lib.rs
   pub struct BuiltInTraceEvent {
       pub name: String,
       pub payload: Vec<ContractField>,  // reuse existing shape
       pub fires_per: TraceFiresPer,     // dispatch | step | tenant | ...
   }

   pub enum TraceFiresPer {
       AgentDispatch,
       FlowStep,        // Cut B
       JobInvocation,   // future
       WebhookDelivery, // future
   }

   /// Defined as a const-fn or const initializer; not authored.
   pub fn built_in_trace_events() -> Vec<BuiltInTraceEvent> {
       vec![
           BuiltInTraceEvent {
               name: "agent_run".into(),
               payload: agent_run_payload(),
               fires_per: TraceFiresPer::AgentDispatch,
           },
       ]
   }
   ```

2. Inspect projection extension:

   ```rust
   // --expand=events output adds:
   {
     "events": [
       // ...authored events...
     ],
     "trace_events": [
       // ...authored event.trace...
     ],
     "built_in_trace_events": [
       {
         "name": "agent_run",
         "fires_per": "agent_dispatch",
         "payload": [
           {"name": "agent", "type": "Text", "presence": "required"},
           // ...
         ]
       }
     ]
   }
   ```

`LZIR_SCHEMA`: minor bump (additive `built_in_trace_events`).
`LZI_LANG`: minor bump.

## Doctor diagnostics

| Id | Severity | Source |
|---|---|---|
| `event_trace_reserved_name_diagnostics` | error | A8 |
| `agent_run_subscriber_payload_drift_diagnostics` | error | A8 |

`event_trace_reserved_name_diagnostics` rejects source-side
`event.trace agent_run` declarations.

`agent_run_subscriber_payload_drift_diagnostics` runs when a job
declares `trigger event.trace agent_run` and references a payload
field that doesn't exist in the canonical schema. **Error, not
warning** — a subscriber referencing a non-existent field will
fail at runtime when the field is read; CI must catch this before
ship. The diagnostic body lists the canonical fields so the
author can correct the typo without a separate lookup.

## Inspect delta

`--expand=events` extended (above).
`--expand=summary` per-agent gains a small marker:

```json
{
  "agent": "summarize_customer",
  "emits_trace": ["agent_run"]
}
```

This documents that the agent participates in the canonical
observability contract.

## Why language, not pure runtime

The runtime *could* emit any event shape it wants without language
involvement. Three reasons not to:

1. **Schema drift**. Without language registration, every product
   that consumes `agent_run` (a billing pack, a cost dashboard, a
   capacity-planner job) hardcodes the schema. Schema changes
   become silent breakages. With language registration, doctor
   catches consumers that read non-existent fields.
2. **Inspect contract**. `lazuli inspect` is the typed read-model
   for the language. Built-in events that don't appear there
   create a hidden surface — exactly what `docs/invariants.md`
   "magic discovery requires visibility" rules out.
3. **Cross-runtime portability**. Lazuli's hard separation of
   concerns says "could a Lazuli project still function if the
   Go runtime was replaced by a hypothetical second runtime
   targeting Rust + Yew + Flutter?" Yes, only if the language declares the
   observability contract and the runtime fulfills it. Today's
   runtime can change the wire format; the contract stays put.

## Why language-light, not core

The contract is small (one event + one inner record) and the
runtime does the actual work. Promoting to core would add
ceremony without value. Language-light is the right shelf —
matches `tracing` in `capability-layering.md:246`.

## Acceptance criteria

- Cut A's `Agent` IR has shipped (so the runtime knows what to
  instrument).
- Runtime team has implemented agent-dispatch instrumentation.
- At least one adapter (OTel, file, stdout) consumes the event.
- `lazuli inspect --expand=events` lists `agent_run` under
  `built_in_trace_events`.
- `event.trace agent_run` source declaration is rejected by LSP
  with the reserved-name diagnostic.
- Authoring `job aggregate_agent_costs` with `trigger event.trace
  agent_run` works end-to-end (job sees the canonical payload).
- `docs/canonical-semantics.md §Working With Agents` documents
  the observability contract.
- `docs/invariants.md` adds the `agent_run` built-in trace event
  as a normative invariant.
- `docs/design-decisions.md` records two entries:
  1. *Built-in trace events are registered by the IR, not
     authored. The language reserves their names so subscriber
     jobs can rely on a stable payload schema.*
  2. *`agent_run.cost_usd` is `Decimal`, not `@semantic.Money`.
     Trace events are denominated in a single canonical currency
     because multi-currency conversion at observation time would
     force every adapter to carry exchange-rate state. Adapters
     that bill in non-USD convert at observation. This is a
     scoped exception to the project-wide `@semantic.Money`
     discipline; do not generalize.*

## Non-goals

- **Sampling / rate-limiting of trace events**. Adapter concern.
  The runtime emits every event; adapters can drop based on
  sampling configuration.
- **Custom payload extension**. Adding tenant-specific fields is
  *not* supported; would force every consumer to handle missing
  fields. The canonical payload is canonical. If a product needs
  more, model an authored `event.trace <custom_name>` alongside
  and join in the consumer.
- **Aggregation primitives** (`sum tokens per tenant per day`).
  Belongs in a billing/cost pack, not in language. The language
  emits the raw event; packs aggregate.
- **Cost-budget enforcement** (rejecting requests that exceed a
  budget). That's Cut B's `budget tokens` and `quota cost`
  (deferred). Cut A.8 only observes; it doesn't gate.
- **Multi-currency cost**. `cost_usd` is a single canonical
  currency. Multi-currency cost is a billing-pack concern.
- **Prompt / completion content in payload**. Prompts and
  completions are sensitive (PII, prompt-injection-bait). The
  trace payload deliberately omits them. Adapters that need full
  content can subscribe to a separate (opt-in, future)
  `agent_run_full` event with explicit retention/privacy contracts.

## Coordination with the runtime team

Cut A.8 is the only proposal in the Cut A series that requires
**runtime team coordination**, not just IR/doctor work. The
language declaration is small (~30 lines); the runtime
instrumentation is non-trivial (LLM provider integration,
buffering, async flushing, OTel context propagation).

Recommended coordination:

1. Land Cut A.8 language-side first (event registered, doctor
   reserved, inspect emits the schema).
2. Runtime team implements instrumentation against the schema in
   their parallel cut.
3. Adapter cut (OTel + file/stdout fallback) lands alongside or
   shortly after.

The language-side cut is independent and provides the contract
runtime/adapters need. Runtime can implement at their own pace.

## Reserved

- `agent_run_full` event with prompt + completion content (opt-in,
  retention contract).
- Built-in trace events for jobs (`job_run`), webhooks
  (`webhook_run`), commands (`command_run`). Same shape, different
  `fires_per`. Reserved for future cuts when the demand emerges.
- Sub-day windowing of payload aggregates (currently the runtime
  emits per-dispatch; aggregation is consumer's job).

## Release timing

Coordinate with runtime. Language-side cut can land any time after
Cut A's `Agent` IR is in. Recommended sequence:

```
Cut A   (Agent IR + tools + discriminator + evals)
  ↓
Cut A.7 (agent expose http)                         [pre-evidenced]
  ↓
Cut A.5 (safety list)                               [evidence-gated]
  ↓
Cut A.6 (tool result schema)                        [evidence-gated]
  ↓
Cut A.8 (agent_run trace event)                     [coordinate w/ runtime]
```

Cut A.8 may slot earlier if the runtime team is ready before
A.5/A.6 evidence arrives. The language-side cut is bounded;
runtime work is the variable.
