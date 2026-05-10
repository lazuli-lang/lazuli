# Audit: Pressure 6 — Agent ↔ Contract Record Binding

**Status**: Exploratory audit. Not a proposal yet. Surfaces four
design approaches with concrete syntax, IR shape, and tradeoffs.
Recommends one direction with an honest assessment of the
unbundled sub-problems.

**Source**: Pressure 6 from `docs/proposals/ai-first-roadmap.md` —
the partial duplication between an `agent`'s shape and a
`contract` operation's records.

## The problem (more carefully framed)

The canonical fixture has two artifacts that *look* duplicative
but are not:

```lazuli
# examples/full-capsule/contracts/ai.lzi
contract acme.ai.v1
  record CustomerSummaryRequest
    org_id: ID required
    customer_id: ID required
    name: Text required
    email: @semantic.Email @pii.contact optional
    lifecycle_stage: Text required

  record CustomerSummaryResult
    customer_id: ID required
    summary: Text required
    model: Text required
    generated_at: DateTime required

  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output stream CustomerSummaryResult
```

```lazuli
# in feature customer
agent summarize_customer
  input
    customer_id: Customer.ID required
    prompt: Text required
  context customer.query.by_id(id: input.customer_id)
  ...
  output stream Text
  model @llm.default
```

**They are not 1:1**. The contract describes an external AI
service whose input wants full customer data over the wire
(`name`, `email`, `lifecycle_stage`). The agent's input wants
just the customer_id; the runtime fetches the full customer
record via `context`, then sends to whatever LLM provider the
agent uses.

Three sub-problems hide here, often conflated:

### Sub-problem 1 — Record shape duplication

The contract's `CustomerSummaryRequest` and the agent's `context.
customer` (loaded from a query) end up with overlapping field
sets (name, email, lifecycle_stage). The runtime composes
`CustomerSummaryRequest` from `context.customer` when wiring the
call to the external service. **Doctor cannot today verify that
the composition will work** — that
`CustomerSummaryRequest.lifecycle_stage` is satisfied by
`Customer.lifecycle_stage`.

### Sub-problem 2 — Dispatch target ambiguity

Today, `agent ... model @llm.default` declares dispatch through a
local LLM provider. Some agents need to dispatch through an
external service (the contract above). **There is no syntax for
"this agent dispatches via contract operation X"**.

### Sub-problem 3 — Output shape duplication

The contract's `CustomerSummaryResult` is a typed record; the
agent's `output stream Text` is loosely typed. If the agent
dispatches via the contract, doctor should know the agent's
output is `CustomerSummaryResult`, not generic Text.

## Four approaches

### Approach A — Record reuse via `from`

The agent declares its `input` or `output` shape from a contract
record. Doctor checks field compatibility.

```lazuli
agent summarize_customer
  input from contract.acme.ai.CustomerSummaryRequest
    # implicit: org_id, customer_id, name, email, lifecycle_stage
  output from contract.acme.ai.CustomerSummaryResult
  model @llm.default
  prompt "./prompts/summarize_customer.md"
```

#### What it solves

Sub-problem 1 and Sub-problem 3 in isolation. Doctor checks the
input/output shape matches the contract record.

#### What it does NOT solve

Sub-problem 2. The agent still has `model @llm.default`; the
contract record is just a shape source.

#### IR delta

