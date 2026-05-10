# Proposal: Cut E — Async agent (job → agent dispatch) (Tier 2)

**Status**: Draft proposal, **pilot-gated**. Identified as Tier 2
Candidate E in the AI-first roadmap audit
(`docs/proposals/ai-first-roadmap.md` Pressure 7). Requires pilot
evidence before landing.

**Owner**: TBD. **Target version**: post Cut A.7; LZI_LANG minor
bump when the gate fires.

## Motivation

Real AI workflows are routinely background:

- "Summarize all customers nightly."
- "Classify all incoming tickets in a queue."
- "Re-embed knowledge sources every week."

Today, the only way is:

1. Author a `job` with a `handler "./path.go"`.
2. Inside that handler, manually dispatch the agent.

The handler is opaque to Lazuli. Doctor cannot check that the
agent referenced actually exists, that arguments match the
agent's `input` slots, that the agent's policy is at least as
strong as the job's actor, or that the agent's rate_limit / safety
contracts are honored.

The audit (`docs/proposals/ai-first-roadmap.md` Pressure 7)
identified this gap. The architect's note recommended **reusing
`calls`** (today goes to integrations) rather than inventing a new
keyword (`dispatches`).

Cut E extends `calls` to accept an `agent.<name>` target shape
inside `job` and `command` bodies.

## Pilot gate

Cut E lands when **at least one pilot product authors a `job`
whose handler delegates to a Lazuli agent**. The fall-through is
the evidence: a real product wrote handler code only to call into
an agent.

Until that evidence emerges, manual handlers are an acceptable
escape hatch.

## Scope

- `calls agent.<name>(args)` inside `job` and `command` bodies.
- Doctor validates the agent reference, argument binding, and
  policy lattice.
- No new keyword. No new construct. Pure extension of `calls`.

## Syntax

```lazuli
job nightly_summarize_customers
  trigger schedule "0 2 * * *"
  fanout tenants org
  policy @policy.system
  retry 3 backoff exponential
  idempotency by schedule.day, target.id

  target query.list filter: status = active
  calls agent.summarize_customer(customer_id: target.id)
```

For event-triggered jobs:

```lazuli
job classify_incoming_tickets
  trigger event support.ticket_created
  tenant_from payload.org_id
  idempotency by envelope.id

  calls agent.classify_intent(message: payload.message)
```

The `calls` clause may appear multiple times (sequential calls).
Per-call result binding is reserved for future cuts; today the
result discards.

## Rules (normative)

- **Target shape**: `agent.<name>` for local-feature; or
  `<feature>.agent.<name>` for cross-feature (requires `uses`).
- **Argument binding**: arguments bind by name to the agent's
  `input` slots. Doctor rejects missing required slots and
  unknown slot names.
- **Policy lattice**: the job's effective actor must satisfy the
  agent's `policy @policy.<name>`. Doctor cross-checks using the
  existing lattice helper (Cut A introduced it for tools).
- **Tenancy / context**: the job's `tenant_from` (or fanout
  tenant) propagates to the agent invocation. The agent runs
  under the job's authorization context.
- **Rate limits and safety**: the agent's own `rate_limit`,
  `safety`, and `budget` (when Cut B `budget tokens` lands)
  apply to each invocation. Rate-limit accounting is per-agent,
  not per-job — a single agent rate-limited at "100 per hour per
  tenant" applies across UI-triggered AND job-triggered
  dispatches.
- **Result handling**: in v1, the agent's output is discarded
  (or only logged via Cut A.8's `agent_run` trace event). Capturing
  the result back into the job's context is reserved for a future
  cut.

## Why reuse `calls`, not invent `dispatches`

The architect's audit endorsed `calls` reuse for one reason: today
`calls` already means "synchronously invoke a named external
target." Whether the target is an integration's operation
(`calls gateway.normalize_import_batch(...)`) or an agent
(`calls agent.summarize_customer(...)`) is the same shape: name-
based invocation with typed argument binding. Adding `dispatches`
would proliferate vocabulary.

Doctor and inspect distinguish the two by target prefix.

## Doctor diagnostics

| Id | Severity |
|---|---|
| `calls_agent_target_unresolved_diagnostics` | error |
| `calls_agent_arg_unknown_diagnostics` | error |
| `calls_agent_arg_missing_diagnostics` | error |
| `calls_agent_policy_diagnostics` | error |

The first three are file-local (LSP) or feature-local (doctor),
matching how integration `calls` is checked today. The policy
lattice check is doctor (cross-feature when the agent is in
another feature).

## IR delta

Extend the existing `Call` IR node (Cut for integrations introduced
it) with a target kind variant:

```rust
pub enum CallTarget {
    Integration { slot: String, operation: String },
    Agent { feature: Option<String>, name: String },
    // future: Command, Query
}
```

The `Job.calls: Vec<Call>` field already exists; this cut adds the
`Agent` variant to the existing enum.

`LZIR_SCHEMA`: minor bump (additive variant). `LZI_LANG`: minor
bump.

## Inspect delta

`--expand=summary` job entries already report `calls`. The
addition is the target kind: today implicit (integration), now
explicit:

```json
{
  "job": "nightly_summarize_customers",
  "calls": [
    {
      "kind": "agent",
      "target": "customer.agent.summarize_customer",
      "args": [{"name": "customer_id", "from": "target.id"}]
    }
  ]
}
```

## Why language, not pack

Three reasons mirror Cut A.7:

1. **Static**: the call target and argument binding are decided at
   author time. Runtime doesn't infer.
2. **Doctor needs cross-feature visibility**. Policy lattice and
   argument-shape checks span feature boundaries.
3. **Removes a fall-through pattern**. Today every async-AI
   product writes a handler.go that calls into an agent. That's
   exactly the kind of language-shaped gap that promotes from
   pack-level to language-level (per `capability-layering.md`).

## Acceptance criteria

- Cut A's `Agent` IR has shipped.
- Pilot product authored a job-with-handler-calling-agent pattern.
- `calls agent.<name>(args)` parses inside `job` and `command`
  bodies.
- Four doctor diagnostics implemented and tested.
- `--expand=summary` reports `kind: agent` on calls.
- `docs/grammar.lzi.md §10 (jobs)` adds `calls agent.<name>`.

## Non-goals

- Capturing agent output into the job. Reserved.
- Parallel agent calls in a single job. Sequential only.
- Multi-step agent orchestration (use `flow` from Cut B, not jobs).
- Synchronous user-facing flows where the agent's output is
  streamed back to the user. That's Cut A.7 (`expose http`).

## Reserved

- Capturing agent output: `let summary = agent.summarize_customer(...)`.
- Parallel: `calls parallel agent.a(...), agent.b(...)`.
- Conditional invocation: `calls agent.<name>(...) when <predicate>`.

## Release timing

After Cut A.7, when the pilot gate fires. Independent of A.5 / A.6
/ A.8 / D.

## Coordination with Cut B's `flow`

Cut B's `flow` (deferred) is the right shape for multi-step,
branching agent orchestration in a synchronous request. Cut E is
the right shape for single-agent dispatch from background work.
They don't overlap. A future cut might allow `flow` to run inside
a `job`, but that's beyond this proposal.
