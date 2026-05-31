---
id: 0003
title: pauta-crud-migration
type: techspec
track: tell/pilot
depends_on: [0001, 0002]
parallel_safe: false
status: ready
created: 2026-05-31
test_gate: "lazuli check . && lazuli doctor . && go build ./..."
agent: unassigned
---

# TechSpec — Pauta-web crud-by-convention migration

## Approach

Migrate Pauta-web's hand-rolled create/update (+read/list/by_id) commands onto
`conventions [crud]`, one feature per commit, proving IR equivalence per member
with `lazuli inspect --expand=all`. Delete stays explicit (soft-delete/LGPD). Not
parallel-safe: contends on the Pauta `.lzi` files; run serially after 0002 merges.

## Grounding (verified by reading)

- Repo: `C:\Users\lucas\dev\pauta-web-monorepo\app\features\` — 19 feature dirs,
  one `.lzi` each, ~126 `command` lines total, **0** `conventions [...]` opt-ins.
- `customer_management/customer_management.lzi` (17 cmds): hand-rolled
  `create_customer`, `update_customer`, `delete_customer` (soft: sets
  `deleted_at`/`deleted_by`; `Customer` carries `retention 5y then anonymize`),
  plus custom `convert_prospect`/`suspend_customer`/`reactivate_customer`/
  `mark_defaulter`/`clear_defaulter`, and the `CustomerCategory`/`ProductService`/
  `Contact` CRUD sets. Queries: `list_customers`, `customer_by_id`, etc.
- `supplier/supplier.lzi` (14 cmds): `create/update/delete_supplier` (delete soft)
  + `add_representative`/`upsert_supplier_price_entry`/`delete_*` hard-deletes on
  `Representative`/`SupplierPriceEntry`.
- `media_price_tables/media_price_tables.lzi` (17 cmds): `MediaPriceTable` +
  `TvRadioProgram`/`PrintSection`/`PrintPlacement`/`OutdoorLocation`, every delete
  soft (`deleted_at`/`deleted_by`); carries `VOCAB-SHADOW-RECORD` /
  `VOCAB-EVENT-PAYLOAD` / `VOCAB-TESTS-MISSING` waivers.
- Synth contract: `crates/lazuli_analyzer/src/conventions/mod.rs` +
  `feature.rs` §5 — synthesizes `create/update/delete_<r>` + `read/list_<r>`;
  delete is hard; author members override by name.
- Linter (dep 0002): `VOCAB-CRUD-SYNTH-AVAILABLE-001` flags candidates and is
  silent post-migration on create/update.

## Surface

**Modify (per feature, in order):**
- `app/features/<feature>/<feature>.lzi` — add `conventions [crud]` to each
  eligible resource; delete the matched hand-rolled `create_X`/`update_X` (and
  `read_X`/`list_X`/`*_by_id` where they match the synth). Keep `delete_X` (soft)
  + custom verbs.

**Fill (Teach gate, supersedes 0001's stub):**
- `docs/lazuli_way/crud-by-convention.md` (in the Lazuli framework repo, created as
  a stub by 0001) — replace the placeholder before/after with the real
  customer_management excerpt.

**Create (per feature, transient, NOT committed unless useful):**
- IR snapshots `inspect-before.json` / `inspect-after.json` for the diff gate (a
  scratch artifact; the diff result is what matters).

## Contracts

**Migration order** (by hand-rolled command count; task pins media-first for the
first three):
1. customer_management (17) — Customer, CustomerCategory, ProductService, Contact
2. supplier (14) — Supplier, SupplierCategory, SupplierProductService (+
   Representative/SupplierPriceEntry stay explicit: hard-delete custom verbs)
3. media_price_tables (17) — MediaPriceTable + 4 sub-resources
4. the rest, descending: job_steps_activities, agency, job_lifecycle,
   workflow_templates, agency_service_catalog, billing_config, media_vehicles,
   reports_exports, account, geography_broadcast, attachments,
   hoxo_financial_integration, notifications, admin_panel.

**Per-resource adoption rule (the invariant across all features):**
- adopt `[crud]` for create/update/read/list/by_id ONLY;
- NEVER adopt synth delete while the hand-rolled `delete_X` is soft
  (`deleted_at`/`retention`) — keep `delete_X` verbatim;
- keep all custom verbs verbatim;
- a member migrates only if its synth equivalent is IR-equivalent to the removed
  one (the `inspect --expand=all` diff is the judge).

**Worked example (customer_management → Customer).**

Before (excerpt):
```lzi
resource Customer
  agency_id: ID required
  legal_name: Text required @pii.identity
  ...
  retention 5y then anonymize

command create_customer
  input { agency_id, legal_name, trade_name, cnpj, email, ... }
  creates Customer ...
  emits customer_created

command update_customer
  route id: ID
  input { legal_name, trade_name, email, ... }
  updates Customer where id = route.id ...
  emits customer_updated

command delete_customer
  route id: ID
  updates Customer where id = route.id
    deleted_at = ctx.now
    deleted_by = ctx.actor.id
  emits customer_deleted
```

After (excerpt):
```lzi
resource Customer
  agency_id: ID required
  legal_name: Text required @pii.identity
  ...
  conventions [crud]
  retention 5y then anonymize
  # create_customer / update_customer / read_customer / list_customers synthesized.
  # delete kept explicit: soft-delete + LGPD retention (synth delete is hard, spec 0015).

command delete_customer
  route id: ID
  updates Customer where id = route.id
    deleted_at = ctx.now
    deleted_by = ctx.actor.id
  emits customer_deleted
