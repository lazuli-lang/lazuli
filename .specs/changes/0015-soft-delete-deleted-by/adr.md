---
id: 0015
title: soft_delete → deleted_by — actor column on the soft-delete trait
type: adr
status: ready
created: 2026-05-31
depends_on: [0001, 0003]
parallel_safe: false
track: evolve/ship
---

# ADR — deleted_by on soft_delete

## Context
`soft_delete` is a resource-body trait (`crates/lazuli_keywords/src/registry/sections/s05.rs:147`) + a defaults key (`s11.rs:232`). It projects `deleted_at` only. The sibling trait `timestamps` (`s05.rs:141`) already projects an actor pair (`created_by`/`updated_by`), so the precedent for "trait projects an actor column populated from `ctx.actor`" exists. Pauta needs *who deleted* and hand-rolls `deleted_at: DateTime optional` + `deleted_by: ID optional` 54× across 10 features, each tagged `# Soft-delete` (e.g. `features/media_price_tables/media_price_tables.lzi:31-33`, `agency/agency.lzi:80-82`, `workflow_templates/workflow_templates.lzi:39,64`). Hostpoint is hard-delete and uses the trait 0×. Spec 0003 migrated Pauta create/update to `conventions [crud]` but **deferred delete** precisely because the canonical `crud` delete synth is not soft-delete-aware.

## Decision
1. **Extend the trait, don't add a new one.** `soft_delete` gains an optional actor projection: `soft_delete by` (or equivalent surface) projects a `deleted_by` actor column alongside `deleted_at`, populated from `ctx.actor` on the soft-delete write — mirroring `timestamps`' `created_by`/`updated_by` exactly. Bare `soft_delete` stays `deleted_at`-only (back-compat).
2. **Make the `crud` delete synth soft-delete-aware.** A resource carrying `soft_delete` gets a soft-deleting `delete` command under `conventions [crud]` (sets `deleted_at`/`deleted_by`, excluded from default reads), instead of a hard `DELETE`. This is the unblock for Pauta to finally adopt `[crud]` delete.
3. **Currency-of-actor stays app data, column-only.** The actor column is an `ID` referencing the actor; no locale/role semantics baked in (locale discipline).
4. **Cascade is out of scope** — see Alternatives.

## Alternatives considered
- **New `audited_soft_delete` trait** — rejected: forks the vocabulary, leaves bare `soft_delete` a second-class citizen, and doesn't mirror the `timestamps` precedent agents already know.
- **Leave it hand-rolled, just add a doctor nudge** — rejected: the recurring `# Soft-delete` comment proves authors *will* hand-roll the pair; a nudge without a trait to point at is noise.
- **Do cascade now** (soft-delete children when parent is soft-deleted) — rejected/deferred: cascade semantics are an open upstream question (index "Deliberately cut / deferred"). Doing the column without cascade is independently valuable and unblocks 0003; cascade can layer on later.

## Consequences
- **Positive**: Pauta drops 54 hand-rolled pairs; `[crud]` delete adoption unblocked; the `# Soft-delete` discoverability smell disappears (doc-curator finding closed); `soft_delete`/`timestamps` now symmetric.
- **Negative / cost**: grammar + IR + Go codegen + migration-DDL all touch the trait; the `crud` delete synth changes shape (existing hard-delete `[crud]` consumers must be unaffected — bare `soft_delete`-less resources keep hard delete).
- **Migration risk**: the 54 hand-rolled columns must map 1:1 to the trait's emitted columns (same names `deleted_at`/`deleted_by`, same nullability) so no Pauta schema diff is produced. Verified in the migrate gate.
