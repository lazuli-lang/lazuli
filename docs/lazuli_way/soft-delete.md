# Soft delete

## Reach for this

A resource is **soft-deleted** when "delete" means *mark the row gone*,
not *remove it* — the row stays for audit, undo, or referential safety,
and default reads exclude it.

Declare the trait, do not hand-roll the columns:

```text
resource MediaPriceTable
  name: Text required
  soft_delete by
```

- **`soft_delete`** projects a nullable `deleted_at` column. Default
  reads filter `deleted_at IS NULL`; `delete` becomes
  `UPDATE … SET deleted_at = now()` instead of `DELETE FROM …`.
- **`soft_delete by`** *also* projects a nullable `deleted_by` actor
  column (an `ID`). On the soft-delete write the runtime stamps
  `deleted_by = ctx.actor` alongside `deleted_at = now()` — exactly the
  "who deleted this row" question pilots used to hand-roll.

`soft_delete by` is the soft-delete sibling of `timestamps`: where
`timestamps` carries the lifecycle *when* (`created_at`/`updated_at`),
`soft_delete by` carries the deletion *when + who*
(`deleted_at` + `deleted_by`), both populated by the runtime, never by a
hand-written command body.

Because the canonical `crud` delete synth is **soft-delete-aware**, a
resource carrying `soft_delete` (or `soft_delete by`) can adopt
`conventions [crud]` for delete: the synthesized `delete_<resource>`
soft-deletes (stamping `deleted_at`, and `deleted_by` when the actor form
is set) instead of issuing a hard `DELETE`. A resource *without*
`soft_delete` keeps the hard-delete synth (unchanged).

> **Out of scope (column only):** cascading soft-delete
> (soft-deleting children when a parent is soft-deleted) is an open
> upstream question — `soft_delete by` stamps the column on *this* row
> only.

## Before (hand-rolled) / After (idiomatic)

**Before** — Pauta hand-rolled the `deleted_at` + `deleted_by` pair on
every soft-deletable resource (54× across 10 features), each tagged with
a recurring `# Soft-delete` comment, and re-stamped both columns by hand
in every delete command body
(`app/features/media_price_tables/media_price_tables.lzi:35-37`):

```text
resource MediaPriceTable
  …
  # Soft-delete
  deleted_at: DateTime optional
  deleted_by: ID optional
```

```text
command delete_media_price_table
  …
  deleted_at = ctx.now
  deleted_by = ctx.actor.id
```

**After** — the trait owns both columns; the `# Soft-delete` comment and
the hand-rolled field pair are gone, and the runtime stamps both columns
on the soft-delete write:

```text
resource MediaPriceTable
  …
  soft_delete by
```

Column names are fixed (`deleted_at`, `deleted_by`) and the `deleted_by`
column emits as a nullable `BIGINT`, byte-matching the hand-rolled
`deleted_by: ID optional` — migrating to the trait produces **zero**
schema drift.

## Enforced by

`VOCAB-SOFT-DELETE-ACTOR-001` — fires (advisory, `vocabulary` /
non-gating) on a resource that hand-rolls a `deleted_at` + `deleted_by`
field pair without the `soft_delete by` trait, and is silent once the
resource is migrated. Suppress an intentional hand-roll with
`# doctor:allow VOCAB-SOFT-DELETE-ACTOR-001 — reason "…"`.
