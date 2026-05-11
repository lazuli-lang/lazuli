# Runtime Hand-off — AI Primitives Cut A

**Status**: Hand-off doc. References the design completed in
`docs/proposals/` for the Lazuli runtime team to implement.

**Audience**: the Lazuli runtime team. Assumes familiarity with
the existing crates (`lazuli_syntax`, `lazuli_ir`,
`lazuli_analyzer`, `lazuli_lsp`, `lazuli_cli`) but not with the
AI primitives proposals.

## What's being handed off

The full AI primitives design space is documented and graded.
Cut A is the load-bearing foundation; eleven cuts (A, A.5–A.8,
B, D, E, F, G, H) plus Cut B's four sub-primitives sit on top.
Every cut has an evidence-shaped gate; only Cut A and Cut A.7 are
ready to start now (the others are pilot-gated and wait for real
products to surface the failure mode).

**The runtime team's immediate job is Cut A implementation.** Cut
A.7 follows naturally once Cut A's IR lands.

## What you need to read

In this order:

1. **`docs/state-of-ai-first.md`** — 15-minute overview of what
   Lazuli is, the language/runtime/adapters split, what works
   today, what doesn't.

2. **`docs/proposals/ai-primitives-sequence.md`** — master index
   linking every cut. Tells you the dependency graph at a glance.

