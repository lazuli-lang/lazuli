# AI Primitives Sequence — Master Roadmap

**Status**: Living index. Aggregates the full AI-first primitive
roadmap into one document with explicit ordering, dependencies, and
gates. Each row links to the focused proposal. The sequence is
normative for planning; individual proposals are normative for
their own contracts.

**Audience**: language team, runtime team, and proposal authors who
need a one-stop view of the AI surface beyond Cut A.

## Sequence

```
Cut A ──▶ A.7 ──▶ A.8 ──▶ A.9 ──▶ A.10 ──▶ A.11 ──▶ A.5 ──▶ A.6 ──▶ B
   │      │       │       │       │        │        │       │
   │      │       │       │       │        │        │       │
shipped shipped shipped shipped shipped shipped pilot-  pilot-
                                                  gated   gated
```

Cut A is the load-bearing foundation. Everything after is additive
on top of Cut A's `Agent` IR. Six cuts (A, A.7, A.8, A.9, A.10,
A.11) have shipped. A.5 and A.6 stay pilot-gated; Cut B and Cuts
D–H remain on the deferred list.

## Status table

| Cut | Title | Layer | Depends on | Gate | Status |
|---|---|---|---|---|---|
| **A** | tools + discriminated output + evals | language | — | shipped | [proposal](./ai-primitives-v0.md), [plan](./ai-primitives-v0-implementation.md). Phases 1–7, commits d2a6202 → b934207. |
| **A.7** | agent `expose http` | language | A | shipped (pre-evidenced) | [proposal](./ai-primitives-cut-a-7.md), [plan](./ai-primitives-cut-a-7-implementation.md). Commit 3be8611. |
| **A.8** | `agent_run` built-in trace event | language-light + runtime + adapter | A | shipped (language-side) | [proposal](./ai-primitives-cut-a-8.md). Commit ac0241d. Runtime instrumentation is parallel Drusa work. |
| **A.9** | `approval` on commands (third write-tool guard) | language | A | shipped | [proposal](./ai-primitives-cut-a-9.md). Commit b0304b4. Surfaced from the post-Cut A second-opinion analysis. Text-pattern facts until Phase L. |
| **A.10** | golden file evals (`golden "./path.jsonl" min_score N`) | language | A | shipped | Commit 3f7fcd3. AST + IR additive extension of `EvalCase`. Adapter loads + scores. |
| **A.11** | CORS in `app.lzi` (allowlist + credentials + max_age) | language-light | — | shipped | [proposal](./ai-primitives-cut-a-11.md). Commit b3fc39e. Sits alongside `urls`; the runtime materialises CORS middleware. |
| **A.5** | `safety` accepts list (PII coverage) | language | A | pilot-gated — first pilot with multi-class PII fan-in *plus catch-all anti-pattern review* | [proposal](./ai-primitives-cut-a-5.md). IR shape (`safety: Vec<QualifiedName>`) already shipped by Cut A. |
| **A.6** | tool result schema in registry | language | A | pilot-gated — first pilot referencing `tools.<x>.<field>` | [proposal](./ai-primitives-cut-a-6.md) |
| **B** | flow + budget tokens + knowledge + quota cost | language-light + pack | A | pilot-gated — ≥1 pilot with multi-step flow + each sub-cut has own gate | [Cut B section in A](./ai-primitives-v0.md) |
| **D** | multi-slot `context` block (Tier 2) | language | A | pilot writes `@fn.*`/`query.sql` joining contexts for one agent | [proposal](./ai-primitives-cut-d.md) |
| **E** | `calls agent.<name>(args)` in jobs/commands (Tier 2) | language | A | pilot writes job handler dispatching an agent | [proposal](./ai-primitives-cut-e.md) |
| **F** | `input from contract.X.Y` / `output from` (record reuse) | language | A | pilot's agent input/output drifted from a contract record in production with the asymmetry visible in code review | [audit](./pressure-6-agent-contract-binding.md) |
| **G** | `calls contract.X.Y` in agent body (dispatch via contract) | language | A; E (precedent) | pilot's agent `handler` reduces to translation between agent shape and contract operation | [audit](./pressure-6-agent-contract-binding.md) |
| **H** | typed prompt manifest (inline `prompt { vars { ... } body }`) | language | A; A.6 (compositional substrate) | pilot's prompt variable list drifted from `input`/`context` and shipped to production | [audit](./pressure-2-typed-prompt-manifest.md) |

