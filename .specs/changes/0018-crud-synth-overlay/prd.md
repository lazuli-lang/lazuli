---
id: 0018
title: CRUD synth overlay — policy / validators / assignments / emits on conventions[crud]
type: prd
stage: standalone (unblocks 0003)
status: ready
created: 2026-06-01
---

# PRD — CRUD synth overlay

## Problem
`conventions [crud]` synthesizes a bare create/update/delete from a resource's own fields with default rules. Real production CRUD carries more, and the synth can reproduce NONE of it. Measured directly on Pauta (`VOCAB-CRUD-SYNTH-AVAILABLE-001` from spec 0002 fires **0×** across 84 hand-rolled CRUD commands, even after 0004+0015 landed). Concrete gap, from `customer_management.lzi:331` `create_customer`:
- `policy @policy.edit` — a specific policy, not the synth's default.
- `validate @validator.percentage` — a custom resource validator on the command.
- assignment literals the synth can't infer: `situation = prospect`, `is_active = true`, `is_defaulter = false` (default values on create), `category = input.category_id` (field-rename mapping).
- `emits customer_created` — event emission.
- `tests` block — custom predicates.
- a curated `input` that omits system/derived fields.

The result: Pauta's 0/84 adoption is CORRECT — the bare synth would silently change authz, validation, defaults, and events. The audit's headline ("conventions[crud] shipped but unused") is therefore not laziness; it's a real language gap. The synth covers the skeleton; it has no way to let an author overlay the per-resource specifics that every real CRUD command needs.

## Why now (or why ever)
Spec 0003 (migrate Pauta's 84 commands onto `conventions [crud]`) is BLOCKED-BY-DESIGN on this — proven, not hypothesized: the inverse linter refuses to suggest the migration because the commands aren't synth-equivalent. Without an overlay, `conventions [crud]` stays a toy for policy-trivial resources (the only adopters are Hostpoint catalog/host/traveler, all side-effect-free) and the whole DRY win the audit promised never lands. Growing the synth to carry the overlay is the prerequisite that makes `[crud]` viable for production CRUD — and the thing that finally lets 0003 migrate Pauta for real.

## Outcome — done means
1. A resource that opts into `conventions [crud]` can declare a per-bundle OVERLAY that the synth composes onto the generated commands, covering at minimum: (a) per-command `policy` override, (b) `validate @validator.*`, (c) default-literal + field-rename assignments on the synthesized `creates`/`updates` block, (d) `emits <event>`, and (e) curated `input` (omit/include fields). The exact surface is fixed in the ADR.
2. The synth is COMPOSITIONAL: where a resource declares the overlay, the emitted IR command is byte-identical to the equivalent hand-rolled command (the existing emitters lower it; no new lowering shapes — RULE-VOCAB-03 "zero workflow" is preserved).
3. `VOCAB-CRUD-SYNTH-AVAILABLE-001` (spec 0002) is upgraded to fire when a hand-rolled command set matches a synth+overlay it could adopt (today it only matches the bare skeleton, hence 0× on Pauta).
4. Proof on the real Pauta `create_customer`/`update_customer`/`delete_customer` trio: `conventions [crud]` + overlay reproduces all three with IR-equivalence (inspect before/after identical), and the delete is soft-delete-aware (via 0015). This trio is the acceptance fixture; the full 84-command migration is spec 0003.
5. `docs/lazuli_way/crud-by-convention.md` updated: the overlay is the idiom for production CRUD; bare `[crud]` is the trivial case.

## Non-goals
- The full Pauta migration — that's 0003 (this spec ships the language + proves it on ONE trio).
- A workflow/macro language. The overlay composes onto the EXISTING IR command shapes; it does not introduce new lowering or runtime behavior (RULE-VOCAB-03 zero-workflow stays).
- `tests` blocks as part of the overlay — authored `tests` stay explicit (they're behavior beyond policy; the language-backlog already says command actor-matrix tests are generated from policy, behavior tests are authored). If the overlay can carry them cheaply, fine, but not required.
- Changing the `me` convention.
- Inventing new policy/validator/event vocabulary — the overlay only REFERENCES existing `@policy.*`/`@validator.*`/events.

## User stories
- As a Pauta author, I write `conventions [crud]` + a short overlay (policy, validator, 3 default assignments, one emit) and delete ~40 lines of hand-rolled create/update boilerplate, with the compiler proving the behavior is identical.
- As an agent on a new feature, `VOCAB-CRUD-SYNTH-AVAILABLE-001` now fires on my hand-rolled CRUD-with-policy and tells me the overlay shape to adopt — so I don't reinvent the skeleton.

## Constraints
- Zero behavior change for existing bare `[crud]` adopters (Hostpoint catalog/host/traveler) — the overlay is opt-in; absent overlay = today's synth exactly.
- IR-equivalence is the acceptance bar: synth+overlay must emit the same IR command as the hand-rolled original (inspect-diff empty). If any clause can't be reproduced, it stays hand-authored and the overlay covers the rest (partial adoption is allowed; silent behavior change is NOT).
- Reuse existing IR command shapes + emitters (RULE-VOCAB-03). No new lowering.

## Open questions
- Surface shape of the overlay (a `crud { ... }` block on the resource vs. per-command stanzas) — decided in the ADR.
- Whether curated-input omission is expressed as `input excludes <fields>` or an explicit include list — decided in the ADR.
