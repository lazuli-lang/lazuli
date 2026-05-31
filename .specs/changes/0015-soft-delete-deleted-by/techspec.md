---
id: 0015
title: soft_delete → deleted_by — actor column on the soft-delete trait
type: techspec
status: ready
created: 2026-05-31
depends_on: [0001, 0003]
parallel_safe: false
track: evolve/ship
test_gate: "cargo test -p lazuli_syntax soft_delete_deleted_by && cargo test -p lazuli_codegen_go soft_delete && lazuli check app && lazuli doctor app && go build ./... (pauta)"
agent: unassigned
---

# TechSpec — soft_delete carries deleted_by

## Approach
Extend the EXISTING `soft_delete` trait — do not add a new keyword. Add an optional actor projection that mirrors how `timestamps` already carries `created_by`/`updated_by`. Two halves: (a) the trait projects `deleted_by` + populates it from `ctx.actor` on soft-delete; (b) the canonical `crud` delete synth becomes soft-delete-aware so Pauta can adopt `conventions [crud]` for delete (the half 0003 deferred). Then migrate Pauta's 54 hand-rolled `deleted_at`/`deleted_by` pairs to the trait and finish the deferred delete-command migration. Column-only — cascade is out of scope.

## Surface
**Modify (language):**
- `crates/lazuli_keywords/src/registry/sections/s05.rs:147` — extend the `soft_delete` resource-body stmt to accept the actor-projection form (`soft_delete by`), with completion/hover copy mirroring `timestamps` (`s05.rs:141`).
- `crates/lazuli_keywords/src/registry/sections/s11.rs:232` — same extension on the `defaults` `soft_delete` key.
- `crates/lazuli_syntax/src/parser/lzi/resource/mod.rs` (+ `body_handlers.rs`) — parse the actor-projection modifier on `soft_delete`.
- `crates/lazuli_syntax/src/ast/resource.rs` (+ `resource_p1.rs`) — AST: `SoftDelete { actor: bool }` (or `deleted_by: Option<…>`), mirroring the `timestamps` actor flags.
- `crates/lazuli_ir/src/nodes/resource/mod.rs` — IR: project `deleted_by` column (type `ID`, nullable) when the actor form is set; mark its write-origin `ctx.actor`.
- `crates/lazuli_codegen_go/src/emitter/resource/struct_emit_p1.rs` — emit the `DeletedBy` struct field.
- `crates/lazuli_codegen_go/src/emitter/migration_ddl/create_table.rs` — emit the `deleted_by` DDL column.
- `crates/lazuli_codegen_go/src/runtime/resource.rs` — populate `deleted_by` from `ctx.actor` on the soft-delete write; exclude soft-deleted rows from default reads (existing `deleted_at` behavior, extended).
- `crates/lazuli_analyzer/src/...conventions_crud_synth...` — make the `crud` **delete** synth soft-delete-aware: when the resource carries `soft_delete`, the synthesized `delete` command soft-deletes (stamps `deleted_at`/`deleted_by`) instead of hard `DELETE`.

**Create (enforcement):**
- `crates/lazuli_doctor/src/vocab/soft_delete_actor_001.rs` — doctor rule `VOCAB-SOFT-DELETE-ACTOR-001`: fires on a resource declaring a hand-rolled `deleted_at` + `deleted_by` field pair (no `soft_delete by` trait), suggesting the trait. (Place next to the existing `vocab_*` rules; register in the doctor rule set.)

**Modify (teaching):**
- `docs/lazuli_way/soft-delete.md` — fill the stub (idiom shape from 0001).
- `lazurite/templates/default/CLAUDE.md.tmpl` + `AGENTS.md.tmpl` — add the soft-delete idiom bullet ("Reach for `soft_delete by`, not hand-rolled `deleted_at`+`deleted_by`").
- `docs/keyword-reference.md` / `docs/grammar.lzi.md` — document the `soft_delete by` form.

**Migrate (pilot — Pauta only; hostpoint 0×):**
- Replace the 54 hand-rolled pairs across 10 features with `soft_delete by`: `media_price_tables.lzi` (lines 31,57,72,91,108), `agency/agency.lzi:80-82`, `workflow_templates/workflow_templates.lzi:39,64`, `billing_config/billing_config.lzi:68`, `agency_service_catalog/agency_service_catalog.lzi:17`, `geography_broadcast/geography_broadcast.lzi:44`, `account/account.lzi:41`, `media_vehicles/media_vehicles.lzi:83`, `attachments/attachments.lzi:61`, `job_lifecycle/job_lifecycle.lzi:63`.
- Adopt `conventions [crud]` for **delete** on the migrated resources (finish the 0003-deferred delete migration).
- Remove the now-redundant `# Soft-delete` comments.

