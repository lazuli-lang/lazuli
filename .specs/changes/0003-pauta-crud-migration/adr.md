---
id: 0003
title: pauta-crud-migration
kind: adr
track: tell/pilot
depends_on: [0001, 0002]
parallel_safe: false
status: ready
created: 2026-05-31
---

# ADR — Partial crud adoption (create/update/read/list), delete stays explicit

- Status: Accepted
- Date: 2026-05-31
- Deciders: Lazuli core + Pauta-web owners
- Relates: docs/architecture-decisions/0002-resource-conventions-crud-me.md

## Context

Pauta's hand-rolled `delete_X` commands are soft-delete with LGPD retention.
Verified by reading `features/customer_management/customer_management.lzi`:
`delete_customer` sets `deleted_at`/`deleted_by` (not a row delete) and `Customer`
carries `retention 5y then anonymize`; the same posture holds across `Contact`,
`Supplier`, and the `media_price_tables` resources. The shipped crud synth
produces a **hard** delete. Adopting `[crud]` wholesale would either silently swap
a hard delete in (data-loss / LGPD violation) or — because convention members are
overridable by name (`crates/lazuli_analyzer/src/conventions/mod.rs`
`ConventionOrigin::AuthorOverride`) — be suppressed by the surviving hand-written
`delete_X`, in which case the delete boilerplate stays and we gain nothing on that
member.

## Decision

Adopt `conventions [crud]` on each eligible Pauta resource for the
**create / update / read / list (+ `*_by_id` lookup)** members only. **Keep every
`delete_X` hand-written** (its `deleted_at`/`deleted_by` body + the resource's
`retention` unchanged); override-by-name means the synthesized hard delete is
suppressed automatically. Custom verbs (convert_prospect, suspend_customer,
activate/deactivate_*, mark_defaulter, add_representative, upsert_*_price_entry,
…) also stay hand-written — not part of the crud surface.

Migration is proven member-by-member with `lazuli inspect --expand=all`: the
synthesized create/update/read/list must be IR-equivalent to the removed
hand-rolled members (name, kind, input, emitted event). Any non-equivalence keeps
that member hand-rolled and files a synth-fidelity bug rather than shipping a
behavior change. Resources whose hand-rolled create/update inputs deliberately
diverge from their fields (the `VOCAB-SHADOW-RECORD` waiver cases, e.g.
`media_price_tables`) are migrated only where the synth-derived inputs still match;
otherwise they stay explicit.

## Alternatives considered

- **Full adoption including delete.** Rejected: swaps soft+LGPD delete for hard
  delete — a compliance/data-loss regression.
- **Wait for 0015 and migrate everything at once.** Rejected: blocks ~126 lines of
  create/update cleanup behind an unscheduled spec; partial adoption is incremental
  and non-destructive.
- **Keep delete synthesized but re-add soft_delete via override.** Rejected: that
  *is* a hand-written `delete_X`; clearer to leave it explicit and labelled.
- **Migrate all 13 features in one commit.** Rejected: not parallel_safe and the
  IR-diff gate is per-feature; one feature per commit keeps each diff reviewable.

## Consequences

- The create/update/read/list boilerplate collapses to one `conventions [crud]`
  line per resource; delete + custom verbs remain.
- The migrated surface stops tripping 0002's linter for create/update; the linter's
  soft-delete carve-out means it agrees with this decision (never suggests
  replacing the soft delete).
- A clean hand-off point for 0015: when soft_delete synth lands, the only remaining
  hand-rolled delete commands are the ones this ADR intentionally kept.
- Pauta becomes the real before/after evidence in
  `docs/lazuli_way/crud-by-convention.md` (Teach gate).