## Sequence rationale

### Why A.7 ships before A.5/A.6

Both A.5 and A.6 are evidence-gated: they require a real pilot
product to surface the catch-all anti-pattern (A.5) or to wire an
adapter tool with field references (A.6).

A.7's evidence is already in the canonical fixture
(`examples/full-capsule/full-capsule.lzi:305-329` — the duplicated
`api customer_summary_stream` next to `agent summarize_customer`).
The architect's grade on A.7 explicitly endorsed shipping it
before A.5/A.6.

### Why A.8 coordinates with runtime

A.8 is **language-light**. The language declares the canonical
`agent_run` payload and reserves the name; the runtime instruments
agent dispatch and emits the event; adapters export to OTel,
Datadog, etc. The language-side cut is bounded (~30 lines of IR
declaration + 2 doctor diagnostics) and self-sufficient — it can
land before the runtime instrumentation is ready, giving the
runtime team a stable target.

### Why Cut B is gated, not blocked

Cut B's deferred items (`flow`, `budget tokens`, `knowledge`,
`quota cost`) are designed but require pilot evidence before
landing. Each has its own gate spelled out in the parent proposal
([Cut B section](./ai-primitives-v0.md#cut-b---deferred)):

- `flow`: ≥1 pilot product with >1 multi-step flow.
- `budget tokens`: pilot with cost-per-request enforcement need.
- `quota cost`: pack candidate, gates on three pilots needing the
  same shape.
- `knowledge`: pack candidate, gates on three pilots plus
  doctor-checkable shape.

Don't merge Cut B without independent re-grade ≥ 8.5 on AI-first
axes.

## Dependencies (technical)

```
                 ┌─────────────────────────────────────┐
                 │  Cut A: parser slice + Agent IR     │
                 │  + tools[] + discriminator + evals  │
                 │  + RegistryToolEntry (basic)        │
                 └────────────┬────────────────────────┘
                              │
       ┌──────────────────────┼──────────────────────────┐
       │                      │                          │
       ▼                      ▼                          ▼
  ┌─────────┐          ┌─────────────┐            ┌─────────────┐
  │ Cut A.7 │          │   Cut A.5   │            │   Cut A.6   │
  │ Agent.  │          │   Agent.    │            │ RegistryTool│
  │ expose_ │          │   safety:   │            │  Entry +    │
  │ http    │          │   Vec<...>  │            │  result_rec │
  └────┬────┘          └──────┬──────┘            └──────┬──────┘
       │                      │                          │
       │                      └──┬───────────────────────┘
       │                         │
       │                         ▼
       │              ┌────────────────────┐
       │              │  PII coverage      │
       │              │  union check       │
       │              │  (A.5 + A.6 inputs)│
       │              └────────────────────┘
       │
       ▼
  ┌──────────────────────────────────────┐
  │  Cut A.8: agent_run trace event      │
  │  (language-light + runtime + adapter)│
  └──────────────────────────────────────┘
```

A.5 and A.6 interact: A.6 widens the resolved-PII-class set; A.5
checks coverage against it. Either alone is partial; both
together close the contract.

A.7 is independent of A.5/A.6 — it touches `Agent.expose_http`,
not `safety` or tools' resolved fields.

A.8 depends only on Cut A's IR. The trace event payload references
the agent name and tool names but does not require the resolved
fields A.5/A.6 derive.

## Cross-cut deltas

### IR fields added by Cut A series

After all cuts ship, `Agent` carries:

```rust
pub struct Agent {
    // Cut A core:
    pub name: String,
    pub feature: String,
    pub input: Vec<TypedSlot>,
    pub context: Option<TargetExpr>,
    pub policy: Option<PolicyRef>,
    pub rate_limit: Option<String>,
    pub output_kind: AgentOutputKind,
    pub output_type: TypeRef,
    pub output_discriminator: Option<DiscriminatorRef>,
    pub model: QualifiedName,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f64>,
    pub seed: Option<i64>,
    pub prompt_path: String,
    pub safety: Vec<QualifiedName>,            // Cut A: 0..1; A.5: 0..N
    pub tools: Vec<ToolBinding>,
    pub evals: Vec<EvalCase>,

    // Cut A.7:
    pub expose_http: Option<HttpExposure>,

    // (no agent-side field added by A.6 or A.8)
}
```

`RegistryToolEntry` carries:

```rust
pub struct RegistryToolEntry {
    // Cut A core:
    pub name: String,
    pub effect: ToolEffect,
    pub pii_classes: Vec<QualifiedName>,
    pub adapter: Option<QualifiedName>,

    // Cut A.6:
    pub result_record: Option<ResultRecord>,
}
```

`Extension` (validators) carries:

```rust
pub struct Extension {
    // existing:
    pub name: String,
    pub contract: ExtensionContract,
    pub resolved_path: PathRef,
    pub previous_names: Vec<String>,
    pub span_ref: Option<SpanRef>,

    // Cut A.5:
    pub covers_pii_classes: Vec<QualifiedName>,
}
```

Plus IR-level built-in trace event registry (Cut A.8).

## Doctor diagnostic family

After all cuts ship, the agent-related diagnostic surface is:

| Cut | Id | Severity |
|---|---|---|
| A | `tool_registry_effect_required_diagnostics` | error |
| A | `agent_tool_policy_diagnostics` | error |
| A | `agent_tool_write_unguarded_diagnostics` | error |
| A | `agent_pii_unsafetied_warning` | warning |
| A | `agent_discriminator_target_invalid_diagnostics` | error |
| A | `agent_discriminator_field_invalid_diagnostics` | error |
| A | `eval_ordered_op_invalid_diagnostics` | error |
| A | `eval_nondeterministic_warning` | warning |
| A.5 | `agent_safety_pii_coverage_gap_diagnostics` | warning → error |
| A.6 | `agent_tool_result_field_unknown_diagnostics` | error |
| A.6 | `tool_result_opaque_warning` (under `--strict-tools`) | warning |
| A.7 | `agent_expose_path_conflict_local_diagnostics` (LSP) | error |
| A.7 | `agent_expose_path_conflict_cross_feature_diagnostics` (doctor) | error |
| A.7 | `agent_expose_slot_unbound_diagnostics` | error |
| A.7 | `agent_expose_slot_must_use_route_diagnostics` | error |
| A.7 | `agent_expose_method_streaming_mismatch_warning` | warning |
| A.7 | `agent_expose_audience_unknown_diagnostics` | error |
| A.8 | `event_trace_reserved_name_diagnostics` | error |
| A.8 | `agent_run_subscriber_payload_drift_diagnostics` | error |

Eighteen diagnostics across the family. Concentrated in
`crates/lazuli_cli/src/doctor.rs` for cross-feature checks and
`crates/lazuli_lsp/src/lib.rs` for file-local checks.

## Inspect projection family

After all cuts:

| Expansion | Cut A | A.5 | A.6 | A.7 | A.8 |
|---|---|---|---|---|---|
| `--expand=summary` | tools, evals, output_kind | safety[] | — | expose_http | emits_trace |
| `--expand=security` | tool effect, policy gap, write-guard, PII propagation, eval coverage | safety_coverage | (PII propagation extends to result fields) | (auth from agent policy) | — |
| `--expand=tools` (new in A) | per-agent dispatch graph | — | result_record | — | — |
| `--expand=events` | — | — | — | — | built_in_trace_events[] |
| `--expand=expose` (new in A.7) | — | — | — | unified HTTP route table | — |

## Out of scope (for the entire Cut A series)

- Multi-step orchestration (`flow`) — Cut B.
- Cost enforcement (`budget tokens` for hard reject) — Cut B.
- RAG / retrieval (`knowledge`) — Cut B / pack.
- Conversation/thread state — pack.
- Prompt templating / variable validation — Pressure 2 from
  audit, separate proposal.
- Agent ↔ contract record binding — Pressure 6 from audit,
  separate proposal.
- Multi-modal output — deferred (pressure 9).
- Streaming protocol differentiation — pack territory.
- Prompt rollout / canary — feature-flag pack.

## Coordination with the runtime team

Cut A.8 is the only cut requiring active runtime coordination. The
language-side cut is self-sufficient: IR registers the event,
doctor reserves the name, inspect emits the schema. The runtime
team's parallel work materializes the actual instrumentation.
Adapters (OTel, file, stdout) export.

For Cuts A, A.5, A.6, A.7, the runtime team's responsibility is
to consume the IR shapes for code generation. No coordination
required beyond IR JSON stability.

## Versioning

Each cut is a `LZI_LANG` minor bump and a `LZIR_SCHEMA` minor bump
(additive). Cut A.7's `HttpMethod` enum migration may be a
`LZIR_SCHEMA` major bump if it breaks IR JSON backward
compatibility for older serialized IRs (decided at impl time).

The architect grades each cut individually before it lands. The
`/lazuli-grade` pipeline is the gate.

## How to use this document

- **Proposal authors**: read this before drafting a new AI-first
  primitive. Cross-check against the Out-of-scope list and the
  Tier 2/3 candidates in [the audit](./ai-first-roadmap.md). If
  your idea overlaps with a deferred cut, defer alongside it.
- **Implementers**: read the per-cut proposal plus this document
  to understand sequencing dependencies. Don't implement a cut
  ahead of its prerequisites.
- **Reviewers**: when grading a cut, check against the IR delta
  and diagnostic table here for completeness.
- **Runtime team**: §"Coordination with the runtime team" tells
  you which cuts you need to actively coordinate (A.8) vs which
  you consume passively (A, A.5, A.6, A.7).

## Pilot-gated cuts (D, E, F, G, H)

These cuts close the audit's design space. All are designed
(audits or proposals graded by the architect) but require pilot
evidence before landing. None blocks the Cut A series.

| Cut | When to ship | Audit / proposal |
|---|---|---|
| D | A pilot writes a join-shaped `@fn.*` or `query.sql` whose only job is to compose multiple resource contexts for one agent. | [proposal](./ai-primitives-cut-d.md) |
| E | A pilot writes a `job` whose `handler` delegates to a Lazuli agent. | [proposal](./ai-primitives-cut-e.md) |
| F | A pilot's agent input/output drifted from a contract record's shape and shipped to production with the asymmetry visible in code review. | [audit](./pressure-6-agent-contract-binding.md) |
| G | A pilot's agent `handler` (or wrapping job's `handler`) only translates between agent shape and a contract operation. | [audit](./pressure-6-agent-contract-binding.md) |
| H | A pilot's prompt variable list drifted from the agent's `input`/`context` and shipped to production before being caught. Only after Cut A.6 lands (tool result schema is the compositional substrate). | [audit](./pressure-2-typed-prompt-manifest.md) |

D, E, F, G land independently in any order — each touches a
different IR surface. H is sequenced after A.6.

## Open audits — none

Both open questions from the AI-first roadmap audit
(`docs/proposals/ai-first-roadmap.md`) have been audited:

- Pressure 2 (typed prompt manifest) → audit recommends Approach
  B; promotes to Cut H after Cut A.6.
- Pressure 6 (agent ↔ contract binding) → audit recommends
  splitting into Cuts F (record reuse) and G (calls contract).

The design space surface for agents is now comprehensively
explored. Future audits should arrive from new pressure points,
not from the existing audit's residue.

## Changelog

- 2026-05-10 — Initial sequence document. Cuts A, A.5, A.6, A.7,
  A.8 proposed; Cut B deferred per architect.
- 2026-05-10 — Cuts D (multi-slot `context`) and E (async agent
  via `calls`) added as Tier 2 pilot-gated proposals.
- 2026-05-10 — Cuts F (record reuse via `from`), G (`calls
  contract.*`), and H (typed prompt manifest) added from
  exploratory audits of Pressure 2 and Pressure 6. All
  pilot-gated; no open audits remain.
- 2026-05-10 — Cuts A, A.7, A.8 shipped (commits d2a6202–b934207,
  3be8611, ac0241d). LZIR_SCHEMA 0.3.6 → 0.6.0.
- 2026-05-10 — Cut A.9 (`approval` on commands, third write-tool
  guard) added + shipped (commit b0304b4). Surfaced from the
  post-Cut A second-opinion analysis; passes boundary test as
  language territory because it changes the write-tool guard
  lattice and the command's authorisation surface.
- 2026-05-10 — Cut A.10 (golden file evals) added + shipped
  (commit 3f7fcd3). Small additive extension of `EvalCase`; runtime
  adapter handles loading + scoring.
- 2026-05-10 — Cut A.11 (CORS in `app.lzi`) added + shipped
  (commit b3fc39e). Language-light tier; sits alongside `urls`.
  Per-endpoint overrides deferred to pilot evidence.
- 2026-05-10 — LZIR_SCHEMA at 0.8.0 after A.10 + A.11.
