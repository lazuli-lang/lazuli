# Lazuli — State of AI-First

**Status**: Single-page overview for contributors, reviewers, and
external readers. Not normative. The normative docs are
`docs/canonical-semantics.md`, `docs/invariants.md`, and the
proposals under `docs/proposals/`. This document explains *why
Lazuli exists* and *where it sits* against alternatives.

**Audience**: someone landing on the repo cold who needs to
understand the project in 15 minutes. Honest about gaps.

## The problem

Most "AI in software" today is AI-as-feature: bolt an LLM call
onto an existing service, write a handler that translates a HTTP
request into a prompt, deal with prompt drift in production. The
language and framework treat the LLM call as just another
function dispatch.

That works for adding one feature. It does not work for building
a product where AI is the spine. Real AI products need:

- **Tool dispatch under typed authorization** — every LLM tool
  call is an attack surface; the language must check policy.
- **Multi-step orchestration between agents** — supervisor
  patterns, classify-then-route, escalate-to-human.
- **Retrieval that respects tenancy and PII** — RAG that doesn't
  leak data across tenants or expose `@pii.contact` to the wrong
  audience.
- **Cost guardrails** — token budgets per request, cost ceilings
  per tenant per month.
- **Eval discipline** — golden tests for non-deterministic
  outputs, gated by determinism pins.
- **Observability that's the same shape across products** —
  tokens, duration, cost, tool calls, finish reason, per turn.

A traditional framework (Express, Phoenix, Convex, Wasp) gives
you HTTP and ORM. You write the AI layer yourself, every time.
The result is a hand-rolled mini-language sprawled across
TypeScript / Python / Go.

Lazuli takes the opposite bet: **the language itself is the AI
primitive surface**. Tools, agents, evals, budgets, retrieval are
first-class declarations. The runtime materializes them. Adapters
plug in the LLM provider. The framework can be replaced; the
language stays put.

## What Lazuli is

Lazuli is a declarative source language with three sibling
artifacts: `.lzi` (feature definitions), `.lzx` (UI experience
projections), and `app.lzi` / `registry.lzi` / `workspace.lzi` /
`contract.lzi` (operational and external contracts).

Three layered concerns, kept strictly separate:

| Layer | Owns |
|---|---|
| **Lazuli** | Verifiable contracts: `.lzi` / `.lzx` source, IR, doctor, inspect, LSP, syntax. |
| **Drusa** (the runtime, codenamed Lazuli runtime in docs) | Runtime/codegen/wiring: Go scaffolding, DI, generated transport bindings, prompt-template loading. |
| **Adapters** | Concrete providers: OpenAI, Anthropic, Stripe, AWS, K8s, MercadoPago. |

The hard test: a Lazuli project should function if Drusa were
replaced by a hypothetical second runtime targeting Rust + Yew +
Flutter. If the language leaks Go-specific or React-specific
assumptions, the proposal is at the wrong layer.

## What Lazuli is not

Lazuli is not:

- A framework. You don't write JS or Go in addition to `.lzi`.
  Generated code is non-editable; user code only enters through
  typed extension points (`@validator.*`, `@fn.*`, `@hook.*`).
- A visual editor. Source is the only authoring surface. MCP
  writes via `write_dsl_feature(new_text)`; there is no second
  write path.
- A low-code platform. Closed-namespace catalog is enforced;
  invented namespaces get rejected. No drag-and-drop, no
  hidden runtime behavior.
- A schema migration tool. Schemas live in `.lzi`; migrations are
  semantic diffs from the IR baseline. There is no IR migration
  tool — re-lower from source.
- A replacement for SQL. Complex queries go through `query.sql
  "./path.sql"` with a typed return contract. Lazuli owns the
  scope and policy; SQL owns the SELECT.

## The closed namespace catalog

The most important architectural decision. Lazuli's references
are organized by axis:

