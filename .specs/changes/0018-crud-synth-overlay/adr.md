---
id: 0018
title: CRUD synth overlay — policy / validators / assignments / emits on conventions[crud]
type: adr
status: accepted
created: 2026-06-01
supersedes: —
---

# ADR — The overlay is a `crud` child block on the resource; it composes onto existing IR command shapes, never new lowering

## Context
- `conventions [crud]` is a resource-body line (e.g. `conventions [crud, me]` in `host.lzi:87`, sitting among fields/lifecycle). The synth pass (`lazuli_analyzer/src/conventions/crud.rs`) builds each command via `build_create_command`/`build_update_command`/`build_delete_command`, each mapping to exactly ONE existing IR shape (`CommandEffect::Creates`/`Updates`/`Deletes`) — the "zero workflow" RULE-VOCAB-03 invariant.
- The measured gap (Pauta `create_customer`, verified) is five things the bare synth omits: per-command `policy`, `validate @validator.*`, assignment literals/renames on the effect block (`situation = prospect`, `is_active = true`, `category = input.category_id`), `emits <event>`, and curated `input`.
- Hostpoint already documents author-intent that the synth can't yet express (`host.lzi:341` "update_host is auto-synthesized … replacing the axis-specific commands") — so the demand for overlay control exists in the canonical pilot too, not just Pauta.
- Two failure modes to avoid: (1) a divergent macro DSL that introduces new lowering/runtime (breaks RULE-VOCAB-03 and the whole "synth = sugar over existing IR" guarantee); (2) silent behavior change (deleting a hand-rolled command whose policy/defaults/events the synth doesn't reproduce).

## Decision
- **Surface: a `crud` child block under the resource's `conventions` line.** It carries per-effect overlays:
  ```
  conventions [crud]
  crud
    create
      policy @policy.edit
      validate @validator.percentage
      input excludes situation, is_active, is_defaulter   # system/derived fields the synth shouldn't ask for
      assign situation = prospect
      assign is_active = true
      assign is_defaulter = false
      assign category = input.category_id                 # field-rename mapping
      emits customer_created
    update
      policy @policy.edit
      ...
    delete
      policy @policy.remove
      # soft-delete-aware automatically when the resource has `soft_delete` (spec 0015)
  ```
  Exact keyword spellings (`crud`/`create`/`update`/`delete`/`assign`/`input excludes`/`policy`/`validate`/`emits`) are fixed in BUILD against the existing registry — REUSE existing keywords (`policy`, `validate`, `emits`, `assign`-equivalent) wherever they already exist; only the `crud` block header + effect sub-headers are new tokens.
- **Composition, not new lowering.** The overlay is merged into the synthesized command DURING the synth pass, BEFORE lowering — the analyzer produces the same `CommandEffect::Creates{...}` it would for the equivalent hand-rolled command. No new IR node, no new emitter, no runtime change. RULE-VOCAB-03 holds: every overlaid command still maps to exactly one existing IR shape.
- **Absent overlay = today's synth, byte-identical.** The `crud` block is opt-in; a resource with bare `conventions [crud]` and no `crud` block emits exactly what it does today (Hostpoint adopters unchanged).
- **Curated input via `input excludes <fields>`** (exclude list), not an include list — because the synth's default is "all writable fields," and the real curation is dropping a handful of system/derived fields. Exclude is the smaller, less error-prone diff.
- **IR-equivalence is the gate.** Acceptance = `lazuli inspect` of the synth+overlay command is byte-identical to the hand-rolled original. If a clause can't be reproduced, that command stays hand-authored; the overlay covers the rest. Partial adoption allowed; silent change forbidden.
- **Upgrade `VOCAB-CRUD-SYNTH-AVAILABLE-001`** to recognize a hand-rolled command set that matches synth+overlay (not just the bare skeleton), so it fires on Pauta and names the overlay to adopt.

## Alternatives considered
- **Per-command stanzas (annotate each hand-rolled command `@synthesizable`)** — rejected: keeps the boilerplate (the input/effect block stays hand-written), so it deletes nothing. The whole point is to remove the ~40 lines.
- **A full macro/template language for commands** — rejected: introduces new lowering + a second way to express commands, violates RULE-VOCAB-03, and is exactly the "no `crud`/`assignment`/`reacts to` macros" line in the language backlog. The overlay deliberately composes onto existing shapes only.
- **Infer defaults/events from the resource** (e.g. auto-emit `<resource>_created`) — rejected as too magic: `situation = prospect` and `customer_created` are author intent the compiler can't safely guess; the overlay makes them explicit but hoisted once.
- **Include-list input (`input includes a, b, c`)** — rejected vs `excludes`: the common case curates OUT a few system fields; an include list re-lists 20+ fields and drifts when the resource grows.

## Consequences
**We accept:** a new `crud` block surface (parser + AST + analyzer merge logic) and the complexity of merging an overlay into the synth before lowering; the overlay can express only what existing IR commands can hold (by design — that's the RULE-VOCAB-03 guarantee, not a limitation to fix later).
**We gain:** `conventions [crud]` becomes viable for PRODUCTION CRUD, not just trivial resources; Pauta's 84 commands become migratable (0003); the inverse linter starts firing on real code; ~40 lines/feature of boilerplate become a short overlay; the audit's headline DRY win finally lands.
**We watch:** if authors start reaching for clauses the overlay can't express (custom multi-step logic), that's a signal they need a real `@fn` command, NOT a bigger overlay — do not grow the overlay into a macro language. The `tests`-block question (non-goal here) reopens only if generated-from-policy tests prove insufficient.
