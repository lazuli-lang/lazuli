# PRD — 0014 Referential Guards

> Status: ready · Created: 2026-05-31 · Track: evolve/ship

## Problem

Both pilots hand-write the same delete-safety check over and over: before deleting (or hard-mutating) a row, load the target and reject the operation if any *live* row still references it. This "referential guard" is copy-pasted **15 times across 11 distinct features in both pilots**, and **7 of the handler files literally comment "same guard pattern as \<other feature\>"** — the strongest vocabulary signal in the audit.

The shape is identical every time: `load target → COUNT/EXISTS live references → reject if > 0`, with tenant scope and soft-delete awareness baked into the SQL.

Evidence:
- `pauta features/billing_config/handlers/guard_billing_type_in_use.go:13` — comment: *"Same guard pattern as agency_service_catalog.guard_no_active_service_items."* Body: `SELECT COUNT(*) FROM invoice WHERE billing_type_id = $1 AND tenant_id = $2 AND deleted_at IS NULL` → reject if `> 0`.
- `pauta features/workflow_templates/handlers/guard_template_not_in_use.go:13-21` — comment: *"Same guard pattern as billing_config.guard_billing_type_in_use, but uses EXISTS instead of COUNT."* Body: `SELECT EXISTS (SELECT 1 FROM job WHERE workflow_template_id = $1 AND deleted_at IS NULL)`.
- Family (both pilots): `guard_no_active_service_items`, `guard_broadcast_area_in_use`, `guard_category_in_use`, `guard_payment_terms_in_use`, `guard_no_active_invoice`, `guard_no_active_teams`, `guard_no_open_activities`, `guard_no_open_step_activities`, `guard_active_table_delete`, `guard_attachment_not_deleted` (pauta); `guard_no_active_reservations` (listings), `guard_no_published_reviews` (trust), `guard_conversation_empty` (messaging) (hostpoint).

This is exactly the kind of hand-rolled, comment-cross-referenced pattern that Lazuli exists to absorb into vocabulary.

## Users & jobs

- **Pilot devs / agents**: want to declare "you can't delete an X while live Y's point at it" in one line, instead of copy-pasting a COUNT/EXISTS guard and a `same pattern as…` comment.
- **Framework authors**: want the cross-referenced duplication gone — when a pattern is written 15× with "same as" comments, it's vocabulary, not application code.
- **Reviewers**: want the integrity rule visible on the resource/command, not buried in a `@fn` whose name is the only hint at what it checks.

## Requirements

- A declarative referential-integrity guard primitive — a precondition on a delete/mutate command that rejects when live references exist.
- Lowers to the scoped `EXISTS` / `COUNT` the pilots hand-write, **tenant-scoped and soft-delete-aware** (the `tenant_id = …` and `deleted_at IS NULL` predicates must be generated, not forgotten).
- A doctor rule that detects a `@fn` guard matching the "COUNT/EXISTS-then-reject" shape and suggests the primitive.
- Migrate all 15 hand-written guards in both pilots onto the primitive.
- Fill the idiom doc `docs/lazuli_way/referential-guards.md` (stub created in 0001).

## Success

- All 15 guards are declared via the primitive; the 7 "same guard pattern as…" comments are gone with the code.
- The doctor rule flags a deliberately-reintroduced hand-written guard and names the primitive.
- `cargo test -p lazuli_syntax referential_guard && cargo test -p lazuli_codegen_go referential_guard` green.
- Both pilots green: `lazuli check . && doctor . && go build ./...`.

## Out of scope

- Cascade *delete* (deleting the referencing rows) — this primitive only *restricts*; cascade is a separate, riskier decision deferred out.
- Cross-database / external-system references.
- FK enforcement at the schema/DDL level — this is an application-layer precondition that is tenant- and soft-delete-aware, which raw DB FKs are not.

## Risk note

The generated guard MUST preserve every scope the hand-written versions carry — most importantly `tenant_id` and `deleted_at IS NULL`. A guard that forgets soft-delete would reject deletes against already-deleted referencers (false positive) or, worse, a guard that forgets tenant scope would leak existence across tenants. Codegen correctness on these two predicates is the load-bearing gate.
