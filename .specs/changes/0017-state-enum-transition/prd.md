---
id: 0017
title: state{} enum bound to transition — lifecycle status as a typed lattice
type: prd
status: ready
created: 2026-05-31
depends_on: [0001]
parallel_safe: false
track: evolve/ship
---

# PRD — closed state{} enum bound to transition

## Problem
7 features run lifecycle state machines (`transition` used 7× in pauta `job_steps_activities`, 6× hostpoint `traveler`, 5× pauta `hoxo_financial_integration`, 4× hostpoint `host`), but none declares the state SET as a closed, named type. Today the states are an inline list inside the `lifecycle status` block (`state pending initial` / `state in_progress` / `state completed terminal`) — there is no first-class `enum`/closed type an author or doctor can reference by name, and 3 features explicitly comment that status is **inferred from which command ran**:
- pauta `job_steps_activities.lzi:60-61` — "no sibling `status:` field, no separate `enum JobStepStatus`".
- pauta `attachments.lzi:46` — "Upload status discriminator (enum-by-command, see note above)".
- pauta `hoxo_financial_integration.lzi:29` — "Owned by the native `lifecycle status` block ... (no sibling `status:` field, no separate enum)".

So lifecycle status is a comment-documented discriminator, not a typed lattice. Transitions reference states as bare identifiers with no closed-type guarantee that every `from`/`to` is a declared member, and "enum-by-command" leaves the status set un-introspectable.

## Who hurts
- **authors of the 7 lifecycle features** — they hand-document the state set in prose ("PENDING -> PAID -> ...") because there's no type to declare it as.
- **doctor / IR consumers** — can't check that a transition's `from`/`to` are members of a *named closed set*, nor flag the comment-only "enum-by-command" shape.
- **agents** — must read prose comments to learn the status lattice.

## What we ship
1. **A first-class closed `state { ... }` declaration** — a named, closed set of lifecycle states.
2. **`transition` blocks reference it** — a transition's `from`/`to` MUST be declared members of the bound `state {}`.
3. **A doctor rule** that flags (a) transitions referencing undeclared states and (b) the comment-only "enum-by-command" shape (a `lifecycle`/`transition` machine with no declared closed state set).
4. **Migrate** the 7 lifecycle features onto declared `state {}` sets.
5. **Teach** `docs/lazuli_way/state-machines.md` (stub from 0001).

## Out of scope
- Reworking the *runtime* lifecycle lowering (the existing `lifecycle status` → discriminator-column codegen stays); this spec adds the closed-type *declaration* the transitions bind to.
- New transition guard/policy semantics (unchanged).

## Success
The 7 lifecycle features declare their states as a closed `state {}` bound to their transitions; the "enum-by-command" comments are gone; the doctor rule is green (no undeclared-state transitions, no comment-only machines); both pilots `lazuli check` + `doctor` + `go build` clean.
