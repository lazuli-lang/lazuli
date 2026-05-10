# Proposal: AI Primitives — Cut A (v0) and Cut B (deferred)

**Status**: Draft. Cut A is the v0 candidate; Cut B is the v1 candidate gated
on pilot evidence per `docs/capability-layering.md` promotion lifecycle.

**Owner**: TBD. **Target version**: `LZI_LANG` minor bump for Cut A;
`LZI_LANG` minor bump per cut for Cut B promotions.

**History**: revised 2026-05-10 to address an independent grade. The earlier
draft proposed six primitives in one cut; this revision splits per the
architect's recommendation and resolves the blocking issues B1–B4 from that
review.

## Motivation

1. The `agent <name>` invariant in [docs/invariants.md](../invariants.md#L106)
   declares an "optional tool list" that the language has no syntax for. This
   proposal closes the invariant.
2. The single-shot `agent` shape (`input → context → prompt → output stream
   <Type>`) cannot express tool dispatch, branched orchestration, retrieval,
   evals, or cost guardrails. The minimum subset that is *statically
   checkable* and *load-bearing for code generation* should be language; the
   rest belongs in packs/runtime/adapters per
   [docs/capability-layering.md](../capability-layering.md).
3. The `@tool.*` and `@llm.*` namespaces are already in the closed catalog
   ([docs/design-decisions.md §5](../design-decisions.md#L131)). Their
   call-site shape is the missing half.

## Scope summary

| Primitive | Cut | Layer | Justification |
|---|---|---|---|
| `tools` child of `agent` | **A** | language | Closes the existing invariant. Tool dispatch is the highest-leverage prompt-injection surface; the contract must be checkable. |
| Discriminated `output` | **A** | language | Two-line IR delta (`output_kind`); enables every other branched construct. |
| `evals` child of `agent` | **A** | language | Reuses `tests` IR shape. Determinism boundary is explicit at the call site. |
| `flow <name>` | B (deferred) | language-light | Multi-agent orchestration. Depends on discriminated output. Defer until ≥1 pilot product shows source pressure. |
| `budget tokens` child of `agent` / `flow` | B (deferred) | language-light | Token caps shape generated transport behavior. |
| `quota cost` (pack) | B (deferred) | pack | Cost-per-tenant-per-month is runtime metering state. Lives in a billing/quota pack, not the language. |
| `knowledge <name>` | B (deferred) | pack candidate | RAG contract is not yet promotion-ready. Documented capsule shape only. |
| `thread` / multi-turn conversation | not in scope | pack | `chat` is already pack territory. Defer until repeated source pressure. |

This split is normative. Cut A may proceed to implementation when this
proposal is approved. Cut B may not enter the language without:

- ≥ 1 pilot product showing > 1 multi-step flow that this syntax captures
  cleanly without escape hatches.
- Resolved load-bearing decisions (entry-step convention; tool-effect
  declaration shape in registry; budget enforcement-vs-warning timing).
- Independent re-grade ≥ 8.5 on the AI-first dimensions.

## Closed-namespace impact

No new namespace is introduced in either cut. Cut A reuses `@llm.*`,
`@tool.*`, `@validator.*`, `@semantic.*`, `@pii.*`. Cut B (when promoted)
will additionally reuse `@adapter.*` for embedding adapters.

## Cross-cutting decisions

### Predicate-language extension for `evals`

The existing closed predicate language ([docs/canonical-semantics.md
§Predicate Expressions](../canonical-semantics.md#L1293)) supports `=`,
`!=`, `has`, `AND`, `OR`, paths, and literals. `evals` extends it
**only inside `requires`/`forbids` clauses of an `evals` block** with two
new shapes:

1. `<ref> contains <token-literal>` and `not <ref> contains <token-literal>`
   — substring presence over an LLM-produced text output.
2. `<ref> contains <semantic-type-ref>` and `not <ref> contains
   <semantic-type-ref>` — *any value of the named semantic type* appears in
   the text. Semantic-type membership is delegated to the validator already
   declared for that semantic type (`@validator.<auto>` per
   `@semantic.<Type>`).

The two shapes share the keyword but are distinct operators:

- `contains <string-literal>` is **substring matching**.
- `contains <@semantic.Type>` is **semantic-membership matching** (any
  substring that the type's validator accepts).

The RHS disambiguates statically. Authors should not expect an
`@semantic.Email` literal to be a string match; it always invokes the
validator.

Both forms are admissible **only inside `evals`**. They do not relax the
predicate language elsewhere. This is enforced by parser scope, not by
diagnostic.

### Vocabulary alignment

The earlier draft used `given`/`expect`. That framing was already rejected
for `tests` in [docs/invariants.md](../invariants.md#L393). This revision
uses `case`/`requires`/`forbids`, which:

- matches the existing rule/transition vocabulary (`requires @policy.<name>`,
  `requires @validator.<name>`),
- avoids introducing a parallel given/when/then framing,
- treats each eval as a *case* (named scenario) rather than a *given*
  (assumption).

## Cut A — v0 candidate

### Primitive A1 — `tools` child of `agent`

#### Goal

Declare which capabilities the LLM may invoke during a turn. Make the
authorization story checkable.

#### Syntax

```lazuli
agent triage_customer
  input
    message: Text required
  policy @policy.read
  output stream Text
  model @llm.default
  prompt "./prompts/triage.md"
  tools
    customer.query.by_id
    customer.query.list
    customer.command.reassign
    @tool.web_search
    @tool.calendar.create_event
```

#### Rules (normative)

- **Tool entry shape**: a fully qualified reference, one per line.
  `<feature>.<kind>.<name>` for first-party tools (`kind` ∈ `query`,
  `command`, `api`) or `@tool.<name>` for adapter-provided tools.
- **Resolution**: same as everywhere else. Cross-feature refs require the
  declaring feature in `uses`. Local-feature shorthand (`query.by_id`) is
  allowed and lowered to feature-qualified form by `lazuli fmt`.
- **Effect inference**: each tool's effect is derived from the underlying
  capability:
  - `query.list`/`query.lookup`/`query.sql` → `read`.
  - `command` with `creates`/`updates`/`deletes` → `write`.
  - `api` follows its declared `method`.
  - `@tool.<name>` carries the effect declared in its registry entry (see
    next bullet).
- **`@tool.*` registry shape (resolves Q1 from earlier draft)**: every
  `@tool.<name>` entry in `registry.lzi` must declare a single `effect:
  read | write` and may declare `pii_class: <pii.class>...`. Adapters that
  expose both read and write semantics over the same operation must register
  two named tools. Doctor diagnostic id
  `tool_registry_effect_required_diagnostics`.
- **Policy compatibility (doctor)**: the agent's `policy @policy.<name>`
  must be at least as strict as each tool's policy. Diagnostic id
  `agent_tool_policy_diagnostics`. The policy lattice already exists in IR
  and is checkable from existing fields.
- **Write-tool guards (doctor)**: any `write`-effect tool requires that the
  agent declare `safety @validator.<name>` *or* `idempotency by ...`.
  Diagnostic id `agent_tool_write_unguarded_diagnostics`.
- **PII propagation (doctor, warning)**: if a `read` tool returns fields
  marked `@pii.*`, or its registry entry declares `pii_class`, and the
  agent does not declare `safety @validator.<name>`, doctor warns with
  `agent_pii_unsafetied_warning`. Implementation note: requires
  `Agent.tools[].resolved_pii_classes: list<PiiClass>` derived in
  `lazuli inspect` expand; doctor reads the derived field, not the source
  query.
- **Inspect**: `--expand=summary` lists tool refs per agent.
  `--expand=security` extends with effect, policy gap, and write-guard
  status. New `--expand=tools` returns the per-agent dispatch graph keyed
  by tool ref with resolved effect, policy, feature locator, and PII
  classes.
- **MCP**: no schema change. Read tools may add a `tools` projection in a
  minor MCP bump.

#### Why no explicit per-tool effect at the binding site

The underlying capability already encodes the effect. Re-declaring at the
binding adds a contradiction surface that doctor must reconcile — extra rule
for no extra invariant. Surfaces (`submit command.<name>`) already inherit
effect from the command; `tools` follows the same convention.

The author's intent is captured by *which* tools are listed, not by a
re-declared effect. Doctor still cross-checks policy and write-guards.

### Primitive A2 — Discriminated `output`

#### Goal

Allow agent outputs to be statically branched on. This primitive lands
*before* `flow` (Cut B) so flow's branch-typing is implementable when flow
ships.

#### Syntax

Two shapes:

```lazuli
# discriminator-only: output is exactly one enum variant
agent classify_intent
  input
    message: Text required
  policy @policy.read
  output discriminator Intent
  model @llm.classifier
  temperature 0
  seed 42
  prompt "./prompts/classify_intent.md"

enum Intent
  urgent
  refund_request
  question
  other

# discriminated record: output is a record whose tag field discriminates
agent extract_action
  ...
  output Action
  model @llm.default
  prompt "./prompts/extract_action.md"

record Action
  kind: ActionKind discriminator
  customer_id: Customer.ID optional
  reason: Text optional

enum ActionKind
  reassign
  archive
  ignore
```

#### Rules (normative)

- **`output discriminator <Enum>`**: the agent returns a single enum value;
  the runtime instructs the LLM via the provider's structured-output mode to
  emit one of the enum's variants.
- **`output <Record>` with a `discriminator` marker on a field**: the
  runtime instructs the LLM to emit the record; consumers may branch on the
  marked field. At most one field per record may carry the `discriminator`
  marker.
- **Streaming**: `output stream <Type>` retains its current meaning.
  Discriminated outputs are non-streaming by construction (you cannot branch
  on a partial value).
- **Layer**: language. IR delta: `Agent.output_kind: text | stream |
  discriminated_enum | discriminated_record` defaulting to `text`. This is
  an additive minor bump.
- **Inspect**: `--expand=summary` reports the kind and the discriminator
  symbol. No new expansion.
- **Doctor**: `agent_discriminator_target_invalid_diagnostics` if the
  declared enum or record is not in scope; `agent_discriminator_field_invalid_diagnostics`
  if more than one field carries `discriminator` or the marked field's type
  is not an enum.

#### Why this lands first

Without it, `flow` (Cut B) cannot do `step on classify.urgent` — the branch
token has nothing typed to bind to. Landing the discriminator first means
`flow` is a pure routing-graph addition on top of an existing, already-
shipped output type system, and the branch-checking diagnostic is
implementable from day one.

### Primitive A3 — `evals` child of `agent`

#### Goal

Make agent behavior testable through the existing `lazuli test` pipeline,
without expanding the determinism boundary of `lazuli check`.

#### Syntax

```lazuli
agent summarize_customer
  ...
  evals
    case short_for_active
      requires customer.lifecycle_stage = active
      requires output.length < 800
      requires output contains "active"

    case redacts_email
      requires customer.email = "ada@example.com"
      forbids output contains @semantic.Email

    case uses_lookup_when_id_known
      requires input.customer_id = "cus_123"
      requires tools.calls includes customer.query.by_id
```

#### Rules (normative)

- **Block shape**: `evals` opens a list of `case <name>` children. Each
  case has one or more `requires <predicate>` and/or `forbids <predicate>`
  clauses. Negation lives in `forbids`; do not write `not` inside
  `requires`.
- **Predicate language inside `requires`/`forbids`**: the closed predicate
  language extended with the two new shapes documented in *Cross-cutting
  decisions / Predicate-language extension*. The `<` `<=` `>` `>=` operators
  are admissible only when both sides are numeric (`<ref>.length`,
  `<ref>.count`, fields of numeric type). Doctor rejects ordered ops on
  non-numeric refs with `eval_ordered_op_invalid_diagnostics`.
- **Determinism gate (normative)**: a case is a gating test (passes/fails
  CI) **only when its enclosing agent declares both `temperature 0` and
  `seed <int>`**. Cases on non-deterministic agents run as informational
  results. This is the load-bearing rule of `evals`: without it, CI would
  flake on any model temperature change. Authors who want gated evals must
  pin determinism at the agent definition.
- **Two run modes**:
  - `lazuli check` does not run evals. It validates eval bodies against
    the predicate language and emits `eval_nondeterministic_warning` when a
    case is declared on an agent without `temperature 0` and `seed`.
  - `lazuli test --evals` runs evals against the agent's configured
    `@llm.<name>`. Cases on agents without `temperature 0` and `seed` run
    as informational results, not as gating tests.
- **Layer**: language. IR delta: `Agent.evals: Vec<EvalCase>`. Additive.
- **Inspect**: `--expand=summary` lists case names per agent.

#### Why `evals`, not `tests`

`tests` are pure-IR predicates evaluated by `lazuli check` and `lazuli
test` against the IR. They do not dispatch. Letting `tests` silently
dispatch to an LLM would make `lazuli check` non-deterministic for some
constructs and not others. `evals` keeps the determinism boundary explicit
at the call site and preserves `tests` as a pure-pipeline tool.

## Cut B — deferred

The following primitives are documented here for forward-compatibility
analysis. They do not enter the language with Cut A. Each requires a
separate proposal, fixture, pilot evidence, and grade pass before
promotion. Their syntax sketches are illustrative, not normative.

### Primitive B1 — `flow <name>`

Renamed from the earlier draft's `agent.flow`. Lazuli's discipline is *one
keyword per noun* (`workflow`, `webhook`, `notification`, `agent`); flow is
a distinct noun.

```lazuli
flow handle_customer_ticket
  input
    message: Text required
    customer_id: Customer.ID required
  policy @policy.read
  budget tokens 8000 per request

  step entry classify
    by agent.classify_intent(message: input.message)

  step on classify.urgent
    by agent.summarize_customer(customer_id: input.customer_id)
    then customer.command.reassign(target_owner: @role.lead_specialist)

  step on classify.refund_request
    by agent.summarize_customer(customer_id: input.customer_id)
    then customer.command.reassign(target_owner: @role.refund_specialist)

  step otherwise
    by agent.summarize_customer(customer_id: input.customer_id)

  output stream Text from step.summarize
  emits ticket_handled
```

#### Pre-promotion decisions to pin

- **Entry convention** (resolves Q3 from earlier draft): `step entry
  <name>` is required when no `step otherwise` exists. Declaration order
  is never load-bearing.
- **Step typing**: `step on <prev>.<branch>` requires the previous step's
  sub-agent to declare a discriminator (Primitive A2). Without
  discriminator, the diagnostic
  `flow_branch_unreachable_diagnostics` fires.
- **Sub-agent dispatch and `then` post-step**: same name resolution as
  `tools`; cross-feature requires `uses`.
- **Reachability**: every `step` must be reachable from the entry through
  declared branches or `otherwise`. Diagnostic `flow_step_unreachable_diagnostics`.
- **Why not `workflow`**: `workflow` is a state machine on a *resource*
  whose transitions move it between states. `flow` is a routing graph
  between *agents* with no implicit resource state. Conflating the two
  would prevent strict checks (a workflow transition must declare its
  target state; a flow step must not).

### Primitive B2 — `budget tokens` child of `agent` and `flow`

Token caps only. Cost-per-tenant-per-month is *not* part of the language.

```lazuli
agent triage_customer
  ...
  budget tokens 4000 per request

flow handle_customer_ticket
  ...
  budget tokens 8000 per request
```

#### Pre-promotion decisions to pin

- **Scope**: `per request` only in the language. Aggregate windows belong
  in the quota pack (see B3 below).
- **Enforcement timing**: hard reject in production profile, warn-then-proceed
  in development profile, controlled by `--security-profile`. Decided at
  promotion time.
- **Doctor checks**: positive integer; flow's `per request` ≥ max of any
  step's `per request`. Diagnostic ids
  `agent_budget_tokens_invalid_diagnostics`,
  `flow_budget_underprovisioned_warning`. The flow rollup requires each
  agent to *declare* `budget tokens` before doctor can sum.

### Primitive B3 — `quota` (pack)

Cost-per-tenant-per-month is runtime metering state. Per
[docs/capability-layering.md](../capability-layering.md#L184), it should
ship as a `quota_pack` with optional doctor rules, not as language.

The pack would expose declarations like:

```lazuli
# In a billing/quota pack feature
quota agent_cost on agent.summarize_customer
  cost "$0.005" per request
  cost "$25" per tenant per month

quota agent_cost on flow.handle_customer_ticket
  cost "$0.05" per request
  cost "$200" per tenant per month
```

Promotion to language is gated on three or more pilot products needing the
same shape *and* on the metering contract reducing to a checkable IR.

### Primitive B4 — `knowledge` (pack candidate)

The earlier draft proposed `knowledge` as language-light. Per the architect's
review, the four invariants `knowledge` would carry (tenancy, PII,
retention, audit) already have language primitives, and the `knowledge`
block reduced to a *thin wrapper* over an existing `query.list` plus an
embedding adapter. That is pack territory.

The deferred shape:

```lazuli
# In a `rag_pack` feature
knowledge customer_history from @pack.rag.embedded_query
  source query.list customer.list
  chunk by lifecycle_stage
  retention 30d
  pii contact, behavioral
  tenant_from customer.org_id
```

Agent-side reference would land alongside `flow` in B1 if `knowledge`
graduates:

```lazuli
agent summarize_customer
  ...
  knowledge @pack.rag.customer_history
    top_k 5
    filter customer_id = input.customer_id
    must_cite true
```

Promotion is gated on three or more pilot products plus a doctor-checkable
shape that survives the pack-vs-language test.

## Cut A IR delta (summary)

Additive minor bump on `LZI_LANG`; minor bump on `LZIR_SCHEMA`:

- `Agent.tools: Vec<ToolBinding>`. `ToolBinding { ref: SymbolRef, source_span }`.
- `Agent.tools[].resolved_effect: Read | Write` derived in expand.
- `Agent.tools[].resolved_policy: PolicyRef` derived in expand.
- `Agent.tools[].resolved_pii_classes: Vec<PiiClass>` derived in expand.
- `Agent.evals: Vec<EvalCase>`. `EvalCase { name, requires: Vec<Predicate>, forbids: Vec<Predicate> }`.
- `Agent.output_kind: Text | Stream | DiscriminatedEnum | DiscriminatedRecord` defaulting to `Text`.
- `Agent.output_discriminator: Option<SymbolRef>` (the enum or record-field).
- `RegistryToolEntry.effect: Read | Write` (required).
- `RegistryToolEntry.pii_class: Vec<PiiClass>` (optional).

## Cut A doctor diagnostics (summary)

| Id | Severity | Source primitive |
|---|---|---|
| `tool_registry_effect_required_diagnostics` | error | A1 |
| `agent_tool_policy_diagnostics` | error | A1 |
| `agent_tool_write_unguarded_diagnostics` | error | A1 |
| `agent_pii_unsafetied_warning` | warning | A1 |
| `agent_discriminator_target_invalid_diagnostics` | error | A2 |
| `agent_discriminator_field_invalid_diagnostics` | error | A2 |
| `eval_ordered_op_invalid_diagnostics` | error | A3 |
| `eval_nondeterministic_warning` | warning | A3 |

## Cut A inspect deltas (summary)

| Expansion | Adds |
|---|---|
| `--expand=summary` | `agents[].tools[]`, `agents[].evals[]`, `agents[].output_kind`, `agents[].output_discriminator` |
| `--expand=security` | per-tool effect/policy gap, write-guard status, PII propagation status, eval coverage |
| `--expand=tools` (new) | per-agent dispatch graph keyed by tool ref |

**Pass model**: base `lazuli inspect` stays single-pass; it reports tool
refs verbatim without resolving the underlying capability. Cross-feature
resolution of `Agent.tools[].resolved_effect`, `resolved_policy`, and
`resolved_pii_classes` is performed only under `--expand=tools` and
`--expand=security`, and only those expansions require the workspace IR to
be loaded. This preserves the existing single-pass guarantee for callers
that only need the summary projection.

## Cut A non-goals (v0)

- Multi-step orchestration (Cut B).
- Cost-per-tenant metering (pack).
- RAG/knowledge contracts (pack candidate).
- Conversation/thread memory (pack).
- Tool protocol specifics (MCP tool calls, OpenAI tool calls). Adapters
  bridge the contract to whatever the LLM provider offers.
- Loops, conditionals, computations beyond the existing `command` /
  `query.sql` / `@fn.*` escape hatches.

## Acceptance criteria for Cut A implementation

- This document approved by ≥ 1 senior reviewer plus the `/lazuli-grade`
  pass with all axes ≥ 7 and the AI-first axis ≥ 8.
- Doctor diagnostics drafted line-by-line with crate paths.
- IR delta drafted in `crates/lazuli_ir/` with backward compatibility test.
- Fixture in `examples/full-capsule/` updated to use `tools`,
  `output discriminator`, and one `evals` block on the existing
  `summarize_customer` agent.
- LSP coverage for the new diagnostics, with snapshot tests.
- `quickref.md` updated to reflect the new shapes.

## Relation to `docs/design-decisions.md`

If Cut A lands, add the following decisions:

- **Tools effect derived, not declared.** Re-declaring at the binding site
  creates a contradiction surface; the underlying capability is the source
  of truth.
- **`evals` separate from `tests`.** Determinism boundary at the call
  site. `tests` stays pure-IR; `evals` dispatches under explicit
  determinism.
- **Discriminated `output` lands before `flow`.** Branch typing must be
  implementable from day one of any orchestration construct.

## Reserved for Cut B and beyond

- `flow <name>` (B1).
- `budget tokens` for `agent` and `flow` (B2).
- `quota cost` as a pack contract (B3).
- `knowledge` as a pack contract (B4).
- `thread` for multi-turn agents.
- `step parallel a, b` for fan-out flows.
- Sub-day budget windows.
- `requires tools.calls.count <op> <int>` in evals.

### Cut A.5 — `safety` accepting a list (suggested follow-on)

After Cut A lands, the `safety @validator.<name>` slot deserves a small
audit. Today it is a single ref; with `tools` introducing fan-in over
multiple `@pii.*` classes, a single validator quickly becomes the
bottleneck ("this validator scrubs emails, that one scrubs SSNs").

The minimal Cut A.5 lets `safety` accept a list and lets doctor cross-check
that the union of validator coverage spans the union of
`resolved_pii_classes` across an agent's tools. It is purely additive on
top of Cut A's IR delta (`Agent.tools[].resolved_pii_classes` already
enumerated), checkable from existing fields, and does not derail Cut B.

```lazuli
agent summarize_customer
  ...
  safety @validator.pii_email_scrub, @validator.pii_ssn_scrub
  tools
    customer.query.by_id
    customer.query.payment_history
```

Doctor diagnostic id `agent_safety_pii_coverage_gap_diagnostics`. Promotion
gate: the first pilot product whose tools span more than one `@pii.*` class.

## Changelog

- 2026-05-10 — Cut A / Cut B split. Resolved B1–B4 from the architect
  review: `knowledge` and `quota cost` reclassified as pack candidates;
  Q1 pinned (registry-side `effect: read | write`); Q3 pinned (`step
  entry <name>` required); discriminator reordered before flow. Replaced
  `given`/`expect` with `case`/`requires`/`forbids`. Renamed `agent.flow`
  to `flow`. Removed string-literal money in budgets.
- 2026-05-10 — Initial draft.