| Axis | Namespaces |
|---|---|
| Identity / authorization | `@actor.*`, `@role.*`, `@scope.*`, `@policy.*` |
| Data classification | `@pii.*`, `@cap.*`, `@key.*`, `@semantic.*` |
| Extension surface | `@fn.*`, `@hook.*`, `@validator.*`, `@adapter.*`, `@client.*`, `@query_modifier.*`, `@anchor.*` |
| AI capabilities | `@llm.*`, `@tool.*` |

Every reference resolves through one of these. The catalog is
enforced by `crates/lazuli_lsp/src/lib.rs::is_allowed_reference_namespace`.
LLMs and humans cannot invent `@auth.*` or `@db.*` because the
parser rejects unknown namespaces immediately.

This is what makes AI-authored Lazuli verifiable.

## Quick tour: what does an agent look like

```lazuli
feature customer_support
  uses customer

  enum Intent
    urgent
    refund_request
    question
    other

  agent classify_intent
    input
      message: Text required
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./prompts/classify_intent.md"
    rate_limit "60 per minute per user"
    evals
      case urgent_for_outage
        requires input.message contains "outage"
        requires output = urgent

  agent summarize_customer
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./prompts/summarize_customer.md"
    safety @validator.pii_email_scrub, @validator.pii_behavioral_scrub
    tools
      customer.query.by_id
      customer.query.recent_events
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
      route customer_id: Customer.ID
    evals
      case redacts_email
        requires customer.email = "ada@example.com"
        forbids output contains @semantic.Email
```

Every line is checkable:

- `policy @policy.read` — actor authorization is in the
  feature-level policy lattice; doctor verifies the agent's
  policy is at least as strict as every tool's.
- `output discriminator Intent` — the LLM returns one enum value;
  `agent.flow step on classify.urgent` (future Cut B) can branch
  on it statically.
- `tools customer.query.by_id` — doctor checks the tool exists
  and the agent's policy can call it; PII classes propagate.
- `safety @validator.pii_email_scrub, @validator.pii_behavioral_scrub` —
  Cut A.5's coverage check ensures the union of validators covers
  the union of tool-resolved PII classes.
- `expose http` — the runtime auto-mounts the endpoint with the
  agent's policy / rate_limit / output; no `api` shim handler.
- `evals case ... requires/forbids` — gated by `temperature 0` +
  `seed N`; runs against the LLM under `lazuli test --evals`.

## What Cut A ships

The Cut A proposal (`docs/proposals/ai-primitives-v0.md`) is
approved by the architect at 8.8 / 9.2 / 8.5 / 9.0 across the
relevant axes. It introduces three primitives on `agent`:

1. **`tools` block** — declare which capabilities the LLM may
   invoke. Effect (read/write) is derived from the underlying
   capability. Doctor checks policy compatibility and PII
   propagation.
2. **Discriminated `output`** — agents can return a typed enum or
   a record with a discriminator field. Lands before any
   orchestration construct (`flow`) so branch typing is
   implementable from day one.
3. **`evals` block** — `case <name>` with `requires`/`forbids`
   clauses against the same closed predicate language used by
   `tests`. Determinism gate: only `temperature 0` + `seed N`
   makes evals gating-eligible.

Cut A's implementation plan
(`docs/proposals/ai-primitives-v0-implementation.md`) sequences
the work: parser slice → IR → doctor → LSP → inspect → fixture →
runtime hand-off. Budget: 16–18 days for one engineer.

## The roadmap (cuts after A)

| Cut | Goal | Gate |
|---|---|---|
| **A.5** | `safety` accepts a list of validators; PII coverage check | Pilot product where a code review rejected a single validator covering multiple PII classes |
| **A.6** | Tool result schema in `registry.lzi` | Pilot referencing `tools.<x>.<field>` in prompts or evals |
| **A.7** | `expose http` on agent (replaces api/agent boilerplate) | Pre-evidenced in the canonical fixture; ships first after Cut A |
| **A.8** | `agent_run` built-in trace event | Coordinated with runtime team |
| **D** | Multi-slot `context` block | Pilot writes join-shaped `@fn.*` for one agent |
| **E** | `calls agent.<name>(args)` in jobs | Pilot writes handler.go that only dispatches an agent |
| **F** | Record reuse: `input from contract.X.Y` | Pilot's agent input drifted from contract record in production |
| **G** | `calls contract.X.Y` in agent body (dispatch via remote) | Pilot's handler reduces to translation |
| **H** | Typed prompt manifest (inline `prompt { vars { ... } body }`) | Pilot's prompt vars drifted; lands after A.6 |
| **B** | `flow` + `budget tokens` + `knowledge` + `quota cost` (deferred) | Each sub-cut has own gate; ≥1 pilot with multi-step flow |

