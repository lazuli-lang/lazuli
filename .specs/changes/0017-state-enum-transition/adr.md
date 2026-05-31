---
id: 0017
title: state{} enum bound to transition — lifecycle status as a typed lattice
type: adr
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
track: evolve/ship
---

# ADR — closed state{} enum bound to transition

## Context
`lifecycle`, `state`, and `transition` already exist as keywords (`crates/lazuli_keywords/src/registry/sections/s06.rs:74` `lifecycle`, `s10.rs:89-90` `state`/`transition`) and the analyzer already lowers a `lifecycle status` block into a synthesized discriminator enum + column (see the traveler waivers, `traveler.lzi:2-3`, and the existing lifecycle doctor rules `transition_to_undeclared.rs` = `LIFECYCLE-TRANSITION-TO-UNDECLARED`, `transition_from_undeclared.rs`, `unreachable_state.rs`). What's MISSING is a **first-class, named, closed `state {}` type** that authors declare and `transition` blocks bind to. Today states are an inline list inside `lifecycle status` (`job_steps_activities.lzi:69-71`), and 3 features (`job_steps_activities.lzi:60`, `attachments.lzi:46`, `hoxo_financial_integration.lzi:29`) document in prose that status is "enum-by-command" — no declared enum at all.

## Decision
1. **Add a closed `state { ... }` declaration** — a named set of states (with `initial`/`terminal` markers), reusing the existing `state` vocabulary but elevating it from an inline list to a referenceable closed type.
2. **`transition` binds to a `state {}` set.** Every `transition.from`/`transition.to` must resolve to a member of the bound set. This generalizes the existing per-machine `LIFECYCLE-TRANSITION-*-UNDECLARED` checks into a closed-type membership guarantee.
3. **Flag "enum-by-command".** A `transition`/lifecycle machine whose state set is documented only in comments (no `state {}` declared) is flagged — the comment-only shape becomes a doctor finding, not an accepted convention.
4. **Don't disturb runtime lowering.** The existing `lifecycle status` → discriminator-column/enum codegen stays; `state {}` is the declaration the transitions reference. The synthesized-enum false-positive waivers (`LIFECYCLE-ENUM-DUPLICATE`/`-FIELD-DOUBLE-DECLARED` in `traveler.lzi`) must not regress.

## Alternatives considered
- **Keep the inline `state X initial` list, add no new declaration** — rejected: doesn't give a named closed type to reference, and can't kill the "enum-by-command" prose shape (the 3 features have NO state list at all in those cases — the discriminator is purely command-implied).
- **Reuse plain `enum`** — rejected: a generic `enum` carries no `initial`/`terminal`/transition-binding semantics; lifecycle states are a lattice, not a flat value set. `state {}` makes the lifecycle intent first-class.
- **Make every lifecycle machine emit a `status:` field** — rejected: that's a storage decision, orthogonal to declaring the closed type; the runtime lowering already owns the discriminator column.

## Consequences
- **Positive**: lifecycle status is a typed closed lattice; transitions are membership-checked against a named set; "enum-by-command" prose is replaced by a declaration; agents read the state set from the type, not comments.
- **Negative / cost**: grammar + IR + codegen gain the `state {}` declaration + transition-binding; the 7 lifecycle features migrate; must coexist with the existing lifecycle lowering without double-emitting enums (respect `synth_origins` — the same root cause behind the traveler waivers).
- **Migration shape**: features that already have an inline state list (`job_steps_activities`, `hoxo`) lift it to a `state {}`; the "enum-by-command" features (`attachments`, and the command-implied cases) gain an explicit `state {}` and bind their transitions/commands to it.