# convert_prospect / suspend_customer / reactivate_customer / mark_defaulter / clear_defaulter unchanged.
```

IR-equivalence note: the removed `create_customer`/`update_customer` carried
explicit field inputs; the synth equivalents derive inputs from resource fields.
The `inspect --expand=all` diff is the gate — if synth-derived inputs differ from
the hand-rolled inputs (a field omitted/renamed), KEEP that member hand-rolled and
file a synth-fidelity bug instead of shipping a behavior change. (customer_management's
create/update inputs map 1:1 to fields, so they migrate; media_price_tables
sub-resources are checked individually because of their shadow-record waiver.)

## Plan — for the executing agent

1. **Step 0 — blocker verification (before any edit):** read
   `customer_management.lzi`'s `delete_customer`. Confirm soft-delete +
   `retention` (already verified in this spec). Record the confirmation in the
   commit message of the first feature. If any feature's delete is later found to
   be HARD, that delete MAY adopt synth delete — but every delete read so far is
   soft.
2. For each feature in order:
   a. `lazuli inspect --expand=all > inspect-before.json` (scope to the feature).
   b. Edit the `.lzi`: add `conventions [crud]`; remove matched hand-rolled
      create/update (+read/list/by_id); keep delete + custom verbs.
   c. `lazuli inspect --expand=all > inspect-after.json`; diff. The synthesized
      create/update/read/list members MUST match the removed ones (name, kind,
      input, emitted event); `delete_X` + custom verbs byte-identical.
   d. `lazuli check . && lazuli doctor . && go build ./...` — all green.
   e. `lazuli doctor .` no longer emits `VOCAB-CRUD-SYNTH-AVAILABLE-001` for the
      migrated create/update surface; any residual note targets the kept soft
      delete — suppress with `# doctor:allow VOCAB-CRUD-SYNTH-AVAILABLE-001 —
      reason "..."` where intentional.
   f. Commit (one feature per commit).
3. After the last feature, fill `docs/lazuli_way/crud-by-convention.md` with the
   customer_management before/after excerpt + the IR-equivalence proof method.

## Tests first (TDD)

This is a migration; the "tests" are the gate commands run per feature:

- [ ] `inspect_diff_equivalent` — for each feature, the create/update/read/list IR
      after migration equals the removed hand-rolled members (manual/scripted diff
      of `inspect --expand=all`).
- [ ] `check_clean` — `lazuli check .` exits 0 per feature.
- [ ] `doctor_clean` — `lazuli doctor .` exits 0; no new errors; the inverse linter
      no longer fires on migrated create/update.
- [ ] `go_build` — `go build ./...` exits 0 per feature.
- [ ] `delete_preserved` — each migrated feature still has its soft `delete_X`
      command verbatim (grep the `.lzi` post-edit).

## Gate — Definition of Done (Lazuli feature gate)

> Embedded verbatim from `0001-teaching-spine/techspec.md`, made concrete for 0003.

```
## Definition of Done (Lazuli feature gate)
1. BUILD: implemented; `cargo test -p <crate>` green for the new rule/grammar/codegen.
2. MIGRATE: every pilot that needed it is on it; `lazuli check && lazuli doctor && go build ./...` clean in hostpoint and/or pauta-web.
3. TEACH: docs/lazuli_way/<slug>.md filled (idiom → before/after pilot excerpt → enforcing doctor rule); scaffold CLAUDE.md.tmpl + AGENTS.md.tmpl bullet added.
4. ENFORCE: a doctor rule fires on the old hand-rolled shape OR the scaffold seed demonstrates the idiom. The rule code is named in the idiom doc.
A spec that skips gate 3 or 4 is NOT done. The RULE-team grader blocks it.
```

Concrete for 0003:
1. **BUILD** — no framework code in this spec; "build" = the migration compiles:
   per feature, `lazuli check . && lazuli doctor . && go build ./...` green in the
   Pauta repo (the `test_gate`), and the `inspect --expand=all` IR diff proves
   create/update/read/list equivalence.
2. **MIGRATE** — every eligible Pauta resource across the 13 CRUD-bearing features
   carries `conventions [crud]`; each delete stays explicit; the whole repo is
   green under the gate commands.
3. **TEACH** — `docs/lazuli_way/crud-by-convention.md` FILLED with the real Pauta
   before/after (customer_management at minimum), the soft-delete carve-out
   rationale, and the IR-equivalence proof method. Names `VOCAB-CRUD-SYNTH-
   AVAILABLE-001` as the enforcing rule.
4. **ENFORCE** — proven by 0002's `VOCAB-CRUD-SYNTH-AVAILABLE-001`: it fired on the
   pre-migration hand-rolled resources and is silent on the migrated create/update
   surface afterward. Rule code named in the idiom doc.

## Risks & rollback

- IR diff not equivalent → keep that member explicit + file synth-fidelity bug; do
  not force the migration.
- Soft delete accidentally dropped → `delete_preserved` test + per-feature review;
  delete body must be byte-identical post-edit.

**Rollback:** per-feature commits → `git revert` the offending feature's commit;
each feature is independent, so a bad migration rolls back without touching others.

## Parallel-safety

`parallel_safe: false` — every step edits Pauta `.lzi` files and re-runs
`lazuli check`/`go build` over the same tree; concurrent feature edits race the
build/inspect gates. Run features serially in the order above. Independent of 0002
(different repo) but depends on 0002 being merged so the linter can verify the
post-migration surface.

## Teach-cell note (supersedes 0001's stub)

0001 created `docs/lazuli_way/crud-by-convention.md` as a stub. This spec's teach
cell does NOT re-create it; it replaces the stub's placeholder example with the
real migration's before/after now that the migration proves them.