Every cut has an evidence-shaped gate. Lazuli does not ship cuts
on pressure or theoretical risk; it ships when a pilot product
proves the need.

## What Lazuli stays out of

Equally important: things that look like they should be language
but are deliberately not.

| Concern | Where it lives |
|---|---|
| HTTP / RPC transport details (TLS, headers, compression) | Adapters |
| Service-mesh / proxy / load-balancer config | Adapters / runtime |
| DI mechanics (construction order, lifetimes) | Drusa runtime |
| SDK generation for client languages | Publication artifact, not language |
| Multi-region / replication topology | Runtime decision; language declares contract only |
| Streaming protocol differentiation (SSE vs WS vs gRPC) | Pack / adapter |
| Multi-modal output (image, audio) | Deferred until ≥3 pilots |
| Conversation / thread state | Pack (`chat` is already pack territory) |
| Prompt versioning / canary | Feature-flag pack, when feature flags are core |
| Vector index operational tuning | Adapter |

The discipline: if a capability changes static analysis, policy
reachability, tenancy, generated API shape, migration identity,
or security proof — it's language or IR. Otherwise it's runtime,
pack, or adapter.

## Comparison

How does Lazuli sit against the alternatives?

| Tool | What it ships | Why Lazuli is different |
|---|---|---|
| **Wasp** | DSL `.wasp` + JS/TS code for actions/queries; generates React + Express scaffolding. | Lazuli is fully declarative — you don't write JS/TS alongside the DSL. Generated code is non-editable. AI primitives are core, not bolt-on. |
| **Convex** | TypeScript-first with reactive backend; AI features added later as a SDK layer. | Convex's AI is a library on top of a backend; Lazuli's AI is language-level (`agent`, `tools`, `evals` are syntax). |
| **Encore.dev** | Go with `//encore:api` decorators; runtime parses source and generates infra. | Same fat-runtime / thin-generated split as Lazuli, but with inline Go decorators instead of an external DSL. Lazuli's DSL is denser, has a closed grammar, doesn't require Go knowledge to author. |
| **Phoenix / LiveView** | Elixir framework with HTML-first UI primitives. | Phoenix is great for human-authored full-stack web apps; Lazuli's bet is that LLMs author and reason about declarative source better than imperative framework code. |
| **LangChain / Pydantic AI / Mastra** | TypeScript/Python libraries for LLM orchestration. | Library, not language. No static analysis of the agent's tool authorization, PII coverage, or prompt-variable bindings. Lazuli's `tools`, `safety`, `evals` are doctor-checked. |
| **Salesforce / Mendix / OutSystems** | Visual low-code with proprietary IDEs. | Lazuli has no visual editor — source is the only write surface. AI authoring requires textual contracts, not GUI workflows. |

The honest summary: Lazuli is closest in shape to Wasp + Encore +
something AI-shaped that doesn't exist yet. The architectural bet
is that a fully declarative language + a small typed IR + a
batteries-included runtime + adapter ecosystem will outperform
hand-rolled framework code for AI-heavy products.

## What works today

- Closed-namespace catalog enforced in LSP and doctor.
- `lazuli check` (file-local) + `lazuli doctor` (package-wide) +
  `lazuli inspect --expand=...` (typed read model).
- MCP server with single write tool: `write_dsl_feature(new_text)`.
  The text passes the full check/doctor pipeline before landing.
- `app.lzi`, `registry.lzi`, `workspace.lzi`, `contract.lzi`,
  `profiles.lzi` operational contracts.
