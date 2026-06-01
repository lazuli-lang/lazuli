---
id: 0003
title: pauta-crud-migration
kind: prd
track: tell/pilot
depends_on: [0001, 0002]
parallel_safe: false
status: ready
created: 2026-05-31
---

# PRD — Migrate Pauta-web onto `conventions [crud]`

## Problem

Pauta-web (`C:\Users\lucas\dev\pauta-web-monorepo\app`) hand-rolls CRUD command
boilerplate across 13 features with **zero** convention adoption (verified:
`Select-String 'conventions \['` over `app/features/**/*.lzi` returns 0). The same
`create_X`/`update_X`/`delete_X` shape is copy-pasted per resource and has started
to drift (e.g. `update_supplier` updates only `updated_at`; create/update inputs
diverge from resource fields per the `VOCAB-SHADOW-RECORD` waiver). `conventions
[crud]` exists to delete exactly this, and 0002's inverse linter now points at
every candidate.

## Who

- Pauta-web maintainers (the pilot's owners).
- Lazuli core, using Pauta as the canonical proof the convention scales.

## What

Per migration-eligible resource: replace the hand-rolled `create_X`/`update_X`
(and `read_X`/`list_X`/`*_by_id` where they match the synth) with `conventions
[crud]`, proving via IR diff that the synthesized members are equivalent to the
removed ones. **Delete stays explicit** wherever it is soft-delete (see blocker).
Custom verbs (convert_prospect, suspend_customer, activate_*, mark_defaulter,
add_representative, upsert_*, …) also stay hand-rolled — they are not crud.

## Why now

- 0002 ships the linter that finds candidates and verifies (post-migration) that
  no further nudge fires on create/update.
- Pauta is the canonical-shape pilot; if the convention can't carry Pauta's CRUD,
  the convention is wrong. This is the "tell" proof for the teach doc.

## Blocker verification (FIRST sub-step — must precede any edit)

Read one Pauta `delete_X` before touching anything. Confirmed on read of
`features/customer_management/customer_management.lzi`: `delete_customer` does NOT
hard-delete — it sets `deleted_at = ctx.now` / `deleted_by = ctx.actor.id`, the
`Customer` resource declares `retention 5y then anonymize` (LGPD), and every
PII-bearing Pauta resource follows the same soft-delete + retention posture
(`Contact`, `Supplier`, `MediaPriceTable` children all carry `deleted_at` +
`retention`/cascade). The canonical crud synth's delete is **HARD**. Therefore:
adopt `[crud]` for **create/update (and read/list/by_id) ONLY**; leave every
`delete_X` hand-rolled until spec 0015 (soft_delete → deleted_by) lands.

## Migration order (by hand-rolled command count, descending)

Verified counts (`command` lines per `.lzi`):
1. `customer_management` — 17
2. `supplier` — 14
3. `media_price_tables` — 17 (3rd by the task's media-first weighting; highest
   sub-item fan-out, all soft-delete)
4. then the rest: `job_steps_activities` (12), `agency` (10), `job_lifecycle` (9),
   `workflow_templates` (7), `agency_service_catalog` (6), `billing_config` (6),
   `media_vehicles` (6), `reports_exports` (6), `account` (5), `geography_broadcast`
   (3), `attachments` (3), `hoxo_financial_integration` (2), `notifications` (2),
   `admin_panel` (1). (`dashboard`, `operation_audit_log` have 0 commands.)

(13 features carry migratable CRUD; ~126 command lines total today, 0 adoption.)

## Success criteria

- Each migrated feature: `lazuli check . && lazuli doctor . && go build ./...` all
  green in the Pauta repo.
- `lazuli inspect --expand=all` IR diff before/after shows the synthesized
  create/update/read/list members are equivalent to the removed hand-rolled ones
  (same names, kinds, inputs, emitted events).
- After migration, 0002's `VOCAB-CRUD-SYNTH-AVAILABLE-001` no longer fires on the
  migrated create/update surface (it may still note the intentionally-kept soft
  delete; suppress with `# doctor:allow` where intentional, with a reason).
- `docs/lazuli_way/crud-by-convention.md` carries real Pauta before/after excerpts.

## Non-goals

- Migrating `delete_X` (soft-delete) — deferred to 0015.
- Migrating custom verbs (convert/suspend/activate/mark/upsert/add_representative).
- Migrating resources whose synth inputs would NOT match the hand-rolled inputs
  (e.g. inputs that omit fields like `situation`/`is_active`) — keep those explicit
  and file a synth-fidelity bug.
- Changing Pauta runtime behavior. Source refactor proven IR-equivalent.
- Touching framework crates (that's 0002).

## Risks

- An IR diff that is **not** equivalent (synth input shape differs from the
  hand-rolled input). Mitigation: the IR diff is a hard gate; on divergence keep
  that member hand-rolled and file a synth-fidelity bug.
- Accidentally dropping the LGPD delete. Mitigation: blocker step + per-feature
  diff review; `delete_X` must remain present and unchanged after every edit.
- Synth `create_<r>` requiring a non-tenant required field
  (`crud_synth_no_required_fields`): Pauta resources have plenty, so low risk, but
  the per-feature `lazuli doctor` run catches it.

---

## RESOLVED 2026-06-01 — bounded by intentional input decoupling (after 0004+0015+0018)

Three language specs grew `conventions [crud]` to close the gaps 0003 needed: 0004 (defaults inherit rate_limit/audit), 0015 (soft-delete-aware crud delete), 0018 (crud overlay: policy/validate/assign/emits/input-excludes). After ALL THREE, `VOCAB-CRUD-SYNTH-AVAILABLE-001` still fires **0×** on Pauta. The overlay closed 4 of 5 gaps (policy, validate, assign, emits — all byte-identical). The **5th is structural and intentional**: Pauta's CRUD inputs are deliberately decoupled from resource shape:
- FK fields are submitted as `<rel>_id: ID` (e.g. `category_id: ID`, `agency_id: ID`), while the synth derives the relation-typed field (`category: CustomerCategory`). `input excludes` can remove but not rename/retype.
- Inputs intentionally omit immutable fields and are documented to "drift apart" from the resource shape — see `geography_broadcast.lzi:91` VOCAB-SHADOW-RECORD waiver: *"create_broadcast_area input intentionally mirrors the BroadcastArea field shape; extracting a shared record now would couple surfaces meant to drift apart."*
- Hand-authored soft-delete as `updates Customer` (set deleted_at/deleted_by) vs synth `Deletes` — different IR nodes (same behavior via 0015).

**Conclusion:** this is NOT boilerplate the synth should absorb — it's intentional API-contract design (the submission shape ≠ the storage shape). `conventions [crud]`'s field-derived input is the wrong model for it, and growing the overlay to express arbitrary input renames/retypes would reinvent the command grammar inside the overlay (ADR-0018 explicitly forbids this — "do not grow the overlay into a macro language").

**Disposition:** 0003's 84-command headline does NOT happen, and that is the CORRECT outcome. The audit's "0/84 non-adoption" was read as pilot debt; it is actually a faithful signal that these commands are not CRUD-skeleton-shaped. The DRY win the audit promised is delivered instead by 0004 (defaults hoist, ~445 lines real) + 0014 (referential guards) + 0015 (soft-delete trait, 25 resources) + 0016 (Money) — all of which DID migrate. `conventions [crud]` + overlay (0018) is now viable for resources whose submission shape == storage shape (future features; Hostpoint-style); it is correctly NOT forced onto Pauta's bespoke-input features. The inverse linter (0002, upgraded by 0018) is the permanent guard: it fires only when adoption is genuinely safe.

0003 is CLOSED as resolved-bounded. No further synth growth pursued (would violate ADR-0018).
