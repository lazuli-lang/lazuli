# Proposal: Cut D — Multi-slot `context` block on `agent` (Tier 2)

**Status**: Draft proposal, **pilot-gated**. Identified as Tier 2
Candidate D in the AI-first roadmap audit
(`docs/proposals/ai-first-roadmap.md` Pressure 5). Requires pilot
evidence before landing.

**Owner**: TBD. **Target version**: post Cut A.7; LZI_LANG minor
bump when the gate fires.

## Motivation

Today an agent's `context` accepts exactly one target expression:

```lazuli
agent summarize_customer
  context customer.query.by_id(id: input.customer_id)
```

When an agent needs multiple contexts (the customer record plus
recent invoices plus recent tickets), the author must choose
between:

1. Writing a single `query.sql` that joins them. Loses typed
   resource shape; doctor cannot check the join.
2. Doing the join in the prompt template. Lazuli does not see the
   join; references are unchecked.
3. Putting the join in a custom `@fn.*`. Opaque to Lazuli; doctor
   cannot reason about the result.

None of those are checkable. Real AI products that need rich
context routinely fall through to one of these escape hatches.

Cut D promotes `context` to accept multiple named slots, each
binding to a query or resource. The agent's prompt and evals can
reference any slot by name; doctor validates that each slot
resolves to a known query result and that prompt/eval references
match the slot's fields.

## Pilot gate

Cut D lands when **at least one pilot product authors a
custom `@fn.*` or `query.sql` whose only job is to join two
or more resources for a single agent's context**. The fall-through
escape hatch is the evidence: a real product wrote join code
because the language couldn't represent multi-context cleanly.

Until that evidence emerges, single-context is sufficient for most
agents and the multi-slot form is over-engineering.

## Scope

- `context` block child of `agent`, replacing the single-line form
  when used.
- Each child of the block is a named slot binding to a query call.
- The single-line form (`context customer.query.by_id(...)`) is
  preserved for backward compatibility and remains canonical for
  single-context cases.
- Doctor diagnostics for slot name conflicts, unknown slot
  references in prompts/evals, and unresolved query targets.

## Syntax

Single-context (existing, unchanged):

```lazuli
agent summarize_customer
  context customer.query.by_id(id: input.customer_id)
```

Multi-slot (Cut D):

```lazuli
agent summarize_customer
  input
    customer_id: Customer.ID required
  context
    customer        = customer.query.by_id(id: input.customer_id)
    recent_invoices = billing.query.invoices_by_customer(customer_id: input.customer_id)
    recent_tickets  = support.query.recent_tickets(customer_id: input.customer_id)
```

The single-line form is sugar for the same IR shape with one slot
named after the query's last segment.

## Rules (normative)

- **Block shape**: `context` opens a block. Each child is
  `<slot_name> = <target_expr>`. Slot names are `IDENT_LOWER`.
- **Target expression**: any feature-qualified `<feature>.query.*`
  (Cut A's tool-binding shape; consistent). Cross-feature requires
  `uses`. Local-feature shorthand `query.<name>` allowed.
- **Slot resolution**: each slot becomes available in the agent's
  prompt template and evals as `context.<slot_name>.<field>`. The
  single-line form is shorthand for a slot named after the
  query's name segment (e.g.,
  `context customer.query.by_id(...)` is equivalent to
  `by_id = customer.query.by_id(...)`). Doctor reports the
  inferred slot name when the shorthand is used.
- **Unique names**: slot names must be unique within an agent's
  context block. Doctor rejects collisions with
  `agent_context_slot_collision_diagnostics`.
- **Prompt/eval references**: doctor walks the prompt file (if
  pursuant to the *typed prompt manifest* — see Open Questions)
  and evals to verify `context.<slot>.<field>` references resolve.
  Without typed prompts, prompt-reference validation is impossible
  and this check fires only on evals.

## Doctor diagnostics

| Id | Severity |
|---|---|
| `agent_context_slot_collision_diagnostics` | error |
| `agent_context_slot_unknown_reference_diagnostics` | error (evals only) |
| `agent_context_target_unresolved_diagnostics` | error |

`agent_context_target_unresolved_diagnostics` rejects slots whose
target query doesn't exist or whose feature isn't in `uses`.

## IR delta

Extend `Agent` (Cut A introduced):

```rust
pub struct Agent {
    // existing fields...
    pub context: ContextBinding,
}

pub enum ContextBinding {
    None,
    Single(TargetExpr),                    // pre-Cut-D form preserved
    Multi(Vec<NamedContextSlot>),
}

pub struct NamedContextSlot {
    pub name: String,
    pub target: TargetExpr,
    pub span_ref: Option<SpanRef>,
}
```

`Single` continues to lower from the single-line form. `Multi`
lowers from the block form. Lowering of the single-line form may
optionally normalize to `Multi` with one slot for IR consumers
that prefer the uniform shape; this is a flag on the lowering
pass, not authored syntax.

`LZIR_SCHEMA`: minor bump (additive enum variant). `LZI_LANG`:
minor bump.

## Why language, not pack

The check that prompt/eval references resolve requires walking
both the agent's source and the source of every slot's target
query. Pack-level checks would need to re-derive cross-feature
query field shapes. Doctor's job.

## Acceptance criteria

- Cut A's `Agent` IR has shipped.
- Pilot product authored a join-shaped `@fn.*` or `query.sql` for
  agent context.
- Block form parses, lowers, and is honored.
- Single-line form continues to work unchanged.
- Three doctor diagnostics implemented and tested.
- Inspect `--expand=summary` reports `context_slots` per agent.
- `docs/grammar.lzi.md §14 (Agent)` adds the block form.

## Non-goals

- Late-binding context (lazy fetch on demand inside the prompt).
  Synchronous resolution at agent dispatch is the only mode.
- Implicit join inference. Each slot is its own query; if two
  slots reference the same resource, the runtime fetches twice
  unless the runtime's caching kicks in.
- Slot-level policy. The agent's overall `policy` applies to all
  slots; per-slot policy overrides are not in scope.

## Open questions

- **Q-D-1**: typed prompt manifest. Without a way to declare
  prompt-template variables, doctor cannot validate
  `context.<slot>.<field>` references in prompt files. The
  proposal works regardless (evals can still be validated), but
  the highest-value check requires the typed-prompt work
  (Pressure 2 from the audit). Cut D should not block on it.
- **Q-D-2**: deprecate the single-line form? Once the block form
  exists, the single-line is sugar. Keep both for now; revisit
  if the dual form causes LLM confusion.

## Reserved

- Per-slot caching declarations.
- Lazy-loaded context slots.
- Computed slots (`<name> = @fn.<name>(...)`).

## Release timing

After Cut A.7, when the pilot gate fires. Independent of A.5 / A.6
/ A.8.