Extend `Agent` (Cut A's IR):

```rust
pub enum AgentInputBinding {
    Inline(Vec<TypedSlot>),                  // existing
    FromContractRecord(QualifiedRecordRef),  // Approach A
}

pub enum AgentOutputBinding {
    InlineType(TypeRef),                          // existing (Text, Record)
    Stream(TypeRef),                              // existing (stream Text)
    Discriminator(QualifiedEnumRef),              // Cut A.2
    FromContractRecord(QualifiedRecordRef),       // Approach A
    StreamFromContractRecord(QualifiedRecordRef), // Approach A
}
```

#### Cost / value

Low cost (one variant per slot). Real value: doctor catches a
class of refactor bugs where the contract's record drifts and the
agent's local shape is no longer compatible.

### Approach B — Operation dispatch (new keyword `dispatches`)

The agent declares it dispatches via a contract operation. Replaces
`model @llm.*`.

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
    prompt: Text required
  context customer.query.by_id(id: input.customer_id)
  dispatches contract.acme.ai.summarize_customer
    org_id = ctx.org.id
    customer_id = input.customer_id
    name = context.customer.name
    email = context.customer.email
    lifecycle_stage = context.customer.lifecycle_stage
  output stream Text       # or implicit from contract
```

#### What it solves

Sub-problems 1, 2, 3 together. The mapping block is explicit.

#### What it does NOT solve

No local LLM fallback. If the contract operation is unavailable,
the agent fails. (Multi-target dispatch — sometimes local, sometimes
remote — is a separate concern: not in this audit.)

#### IR delta

```rust
pub enum AgentDispatchTarget {
    LocalLlm { model: QualifiedName, prompt: PathRef },         // existing
    ContractOperation {                                          // Approach B
        operation: QualifiedOperationRef,
        bindings: Vec<NamedBinding>,
    },
}
```

#### Why a new keyword

`dispatches` is reserved as a candidate keyword in the architect's
Cut E review (`docs/proposals/ai-primitives-cut-e.md`). However,
Cut E rejected `dispatches` in favor of reusing `calls`. The same
logic applies here: if an agent dispatches to an external
operation, that's a `calls`-shaped behavior. See Approach C below.

### Approach C — Reuse existing `calls` (architect's pattern)

The agent uses `calls` (already in language for integration
operations and, post-Cut E, for agent invocations) to reach a
contract operation.

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
    prompt: Text required
  context customer.query.by_id(id: input.customer_id)
  calls contract.acme.ai.summarize_customer
    org_id = ctx.org.id
    customer_id = input.customer_id
    name = context.customer.name
    email = context.customer.email
    lifecycle_stage = context.customer.lifecycle_stage
  output stream Text       # inferred from contract operation's output
```

When `calls` is present on an agent, `model @llm.*` becomes
forbidden (the contract operation IS the LLM dispatch). Doctor
enforces.

#### What it solves

Same as B. **Plus**: reuses existing vocabulary; no new keyword.

#### IR delta

Extend `CallTarget` (added by Cut E):

```rust
pub enum CallTarget {
    Integration { slot: String, operation: String },          // existing
    Agent { feature: Option<String>, name: String },          // Cut E
    ContractOperation {                                        // Approach C
        contract: String,
        operation: String,
        bindings: Vec<NamedBinding>,
    },
}
```

`Agent.dispatch_target: Enum { LocalLlm | ContractCall }` (or
infer from presence of `calls`).

#### Cost / value

Single keyword (`calls`) covers integrations, agents (Cut E), and
contract operations. Closes Sub-problems 1, 2, 3 with minimal new
vocabulary.

### Approach D — Status quo (rejected)

Keep the duplication; document why.

#### Argument

The agent's shape is *deliberately decoupled* from the contract.
Authors who want to migrate the agent from local LLM to external
service shouldn't have to refactor every reference.

#### Counter-argument

Decoupling is fine *until* the runtime composes the call. If
runtime composition is silent, then doctor cannot help. The
duplication is the smell; if the language doesn't model it,
errors surface at runtime.

Rejected.

## Sub-problem decomposition

| Sub-problem | A solves | B solves | C solves |
|---|---|---|---|
| 1: record shape duplication | ✓ | partial | partial |
| 2: dispatch target ambiguity | ✗ | ✓ | ✓ |
| 3: output shape duplication | ✓ | ✓ | ✓ |

A and C are **orthogonal**: A is "record reuse" (a different cut);
C is "dispatch via contract" (a different cut). Both can land
independently. B conflates them by using a new keyword (`dispatches`)
that bundles concerns C already handles.

## Recommendation

**Two separate cuts**, each with its own pilot gate:

### Cut F — `input from contract.<name>.<Record>` and `output from`

Implements Approach A. Pure record reuse. No dispatch semantics
change. Lands when at least one pilot product has an agent whose
input/output is duplicating a contract record by hand and the
shapes drift.

### Cut G — `calls contract.<name>.<operation>` from within agent

Implements Approach C. Extends the existing `calls` keyword
(Cut E adds `agent.X`; Cut G adds `contract.X.Y`) to let agents
dispatch via external services. Mutually exclusive with `model
@llm.*`. Lands when at least one pilot product has an agent that
must dispatch to a remote AI service.

### Why two cuts, not one

The audit's mistake (and Approach B's mistake) is **bundling**
record reuse with dispatch override. The architect's discipline
(`docs/design-decisions.md`) is to keep concerns separate when
their lifecycles diverge. Some products want record reuse without
remote dispatch (a local agent that happens to share shape with a
contract); some want remote dispatch without record reuse (a
remote operation whose input the agent composes inline). Cuts F
and G let each ship on its own evidence.

## Sub-questions

### S1 — Record reuse and field type compatibility

When `input from contract.acme.ai.CustomerSummaryRequest`, the
local agent's binding (`org_id = ctx.org.id`, etc.) must satisfy
the record's field types. The contract record may declare
`@pii.*` markers; PII propagation composes with Cut A.5.

The closed type catalog matters: `ID` in the contract may
correspond to `Customer.ID` locally (typed alias). Doctor needs a
"type equivalence" rule for records imported from contracts vs
locally-derived types.

### S2 — `calls contract.*` and the `@llm.*` exclusion

When the agent dispatches via a contract operation, two distinct
severity decisions apply:

- **`model @llm.*` is forbidden (error)** with diagnostic
  `agent_dispatch_conflict_diagnostics`. The presence of both
  declarations is semantic ambiguity: two dispatch targets for
  one agent. The author must pick one.

- **`temperature`, `max_tokens`, `top_p`, `seed` are warned**
  with `agent_tuning_clauses_unused_warning`. These are tuning
  hints for a local LLM dispatch; under `calls contract.*` they
  are moot because the remote operation owns those decisions.
  Warn (not error) because the author's intent isn't ambiguous,
  just stale; let `lazuli fmt` strip them if requested.