3. **`docs/proposals/ai-primitives-v0.md`** — Cut A design
   proposal (the contract you're implementing). Tools,
   discriminated output, evals. Architect-graded 8.5+ across all
   axes.

4. **`docs/proposals/ai-primitives-v0-implementation.md`** —
   Cut A's phased implementation plan. **This is your work-
   breakdown.** 16–18 days for one engineer; 12 days with two.
   Phases 1–3 are sequential (parser → IR → doctor); 4–7 fan
   out.

5. **`docs/grammar.lzi.md` §14 (Agent)** — formal EBNF for the
   `agent` block including Cut A's `tools`, `evals`, `output
   discriminator` children. The parser slice should accept this.

6. **`docs/proposals/ai-primitives-cut-a-7-implementation.md`** —
   the plan for the *next* cut to ship. Read after Cut A is
   nearly done; it builds directly on Cut A's IR.

Once you're inside the implementation, refer to:

- `docs/invariants.md` for what doctor and LSP enforce.
- `docs/canonical-semantics.md` for the long-form spec.
- `docs/design-decisions.md` before proposing any deviation from
  the design (each entry is "why this dual form is intentional").
- `docs/grading-rubric.md` if you need to re-grade something.

## The 30-second summary of Cut A

Today's `agent` block is text-pattern in LSP only — `crates/
lazuli_lsp/src/lib.rs::agent_contract_diagnostics` at line ~3297.
There is **no Agent IR**. The pest grammar at `crates/lazuli_syntax/
src/grammar.pest` is a 37-line legacy brace MVP.

Cut A adds three primitives to `agent`:

```lazuli
agent summarize_customer
  input ...
  policy @policy.read
  output discriminator Intent           # Cut A: new
  model @llm.default
  prompt "./prompts/summarize.md"
  tools                                  # Cut A: new
    customer.query.by_id
    @tool.web_search
  evals                                  # Cut A: new
    case redacts_email
      requires customer.email = "ada@example.com"
      forbids output contains @semantic.Email
```

To check those three primitives, Cut A's implementation must:

1. Build a narrow canonical-indent parser slice for `agent` blocks
   (the existing pest grammar stays for everything else).
2. Land a typed `Agent` IR node in `crates/lazuli_ir/src/lib.rs`.
3. Lower from AST to IR in `crates/lazuli_analyzer/src/lib.rs`.
4. Add cross-feature doctor checks in `crates/lazuli_cli/src/doctor.rs`.
5. Shrink the LSP text-pattern checks to file-local only.
6. Extend `lazuli inspect --expand=...` to project the new IR.
7. Update the canonical fixture and the relevant docs.

This is Strategy A in the implementation plan. It pays down two
backlog items at once:

- `docs/language-backlog.md:204` "Lower the new canonical surface
  into typed IR instead of LSP-only text diagnostics."
- `docs/language-backlog.md:206` "Add parser support for canonical
  indentation syntax beyond the legacy brace MVP."

The Strategy B fallback (extend LSP text-pattern) was considered
and rejected — it extends technical debt and cannot deliver the
cross-feature `Agent.tools[].resolved_*` derived IR fields that
the architect's grade required.

## Hard constraints (do not violate)

These are inviolable per `.claude/agents/lazuli-language-architect.md`:

1. **No provider names in core syntax.** No `openai`, `anthropic`,
   `mercadopago`, `aws`, `kubernetes` keywords. Provider references
   go through `@runtime/...`, `@plugin/...`, `@adapter.<local>`.
2. **No DI mechanics in source.** Construction order, lifetimes,
   logger/db/client instances — all runtime concerns.
3. **No transport mechanics in contracts.** `contract.lzi`
   declares shape; HTTP routing tables, gRPC stub flags, broker
   partition strategies are adapter-shaped.
4. **No SDK generation as a language concept.** Optional
   publication artifact.
5. **`write_dsl_feature(new_text)` remains the only MCP write
   tool.** Do not add a second write path.
6. **Magic discovery requires visibility.** Any filename
   convention or directory rule that resolves into language
   semantics must surface in `lazuli inspect`, `lazuli doctor`,
   and LSP.

## After Cut A lands

The next four cuts have proposals + (in some cases) plans ready.
Coordinate timing with the language team:

| Order | Cut | Status | Why this order |
|---|---|---|---|
| 1 | **A.7** (expose http) | Plan ready | Pre-evidenced; removes verifiable boilerplate (`api customer_summary_stream`/`agent summarize_customer` duplication in canonical fixture) |
| 2 | **A.5** (safety list) | Proposal | Pilot evidence required (multi-PII-class fan-in in real product review) |
| 3 | **A.6** (tool result schema) | Proposal | Pilot evidence required (`@tool.*` adapter wired with prompt/eval field reference) |
| 4 | **A.8** (agent_run trace event) | Proposal | Requires runtime instrumentation; this is the only cut that you (runtime team) actively design alongside |

Cut B (`flow`, `budget tokens`, `knowledge`, `quota cost`) and
Cuts D/E/F/G/H are pilot-gated and wait for evidence.

## Cut A.8 — coordinate the language contract before instrumenting

`agent_run` is the canonical observability event. The language
side (`docs/proposals/ai-primitives-cut-a-8.md`) registers the
event in IR with a canonical payload schema; reserves the name
so authors cannot redefine it; reserves a subscriber-payload-
drift diagnostic.

The runtime side is your work. You instrument agent dispatch,
capture tokens/duration/cost from the LLM provider, buffer and
flush to adapters, propagate trace context (request_id, trace_id).
Adapters (OpenTelemetry, file, stdout) export.

The language-side cut can land before the runtime instrumentation
is ready. Give the language team a stable target by landing the
language-side IR contract; instrument at your own pace afterward.

## Questions / coordination

- **Where to ask questions about the design**: file an issue
  citing the relevant proposal in `docs/proposals/`. The
  architect grade pipeline (`/lazuli-grade`) can re-grade if a
  design ambiguity surfaces during implementation.

- **When to push back on a design decision**: if implementation
  reveals a constraint the design didn't anticipate, document
  the constraint in a follow-up proposal under `docs/proposals/`
  and request a re-grade. Don't silently work around the design.

- **What to do when a doctor diagnostic is too noisy in
  practice**: log it as an issue with a reproducer fixture. The
  language team may relax to warning, restructure the check, or
  add an opt-out — but only after seeing the noise.

- **How the runtime should consume the IR**: `lazuli inspect
  examples/full-capsule/full-capsule.lzi --format=json` is the
  stable read model. The IR JSON schema is at `docs/ir-abi.md`.
  Do not parse `.lzi` source directly from the runtime —
  consume the IR.

## What success looks like

For Cut A specifically:

- `cargo run -q -p lazuli_cli -- check examples/full-capsule/full-capsule.lzi` passes after fixture update.
- `cargo run -q -p lazuli_cli -- doctor examples/full-capsule` emits zero errors.
- `cargo run -q -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi --expand=summary,security,tools --format=json` reports the Cut A surface.
- New tests in Phases 1–5 pass.
- `tools/generate-fixtures.ps1 -Check` passes.
- A re-run of `/lazuli-grade` returns pass with all axes ≥ 7 and
  AI-first axis ≥ 8.

For the broader hand-off:

- The runtime can take a generated Go backend produced from Cut
  A's IR and run an `agent dispatch` end-to-end (LLM call with
  tools resolved, evals runnable under `lazuli test --evals`,
  policy/rate_limit/safety enforced).
- Cut A.7's `expose http` mounts agents as HTTP endpoints with
  the agent's policy/rate_limit/output applied; no `api` shim
  needed.

Beyond that, every subsequent cut lands when its pilot evidence
fires. The design is in place; implementation is where the rubber
meets the road.

## Final note

The design phase produced ~30 documents (proposals, plans,
audits, grammar, rubric, overview) totaling roughly 11,000 lines
of careful work. The architect's grade pipeline ran on every
proposal; every blocking issue was resolved before commit; every
deferred decision has a documented gate.

This is the contract. Implementation is now load-bearing.

— Lazuli language team