## Contracts
- **Trait surface**: `soft_delete by` projects `deleted_at: DateTime` + `deleted_by: ID` (both nullable, null = live row). Bare `soft_delete` stays `deleted_at`-only (back-compat — DO NOT change its emitted shape).
- **Column names are FIXED**: `deleted_at`, `deleted_by` — must byte-match Pauta's hand-rolled names so migration produces **zero** schema diff.
- **Populate contract**: on the soft-delete write the runtime sets `deleted_at = now()`, `deleted_by = ctx.actor`. Reads exclude `deleted_at IS NOT NULL` by default (unchanged from today).
- **crud delete synth**: resource with `soft_delete` + `conventions [crud]` → `delete` command soft-deletes. Resource WITHOUT `soft_delete` + `[crud]` → hard delete (unchanged).
- **DoD block (embedded verbatim — see Gate).**
- **Idiom-doc shape (from 0001):** `# Soft delete` / `## Reach for this` / `## Before (hand-rolled) / After (idiomatic)` (cite `media_price_tables.lzi:31-33` before, `soft_delete by` after) / `## Enforced by VOCAB-SOFT-DELETE-ACTOR-001`.

## Plan — for the executing agent
1. **BUILD-lang**: extend the `soft_delete` keyword (s05/s11) + parser + AST to accept `soft_delete by`. Unit test `soft_delete_deleted_by` in `lazuli_syntax`.
2. **BUILD-ir/codegen**: project `deleted_by` in IR; emit struct field + DDL column; populate from `ctx.actor`; extend read-exclusion. Test `soft_delete` in `lazuli_codegen_go`.
3. **BUILD-synth**: make the `crud` delete synth soft-delete-aware; extend `conventions_crud_synth_tests` to assert soft-delete delete when `soft_delete` present, hard delete otherwise.
4. **ENFORCE**: write `VOCAB-SOFT-DELETE-ACTOR-001` + tests (fires on hand-rolled pair, silent when trait used).
5. **MIGRATE**: replace the 54 pairs with `soft_delete by` across the 10 Pauta features; adopt `[crud]` delete; drop `# Soft-delete` comments. Run `lazuli check && lazuli doctor && go build ./...` in pauta until clean; diff the generated migration to confirm zero column drift.
6. **TEACH**: fill `docs/lazuli_way/soft-delete.md`; add the CLAUDE.md.tmpl + AGENTS.md.tmpl bullet; update keyword-reference + grammar docs.

## Tests first (TDD)
- [ ] `soft_delete_deleted_by` (lazuli_syntax) — `soft_delete by` parses; bare `soft_delete` still parses unchanged.
- [ ] `soft_delete` (lazuli_codegen_go) — `deleted_by` struct field + DDL column emitted; populated from `ctx.actor`; bare `soft_delete` emits no `deleted_by`.
- [ ] `crud_delete_soft_aware` (analyzer conventions_crud_synth_tests) — `[crud]` + `soft_delete` → soft-deleting delete; `[crud]` without → hard delete.
- [ ] `vocab_soft_delete_actor_001` — fires on hand-rolled `deleted_at`+`deleted_by`; silent on `soft_delete by`.
- [ ] `pauta_zero_schema_drift` (gate) — migrated Pauta produces no `deleted_*` column diff vs pre-migration.

## Gate

## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.

**4 concrete gates:**
1. `cargo test -p lazuli_syntax soft_delete_deleted_by && cargo test -p lazuli_codegen_go soft_delete` green.
2. `lazuli check app && lazuli doctor app && go build ./...` clean in pauta-web (hostpoint untouched — 0× usage); generated migration shows **zero** `deleted_*` column drift.
3. `docs/lazuli_way/soft-delete.md` filled per the 0001 shape; CLAUDE.md.tmpl + AGENTS.md.tmpl bullet present.
4. `VOCAB-SOFT-DELETE-ACTOR-001` fires on the old hand-rolled pair and is silent after migration; its code is named in the idiom doc.

## Risks & rollback
- **Column-name mismatch** → schema diff against Pauta's live DB → mitigation: lock emitted names to `deleted_at`/`deleted_by`; `pauta_zero_schema_drift` gate.
- **`crud` delete synth regresses hard-delete consumers** → mitigation: behavior keyed strictly on presence of `soft_delete`; edge-case test asserts hard delete when absent.
- **Scope creep into cascade** → explicitly OUT (index deferred item); the trait stamps the column only.
- **Rollback**: `git revert`. Language change is additive (bare `soft_delete` unchanged); Pauta migration revert restores the hand-rolled pairs.