- The full kitchen-sink fixture at `examples/full-capsule/`
  passes check + doctor.
- Cut A (agent + tools + evals + discriminator) is designed and
  approved; implementation plan ready (16–18 day estimate).

## What doesn't work today

Honest gaps:

- The canonical-indent parser is a 37-line legacy MVP (`crates/lazuli_syntax/src/grammar.pest`).
  Most checks today are text-pattern in LSP. Cut A's
  implementation plan starts the migration to typed AST/IR for
  one construct (`agent`), establishing the pattern.
- `Agent` does not yet exist in IR. Cut A introduces it.
- The runtime (Drusa) is in Phase B/C spike — list/lookup queries
  and basic CRUD effects work end-to-end against real Postgres;
  agent dispatch is not yet implemented.
- No pilot products exist beyond the canonical fixture. Most cuts
  past A are gated on pilot evidence that has not arrived.
- Codegen for `.lzx` UI surfaces is minimal — the language
  declares the shape; the runtime / codegen materializes the
  React/Expo views.

These gaps are not surprises; they are documented in
`docs/language-backlog.md` and `docs/next-checklist.md`.

## How to read the repo

- **`docs/canonical-semantics.md`** — the long-form spec. Read
  this if you're authoring a feature.
- **`docs/invariants.md`** — what the parser, analyzer, doctor,
  and LSP enforce. Read this if you're implementing.
- **`docs/quickref.md`** — short context pack for first-load
  agent or human authoring.
- **`docs/design-decisions.md`** — pre-emptive answers to "why
  is this dual form not friction?" Read before proposing
  vocabulary cleanup.
- **`docs/capability-layering.md`** — the language / language-
  light / pack / adapter / runtime classifier. Read before
  proposing new primitives.
- **`docs/grading-rubric.md`** — how proposals get graded. Read
  before drafting a proposal.
- **`docs/proposals/`** — all open and approved proposals.
  `ai-primitives-sequence.md` is the master index for the AI
  cut series.
- **`examples/full-capsule/`** — the canonical fixture. Cold-read
  this to test whether the language explains itself.

## How to contribute a proposal

1. Read `docs/grading-rubric.md` §"Self-assessment for proposal
   authors". Walk the 8 checks.
2. Read `docs/design-decisions.md` to verify your proposal isn't
   re-litigating a closed decision.
3. Read `docs/capability-layering.md` to verify the layer
   placement.
4. Draft a focused proposal in `docs/proposals/`. Use the existing
   proposals as templates.
5. Request a grade via `/lazuli-grade` or by invoking the
   `lazuli-language-architect` subagent directly.
6. Apply any blocking issues from the grade. Document non-blocking
   nits with rationale.
7. Land. Update `docs/next-checklist.md` and the relevant
   sequence document (e.g., `docs/proposals/ai-primitives-sequence.md`).

The architect's gate: weighted score ≥ 8.5 and no axis below 7
(`docs/grading-rubric.md` §"Quality gate"). Boundary violations
always block.

## The bet

The bet is straightforward:

- AI-heavy products will be the dominant product class in 3–5
  years.
- LLMs will author the majority of source code.
- LLMs author *declarative* source better than imperative
  framework code.
- A declarative language + small typed IR + batteries-included
  runtime + adapter ecosystem outperforms hand-rolled framework
  patterns when the language treats LLMs as first-class consumers.

Lazuli's discipline — closed namespaces, doctor checks, single
write surface, evidence-gated cuts — is what keeps this
designable as a real language and not a marketing claim.

Whether the bet pays off is empirical. The architecture is
in place; the implementation is in progress; the pilot products
are the next signal.

## Acknowledgements

The architect's discipline, the audit/grade process, and the
"every dual form has a stated reason" approach are inspired by
the experience of the previous Aerocoding/Orion projects. The
lessons from those — what worked, what got reabsorbed, what
should never be revisited — live in `docs/architecture.md` and
`docs/design-decisions.md`.

Lazuli is not a fork or successor of those projects; it is what
the lessons learned point at.