The split (error for dispatch declaration; warn for tuning
clauses) matches the architect's discipline: error when the
language can't decide, warn when the author should clean up.

### S3 — Stream / non-stream interaction

If the contract's operation declares `output stream X`, the agent
auto-inherits `output stream X` (no need to redeclare). If the
contract is non-streaming and the agent declares `output stream`,
that's a doctor error with
`agent_stream_contract_mismatch_diagnostics`. Conversely, if the
contract streams and the agent declares non-stream `output X`,
the same diagnostic fires.

The agent should be able to omit `output` entirely when `calls
contract.*` is present; doctor reports the inherited shape under
`--expand=summary`.

### S4 — Cross-app contract binding

In `workspace.lzi`, an `external app` declares a contract via
`contract "./path.lzi"`. Could an agent in one local app bind to
that external app's contract? Yes — Cut G's `calls contract.*`
should resolve through the workspace's contract registry, same as
`integrations.*` resolves through `app.lzi`.

This is a follow-on consideration; the v1 proposal can land
without it and add cross-app resolution in a subsequent cut.

### S5 — Streaming protocol propagation

Cut B's discussion of streaming protocol differentiation (SSE vs
WebSocket vs gRPC) returns here. If the contract's operation
declares HTTP+SSE, the agent inherits that; if WS, similarly. The
agent doesn't re-declare. **The runtime/adapter** materializes
the wire format. Lazuli only sees `output stream <Type>` plus
contract-declared transport.

## Open questions

- **Q-P6-1**: type-equivalence rule for contract records vs local
  types. Today `Customer.ID` is a typed alias for `ID` (or its
  enum value, depending on declaration). When the contract record
  declares `customer_id: ID`, can the agent's local binding pass
  a `Customer.ID`? Recommendation: yes, with a typed-alias
  resolution pass. **Defer to Cut F implementation.**
- **Q-P6-2**: should agent inputs derive automatically from the
  contract record (no inline binding required when names match)?
  Recommendation: no. Explicit bindings match the project's
  discipline; implicit derivation is the kind of magic the
  architect's grade catches.
- **Q-P6-3**: should `calls contract.*` accept `retry` /
  `timeout` / `idempotency` on the agent (overriding the
  contract operation's declared values)? Recommendation: no in
  v1. The contract is authoritative. Override semantics is a
  follow-on.
- **Q-P6-4**: when a contract operation declares
  `error <Name> status <int>`, how does the agent surface those?
  The agent's `output stream Text` doesn't carry error variants.
  Recommendation: the runtime maps contract operation errors to
  HTTP errors when the agent has `expose http` (Cut A.7), and to
  local errors when the agent is dispatched programmatically.
  Document but don't add IR shape in v1.

## Promotion path

If the two-cut split is endorsed:

| Cut | Gate (evidence-shaped) |
|---|---|
| F (record reuse) | Pilot product where an agent's input or output drifted from a contract record's shape and **shipped to production with the prompt/handler asymmetry visible in code review** before being caught. The "shipped to prod with the asymmetry surfaced in review" double-condition prevents the gate from satisfying on hypothetical drift; it requires both real production exposure and reviewer-visible smell. |
| G (calls contract) | Pilot product where an agent's `handler` (or wrapping job's `handler`) only translates between agent shape and a contract operation. The handler file's reduction-to-translation is the evidence. |

Both cuts depend on Cut A's `Agent` IR. They don't depend on
each other.

## Final recommendation

Defer to a proposal. The audit's split into F and G is the right
shape; the architect should re-grade the split before promotion.

Do **not** bundle as a single "agent ↔ contract" cut. The
bundling is exactly what Approach B got wrong.

## Coordination

Cut G interacts with:

- **Cut E (`calls agent.<name>`)**: same `calls` keyword,
  different target shape. They coexist cleanly because Cut E
  reuses `calls` for agent targets and G reuses it for contract
  operation targets — neither invents a new keyword.
- **Cut B `flow`**: a `flow` step might also `calls contract.*`.
  The `calls`/`then` syntax already exists in flow's design.
  Compatibility: yes, no friction.
- **Approach C's `model @llm.*` exclusion**: doctor enforces.
  Future cuts may relax this if multi-target dispatch (local +
  remote fallback) becomes a real product need.

## Final note

Pressure 6 is the second of two open audits from the AI-first
roadmap. With this audit complete, the design space surface for
agents is comprehensively explored:

- Cut A (foundation): tools, evals, discriminator
- Cuts A.5–A.8 (follow-ons): safety list, tool result schema,
  HTTP expose, agent_run trace
- Cuts D, E (Tier 2 pilot-gated): multi-slot context, async via
  calls
- Cut B (deferred): flow, budget, knowledge, quota cost
- Cuts F, G (this audit): record reuse, calls contract
- Pressure 2 (audit B): typed prompt manifest (post Cut A.6)

Whether each cut graduates is a function of pilot evidence. The
design surface is closed; the discipline is "ship one when its
gate fires."
