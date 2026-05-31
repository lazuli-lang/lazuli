---
id: 0015
title: soft_delete → deleted_by — actor column on the soft-delete trait
type: prd
status: ready
created: 2026-05-31
depends_on: [0001, 0003]
parallel_safe: false
track: evolve/ship
---

# PRD — soft_delete carries deleted_by

## Problem
The `soft_delete` trait exists but only projects `deleted_at`. Every pilot that needs *who* deleted a row hand-rolls a `deleted_at` + `deleted_by` pair. Pauta does this **54×** across **10 features**, each marked with a recurring `# Soft-delete` comment — a smell that the trait was looked at and found insufficient, so the author reached past it. Because the canonical `crud` synth's delete path is not soft-delete-aware, Pauta cannot adopt `conventions [crud]` for the delete half (the blocker that spec 0003 explicitly deferred). The `timestamps` trait already shows the shape we want: it carries `created_by`/`updated_by` actor columns. `soft_delete` should mirror that for `deleted_by`.

## Who hurts
- **Pauta authors** — 54 hand-copied `deleted_at`/`deleted_by` pairs that drift (column names, populate-from-ctx, read-exclusion) and can't migrate delete to `[crud]`.
- **Spec 0003** — left the Pauta delete-command migration unfinished pending this.
- **doc-curator** — the `# Soft-delete` comment recurring 10× is a discoverability finding: the idiom exists but isn't reachable for from where authors stand.

## What we ship
1. **Language**: extend the existing `soft_delete` trait so it can optionally project a `deleted_by` actor column (populated from `ctx.actor` on soft-delete), mirroring `timestamps` → `created_by`/`updated_by`.
2. **Synth**: make the canonical `crud` delete synth soft-delete-aware, so a resource with `trait soft_delete` gets a soft-deleting `delete` command under `conventions [crud]`.
3. **Migrate**: replace Pauta's 54 hand-rolled `deleted_at`/`deleted_by` pairs with the trait; finish the Pauta delete-command migration deferred by 0003.
4. **Teach**: fill `docs/lazuli_way/soft-delete.md` (stub from 0001).
5. **Enforce**: a doctor rule that fires on the hand-rolled `deleted_at` + `deleted_by` shape and suggests the trait.

## Out of scope
- **Cascading soft-delete** semantics (delete-children-when-parent-soft-deleted) — open question upstream (see index "Deliberately cut / deferred"). This spec does the **column only**.
- Hostpoint uses soft_delete 0× (hard-delete model) — no hostpoint migration.

## Success
Pauta is on `trait soft_delete` (with `deleted_by`) everywhere it had a hand-rolled pair, adopts `conventions [crud]` for delete, and stays `lazuli check` + `doctor` + `go build` clean. The `# Soft-delete` comment is gone from the 10 features.
