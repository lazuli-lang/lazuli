# Migrations Lowering Scope (pre-design)

**Status**: pre-design investigation. Resolves a side-quest blocker
discovered during the `bucket=migrations` pipeline Stage 1+2 inventory,
so Stage 3 (design-language) runs against the correct scope.

**Audience**: language team, runtime team, anyone touching the
migrations bucket cycle.

**Date**: 2026-05-11.

## Contexto

The `bucket=migrations` inventory cataloged every migration-adjacent
construct in the canonical fixture, IR, doctor, LSP, codegen, and
runtime. Roadmap §1.6 (`docs/roadmap.md:111-114`) lists the DL targets:

- `tenant_migration` kind.
- `index` / `foreign_key` / `constraint` / `enum_column` / `extension`
  / `trigger` / `generated_column` / `partition` decorators on
  resources.
- Doctor rule: schema drift detection.

Framework-coverage audit §8
(`docs/audit/framework-coverage-1400.md:181-187`) puts the section at
**L=4, DL=10, DF=22, F=2, N=2** (40 features). The DL bucket is
exactly the eight decorators above plus `tenant_migration` and the
doctor drift rule. DF lives in Drusa (atlas execution, locking,
online migrations); F (2) is irreversible-migration markers + schema
snapshots; N (2) is the "Go migrations file format" anti-pattern
(Lazuli re-lowers from source, no parallel author file).

The picture today is more starved than auth or storage were before
their cycles. The defining anchors:

- The fixture authors **field-level identity hints** at
  `examples/full-capsule/full-capsule.lzi:46`
  (`previously migrated Account`) and `:51`
  (`previously migrated status` on
  `lifecycle_stage: CustomerStatus = lead`). These are the only
  migration-adjacent decorations on resources in the canonical fixture.
- The fixture authors **deploy-side migration policy** at
  `examples/full-capsule/app.lzi:91-95`
  (`migrations before_deploy / migration_lock required /
  destructive_migrations require_approval / rollback on_failed_healthcheck`)
  and per-environment overrides at
  `examples/full-capsule/profiles.lzi:16,26`. These already lift to
  `AppDeploy` IR (`crates/lazuli_ir/src/lib.rs:1463-1467`) and project
  through `lazuli inspect`.
- **Zero authored decorators** for the eight resource-level constructs
  the roadmap lists (`index`, `foreign_key`, `constraint`,
  `enum_column`, `extension`, `trigger`, `generated_column`,
  `partition`). `unique` is present as a resource-level constraints
  block (`full-capsule.lzi:69-71`, `:583-584`) but is conceptually a
  *uniqueness invariant*, not an index strategy — its IR lift
  (`Resource.constraints: Vec<Constraint>`,
  `crates/lazuli_ir/src/lib.rs:319`) is already wired.
- **`tenant_migration` kind is not authored** anywhere in the fixture.
- **`migrations.md`** (`docs/migrations.md:1-60`) sketches a
  `lazuli plan` planner over `previously`/IR diff and lists 12 change
  kinds with default-risk classes (low → critical). The doc names a
  CLI command — `lazuli plan` — but **no such command exists in
  `crates/lazuli_cli/src/main.rs`** (grep returns zero hits). Schema
  diff/snapshot is an intent, not a feature.
- **`previously migrated`** parsing exists in pest and is harvested
  through `Resource.previous_names`/`Field.previous_names` at
  `crates/lazuli_ir/src/lib.rs:327,341`. The LSP has a shape rule
  (`previously_mode_diagnostics`,
  `crates/lazuli_lsp/src/lib.rs:1352-1404`) enforcing `migrated|alias`
  + indent discipline. Doctor has **no** `previously`-driven
  cross-check.
- The IR's `Resource` struct
  (`crates/lazuli_ir/src/lib.rs:304-330`) carries `tenancy`,
  `soft_delete`, `timestamps`, `fields`, `constraints`, `validate`,
  `validates`, `previous_names`, `span_ref` — and **nothing else**.
  No `indexes`, `foreign_keys`, `enum_columns`, `extensions`,
  `triggers`, `generated_columns`, `partitions`. Eight DL primitives,
  zero IR slots.

The Stage 1+2 inventory's recommendation is therefore: **do not
propose any of the eight decorators or the `tenant_migration` kind
until the resource lowering decision is made**. Adding the
decorators ahead of that pins them on a half-text-pattern surface
that doesn't survive Phase L Tier 4 without a redo. This proposal
resolves the lowering route first, then names the primitive subset
that justifies a design pass.

## Por que `resource` ainda é text-pattern

The canonical-indent slice (`crates/lazuli_syntax/src/parser.rs`)
was introduced in Cut A Phase 1 (commit `d2a6202`) scoped to
`feature` headers + indented `agent` blocks. Phase L Tier 1
(commit `e1d8521`) added `parse_auth`; Tier 2 (commit `f60f6bf`)
added `@cap.File(args)` typing; Tier 3 (commits `e89ff27` →
`299878e`) added `parse_job` / `parse_webhook` /
`parse_notification` / `parse_event_group`. **Tier 4 is
outstanding** (`docs/next-checklist.md:60`, row 24): no
`parse_resource`, `parse_command`, `parse_query`, `parse_record`.

The state of the slice for `resource` today:

1. `parse_feature_skeleton`
   (`crates/lazuli_syntax/src/parser.rs:1140`) recognises the
   `resource <Name>` header but only records the **name string**
   (`crates/lazuli_syntax/src/parser.rs:1649-1650`). The body is
   silently skipped to the next sibling header.
2. Every consumer downstream re-walks the source as text:
   - LSP: 5 different harvesters call
     `trimmed.strip_prefix("resource ")` to recover field shape
     (`crates/lazuli_lsp/src/lib.rs:5761,6139,6244,6354,10109`).
   - CLI inspect: same pattern at
     `crates/lazuli_cli/src/main.rs:1625,2460,2466,3325`.
3. `lower_field` (`crates/lazuli_analyzer/src/lib.rs:384`)
   processes individual `Field` AST nodes from the legacy pest
   pipeline. The pest grammar (`crates/lazuli_syntax/src/grammar.pest`)
   defines `aggregate { ... }` curly-brace syntax — not the
   canonical-indent `resource <Name>` shape the fixture authors.
   The current production path for resources reads source as
   **typed text-pattern walkers**, not a typed AST.

The criterion documented for promotion is implicit and consistent
with previous tiers: extend the slice when downstream consumers
need a typed cross-check the text-pattern walker cannot answer.
The migrations bucket trips that criterion three times over:

- **Field-level decorators** (`index`, `foreign_key`,
  `generated_column`, `enum_column`) need a typed field carrier so
  cross-feature doctor (e.g., `Customer.owner: User` ⇄ implicit
  FK on `users(id)`) can resolve target columns. Text-pattern can
  catch the keyword; it cannot resolve `User` to a feature symbol.
- **Resource-level decorators** (`extension`, `trigger`,
  `partition`) need a typed slot so doctor can cross-check
  `extension pg_trgm` against the registry-declared DB capability,
  and `partition by date_trunc('month', created_at)` against the
  `created_at` field's existence + `DateTime` type.
- **Schema drift detection** needs IR↔IR diff between the
  current source and a checkpoint. Without typed resource fields
  in IR, the diff input is a flat string list — same shape Rails'
  `db:rollback` reads, and same failure mode (false positives on
  whitespace, missed renames without `previously`).

No row in `docs/next-checklist.md` triggers Tier 4 today; Tier 4
itself is the trigger. This proposal documents the dependency
clearly: **the migrations bucket cycle is gated on Phase L Tier 4
covering `resource` and the surrounding constructs**. Without
Tier 4, the cycle ships a half-implementation that pins eight
decorators to a text-pattern foundation that has to be migrated
the moment Tier 4 lands.

## Routes A vs B vs C

Three ways to close the migrations lowering gap. All honour the
Lazuli/Drusa boundary: no provider names (`atlas`, `pg`,
`mysql`) in core syntax; execution mechanics (atlas planning, lock
acquisition, online column moves) stay in Drusa.

### Route A — full lowering in one run, after Tier 4 lands

Wait for Phase L Tier 4 (`parse_resource` +
`parse_resource_field` + lift `defaults.tenancy`). Then extend
`parse_resource` to recognise the eight new decorators and
extend the IR with eight new typed slots
(`Resource.indexes: Vec<IndexSpec>`,
`Resource.foreign_keys: Vec<ForeignKeySpec>`, etc.) plus add the
`TenantMigration` IR struct (sibling of `Job`/`Webhook`). Wire
through doctor cross-checks once the IR carries the typed shape.

**Cost (in cells, baseline = Tier 1+2 ≈ 2 cells each)**:

- Tier 4 prerequisite: ~5-6 cells (not chargeable to this cycle,
  but the gate is real — see `docs/roadmap.md:720`).
- IR extensions for the 8 decorators: ~1.5 cells.
- IR struct for `tenant_migration`: ~0.5 cell.
- Parser extensions in `parse_resource`: ~1 cell (8 child
  recognisers + position discipline).
- Doctor cross-checks (8 rules from §"Doctor/LSP propostos" below):
  ~1.5 cells.
- `lazuli plan` skeleton (read IR + previous_names → typed diff):
  ~1.5 cells.
- Inspect projection extensions: ~0.5 cell.

**Route A subtotal (excluding Tier 4)**: ~6.5 cells.

**Risk**: blocks indefinitely on Tier 4. The cycle cannot ship
until commands/resources are in the slice. The benefit is a clean
one-shot lift — every decorator lands typed, doctor drives off IR,
codegen has a stable input.

### Route B — text-pattern fact extraction (the `CommandApprovalFact` shape)

Add `collect_resource_decorator_facts` next to
`collect_command_approvals`
(`crates/lazuli_cli/src/doctor.rs:4046`), harvesting eight new
`ResourceDecoratorFact` families (one per decorator) plus
`TenantMigrationFact`. Doctor walks the source line-by-line,
captures the decorator + its arguments, and runs cross-checks
text-side. Inspect projects from facts. No IR change.

**Cost**: ~3 cells.

- One walker per decorator family (8 walkers, mechanical clones of
  existing `collect_*` shapes): ~1.5 cells.
- 8 doctor rules consuming the facts: ~1 cell.
- Inspect projection from facts: ~0.5 cell.

**Risk**: adds the **fourth** text-pattern fact family (after
`CommandApprovalFact`, `collect_feature_symbols`, the existing
`file_capabilities` facts, and `ExternalCallFact`). Phase L's
documented direction is to **shrink** text-pattern facts, not
grow them (`docs/proposals/auth-lowering-scope.md:133`). Every
new decorator the surface gains would extend the walker family
instead of the IR. Tier 4, when it lands, has to migrate all
eight rules — the same migration the proposal would otherwise
avoid.

### Route C — narrow Route A, ship a small subset that does not depend on Tier 4

Identify the strict subset of resource-level decorators whose
*semantics* are encodable today without `parse_resource` landing,
and ship only those. The subset:

- **`previously migrated` doctor cross-checks** — the IR field
  already exists (`Resource.previous_names`,
  `Field.previous_names`). Today no doctor rule consumes them.
  Adding cross-checks (forward references unresolved, rename
  cycles, doctor warning when same `previous_name` claims two
  current names) is a typed cross-check today, **without** Tier 4.
- **`tenant_migration` kind as a feature child** — analogous to
  `job` / `webhook` / `notification`, which Tier 3 already
  proved is liftable as a leaf grammar even without commands. A
  `tenant_migration` is a per-tenant, idempotent, ordered
  migration step (akin to `job` for schema work) with the same
  `idempotency by`, `retry`, `handler` spine. Liftable in a
  Tier 3.5 sibling cell.
- **Deploy-side migrations expansion** — `app.lzi`'s
  `deploy.migrations` already lifts. Add fields the operational
  contract needs today: `strategy <online|offline>`,
  `lock_timeout "<duration>"`, `pre_migration_hook "<path>"`,
  `post_migration_hook "<path>"`. These extend `AppDeploy` IR
  additively (mirrors the Tier 1 `Auth*` additions) without
  touching `Resource`.
- **Schema snapshot — declarative checkpoint pinning** — a
  `checkpoint <name> "<path>"` field on `deploy` that records a
  pinned IR JSON snapshot a `lazuli plan` would diff against.
  The IR is the snapshot format already; this just names a path.
  Doctor rule: snapshot exists; snapshot is newer than the
  authored fixture; snapshot has fields the current source
  removed (high-risk change without `previously`).

What stays deferred to Tier 4:

- All eight resource-field/resource-level decorators (`index`,
  `foreign_key`, `constraint` (typed), `enum_column`, `extension`,
  `trigger`, `generated_column`, `partition`). These need typed
  `Resource.fields` to attach to.
- `lazuli plan` typed diff covering field renames vs adds vs
  removes inside resources. Requires typed `Resource` to diff
  cleanly.

**Cost**: ~3 cells.

- `previously` doctor cross-checks (3 rules): ~0.5 cell.
- `parse_tenant_migration` + IR struct + 3 doctor rules: ~1 cell.
- `AppDeploy` IR extensions + doctor rules for the four new
  fields: ~0.5 cell.
- `checkpoint` field + doctor rules + `lazuli plan --check`
  command (snapshot-only, no field diff): ~1 cell.

**Risk**: the cycle delivers value today but cannot answer the
**core motivation** of the roadmap §1.6 entry (resource
decorators). Migrations runtime work in Drusa (§2.4) cannot
consume typed `index`/`foreign_key` IR because that IR doesn't
exist yet. Codegen for the migrations folder
(`dist/<feature>/migrations/*.up.sql`) stays blocked. Route C's
deliverables are real but narrow.

### Comparison

| Axis | Route A (full + Tier 4) | Route B (text-pattern) | Route C (narrow subset) |
|---|---|---|---|
| Upfront cost (cells) | ~6.5 + Tier 4 prerequisite | ~3 | ~3 |
| Maintenance cost | Lowest long-term — one canonical home for resource shape. | Highest — 8 new text-pattern fact families to migrate at Tier 4. | Low — every Route C deliverable lands typed; Tier 4 follow-up extends `parse_resource`, not retrofits. |
| Fixture coverage | 100% of authored migrations surface + adds 8 new decorators. | Same as A, but text-pattern. | `previously` + `tenant_migration` + deploy expansion + checkpoint snapshot. **No resource-field decorators.** |
| Codegen unblock | Yes — `dist/<feature>/migrations/*.sql` consumes typed IR. | Codegen must re-parse source. | No — resource-field codegen still blocked until Tier 4. |
| Phase L compat | Aligned — shrinks the slice's skip-list. | Misaligned — grows text-pattern surface. | Aligned — every Route C deliverable lands as typed IR. |
| Risk of redesign | Low — Tier 4 settled before this cycle lands. | High — eight walkers retrofit at Tier 4. | None — Route C deliverables are orthogonal to resource lowering. |
| Drusa runtime input | Typed `IndexSpec`, `ForeignKeySpec`, etc. ready for atlas/golang-migrate codegen. | Untyped — same as today. | `previously` + `tenant_migration` + `checkpoint` + deploy expansion ready for atlas wiring. Resource-field decorators stay blocked. |
| Time to first ship | Long — gates on Tier 4. | Medium — ships but doesn't unblock §1.6 properly. | Short — ships without Tier 4 dependency. |

### Recomendação

**Route C now + Route A as the Tier 4 follow-up cycle.** Three
reasons:

1. **The roadmap §1.6 surface is bigger than a single cycle.** The
   eight resource decorators × cross-feature semantics × Drusa
   atlas wiring is a multi-cycle effort. Forcing all of it
   through a single cycle (Route A) pins the cycle on a
   prerequisite (Tier 4) that is itself a multi-cycle effort
   (`parse_command` / `parse_resource` / `parse_query` /
   `parse_record` + `JobDeclarative.raw_*` carve-out retire). Route
   C separates the language work into two natural cycles aligned
   with Phase L's tier rhythm.

2. **The Route C subset is the highest-value, lowest-risk
   subset.** `previously` cross-checks are doctor sugar over IR
   that already exists; the value is immediate (no more silent
   renames). `tenant_migration` is a leaf grammar Tier 3 already
   proved is liftable without commands. Deploy expansion +
   checkpoint pinning are additive IR changes to `AppDeploy`
   that don't touch the resource surface at all. Route C ships
   real migration semantics today.

3. **Route B violates the documented direction.** Phase L's
   stated goal (`docs/proposals/auth-lowering-scope.md:133-134`)
   is to delete text-pattern fact families, not grow them. Eight
   new families would be the biggest single Phase L regression
   shipped this year. The cost saving (~3 cells vs ~6.5 cells)
   does not justify the strategic debt.

Route A is rejected **for this cycle**, not in principle. It is
the correct shape once Tier 4 closes. Document it as the
follow-up cycle in `docs/roadmap.md` Notas de execução.

## PILOT-NEEDED vs SPECULATIVE

Classification of every migration construct against fixture
evidence and roadmap §1.6 (`docs/roadmap.md:111-114`).

### PILOT-NEEDED — exercised by the canonical fixture today

| Construct | Fixture evidence | Tier-3.5 fate |
|---|---|---|
| `Resource.previous_names` doctor cross-check | `full-capsule.lzi:46` (`previously migrated Account`), `:51` (`previously migrated status`) | **Lift now.** IR fields exist; no doctor consumes them today. Three additive rules: forward-reference resolves, rename cycle detect, two-current-claim conflict. |
| `Field.previous_names` doctor cross-check | `full-capsule.lzi:51` | **Lift now.** Same shape as resource-level. |
| `deploy.migrations` policy (incl. `migration_lock`, `destructive_migrations`, `rollback`) | `app.lzi:91-95`, `profiles.lzi:16,26` | **Already lifted** (`AppDeploy.migrations`, `migration_lock`, `destructive_migrations`, `rollback` at `crates/lazuli_ir/src/lib.rs:1463-1467`). This cycle adds expansion fields (`strategy`, `lock_timeout`, `pre_migration_hook`, `post_migration_hook`) plus typed catalogs for the existing string fields. |
| `checkpoint <name> "<path>"` snapshot pin in `deploy` | Not authored, but referenced as intent in `docs/migrations.md:34-50` (planner output assumes pinned baseline). | **Lift now as fixture extension.** The `migrations.md` planner needs a checkpoint format; pinning the IR JSON snapshot is the natural shape. Doctor cross-check: snapshot file exists, snapshot's IR version matches the analyzer's expectation. |
| `tenant_migration <name>` kind | Not in fixture today, but **strongly implied** by `tenancy org` resources (`full-capsule.lzi:46`, every `resource Customer*`) — each tenant needs idempotent schema migration when fan-out runs. | **Lift now as fixture extension.** Mirrors `job` (per-tenant, idempotent, ordered) with a closed body: `target tenants <axis>`, `idempotency by`, `retry`, `handler`. Tier 3's `parse_job` is the structural template. |

### SPECULATIVE — not in the fixture; defer to the Route A Tier-4 follow-up cycle

| Construct | Status | Why defer |
|---|---|---|
| `index <fields> [unique] [where ...]` decorator on resource | Not authored. Roadmap §1.6 lists it. | Pilot needed: a query whose plan requires a covering index that the implicit unique index on `id` does not satisfy. Until that authoring pressure surfaces, `unique` constraints + `query.list` paginate-50 defaults cover the canonical case. The decorator also needs typed `Resource.fields` (Tier 4) to attach to. |
| `foreign_key <field> references <Resource>.<field>` decorator | Not authored. Roadmap §1.6 lists it. | Pilot needed: a resource whose FK target is *not* the implicit `id` of the named resource — e.g., `customer_email: Text references Customer.email`. Today every `field: Customer required` implicitly declares `references Customer.id`; the explicit decorator adds value only for non-identity targets. Needs Tier 4. |
| `constraint check "<sql>"` typed decorator | Not authored. `constraints` block today is uniqueness-only (`full-capsule.lzi:69-71`). Roadmap §1.6 lists `constraint` separately. | Pilot needed: a resource whose invariant cannot be expressed as `unique` + `validates` (the existing two surfaces). Examples (e.g., `check (start_date < end_date)`) are tempting but every existing `validates` example covers them. Needs Tier 4 for typed expression embedding (otherwise it's a `raw_*` carve-out like `JobDeclarative`). |
| `enum_column` decorator | Not authored. Enum columns today are declared via `lifecycle_stage: CustomerStatus = lead` where `enum CustomerStatus` is a sibling block. Roadmap §1.6 lists the decorator. | Pilot needed: a product that needs database-level enum types (PostgreSQL `CREATE TYPE`) rather than text-columns-with-app-side-validation. Today's `enum` block + `Text` column covers the canonical case. |
| `extension <name>` decorator on feature | Not authored. Roadmap §1.6 lists it (PostgreSQL extensions: `pg_trgm`, `uuid-ossp`, `vector`). | Pilot needed: a feature whose authoring needs to declare extension dependencies the runtime cannot infer (vector embeddings, full-text search beyond stdlib tsvector). Until a fixture authors an `agent` with embedding tools requiring `pg_vector`, the decorator has no surface pressure. |
| `trigger <name> on <event>` decorator | Not authored. Roadmap §1.6 lists it. | Pilot needed: a resource whose state machine cannot be expressed as `event` + `job trigger event`. Today's fixture authors every state transition through commands and event-triggered jobs; database triggers add bypassable side effects (changes outside the app are not audited). Cut by design unless pilot evidence overrides. |
| `generated_column <name> as <expression>` decorator | Not authored. Roadmap §1.6 lists it. | Pilot needed: a column whose value is a function of other columns and *must* be database-computed (read-side queries depend on the value). Today's `derived from <expression>` (`full-capsule.lzi:56`) covers app-side computation; the typed decorator would only differ if Lazuli wants the value persisted. Cut until pilot. |
| `partition by <expression>` decorator | Not authored. Roadmap §1.6 lists it. | Pilot needed: a resource whose row count justifies physical partitioning (event_store table, audit_log table at high cardinality). Today's fixture has no such resource. |
| Schema drift detection doctor rule | `docs/migrations.md:1-60` sketches the planner intent. | **PARTIALLY PILOT-NEEDED.** The `checkpoint` half is pilot-needed (covered above); the typed field-diff half needs Tier 4 to land. Route C ships the snapshot half today; Route A follow-up adds the typed diff. |
| `lazuli plan` command | `docs/migrations.md:34-50` describes the intent. The CLI binary does not implement it. | **PARTIALLY PILOT-NEEDED.** Route C ships `lazuli plan --check <snapshot>` (snapshot integrity only). Typed field-level diff (`Rename Customer.status -> Customer.lifecycle_status`) waits for Tier 4. |
| Irreversible migration markers (F-class per audit) | Not in fixture. Audit §8 lists under F. | Pilot needed: a destructive migration whose runbook needs an explicit "no rollback" gate. The existing `destructive_migrations require_approval` covers the approval surface; an irreversibility marker adds rollback-impossible declaration on top. Cut B-ish (post-pilot). |
| Schema snapshot kind beyond pinned-path (F-class per audit) | Not in fixture. Audit §8 lists under F. | Pilot needed: a product whose snapshot needs structured metadata beyond a path (compatibility window, deprecation date, replacement feature). The pinned-path version Route C ships covers the canonical case. |
| Online migrations / zero-downtime helpers | DF-class per audit. | Drusa concern. Not a language primitive at all — atlas/golang-migrate execution plan stays in runtime. |
| Migration locking / status / rollback / redo / squashing | DF-class per audit. | Drusa concern. Same as above. |
| Database create / drop / reset / truncate commands | DF-class per audit. | Drusa CLI concern (`lazuli db ...`). Not a language primitive. |
| Seed loader | DF-class per audit. | Drusa concern. Optional sugar: `seeds "./seeds/customer.sql"` on feature, but that's a Tier 4 follow-up if pilot evidence warrants. |
| Separate "Go migrations" file format | N-class per audit. | Anti-Lazuli: violates "Lazuli re-lowers from source" principle. Permanent cut. |

Net result for Route C: **5 PILOT-NEEDED items** land in the
cycle. **12 SPECULATIVE items** defer to the Tier-4 follow-up
cycle. **3 N-class items** stay cut.

## Closed-cycle criterion para Tier 3.5

Adapted from `docs/proposals/phase-l-tier-3-job-effect-scope.md:441-505`
to the migrations subset.

- [ ] **Fixture authors the full surface.** Route C extends the
  canonical fixture with:
  - One `tenant_migration` kind on the `customer` feature (e.g.,
    `tenant_migration backfill_customer_score` with `target tenants org`,
    `idempotency by`, `handler`).
  - `checkpoint baseline "./tests/fixtures/full-capsule.snapshot.json"`
    in `app.lzi`'s `deploy` block.
  - Deploy expansion fields: `strategy online`,
    `lock_timeout "30s"`, `pre_migration_hook "./hooks/pre.sh"`,
    `post_migration_hook "./hooks/post.sh"`.
  - `previously` already exists at `full-capsule.lzi:46,51`; no
    fixture change for the `previously` doctor rules.
- [ ] **`lazuli check` accepts the syntax.** The legacy text-pattern
  path accepts existing constructs; the new `tenant_migration` +
  `checkpoint` lands through `parse_feature_skeleton` and
  `parse_app_manifest` respectively.
- [ ] **`lazuli inspect --expand=migrations` projects the IR.**
  New projection. Surfaces per-feature `tenant_migrations: Vec<TenantMigration>`
  plus app-level `deploy.checkpoint` + extended `deploy.migrations`
  fields. Mirrors `--expand=jobs` shape.
- [ ] **`lazuli doctor` carries ≥6 cross-feature diagnostics for
  migrations**:
  - `previously_forward_reference_unresolved` —
    `Resource.previous_names`/`Field.previous_names` references a
    name that exists nowhere in the package (typo or stale rename).
  - `previously_rename_cycle` — `A previously B`, `B previously A`
    cycle detection.
  - `previously_duplicate_claim` — two current names claim the
    same `previously` source.
  - `tenant_migration_target_axis_unknown` — `target tenants <axis>`
    references an axis not declared in any `defaults.tenancy`
    in the same feature.
  - `tenant_migration_idempotency_required` —
    `tenant_migration` without `idempotency by` (mandatory; schema
    migrations are not safely re-runnable without an idempotency
    key).
  - `deploy_checkpoint_stale` — checkpoint file exists but its
    IR version is older than the analyzer's current expectation.
  - `deploy_checkpoint_path_invalid` — checkpoint path does not
    resolve to a file relative to `app.lzi`.
  - `deploy_strategy_invalid` — `strategy` is not in the closed
    catalog `{online, offline}`.
- [ ] **`lazuli generate` produces Go that compiles.** Runtime-team
  deliverable (Drusa). Consumed via stable IR JSON through
  `lazuli inspect --format=json --expand=migrations`. Tier 3.5
  only needs the IR ready; runtime owns atlas/golang-migrate
  invocation.
- [ ] **Drusa executes a tenant migration end-to-end.** Runtime-team
  deliverable. Outside language scope. The test case:
  `tenant_migration backfill_customer_score` runs once per
  tenant, recovers from interrupt mid-tenant, satisfies the
  `migration_lock` from `app.lzi`.
- [ ] **`eval`/test coverage.** Doctor fixture exercising each of
  the 8 diagnostics. A `tenant_migration` round-trip eval is
  optional (no LLM in the loop).
- [ ] **LSP hover/completion on every new construct.** Hover on
  `tenant_migration <name>` mirrors `job`. Hover on
  `strategy online|offline` shows the closed catalog. Hover on
  `checkpoint <name> "<path>"` shows the snapshot format
  expectation. Completion on `previously` already exists; new
  completion on `tenant_migration` + `checkpoint` reserved words.
- [ ] **`lazuli plan --check <snapshot>`** validates checkpoint
  integrity (path exists, IR-version matches). Typed field diff
  (`Rename Customer.status -> Customer.lifecycle_status`) is
  **out of scope** for Tier 3.5; lands in the Tier-4 follow-up.

The first four items are language-team Tier 3.5 deliverables. Items
5-6 are Drusa-team. Items 7-9 are language-team but small.

## Recomendação

1. **Take Route C** (narrow subset). Estimated scope: ~3 cells
   (`previously` doctor rules ~0.5 + `parse_tenant_migration` +
   IR struct + 3 doctor rules ~1 + `AppDeploy` expansion ~0.5 +
   `checkpoint` + `lazuli plan --check` ~1).
2. **Land the IR extensions first, then parsers, then doctor** —
   the same ordering Tier 1's erratum
   (`docs/proposals/auth-lowering-scope.md:21-23`) settled.
3. **Defer 12 SPECULATIVE items to the Tier-4 follow-up cycle.**
   Document Route A in `docs/roadmap.md` "Notas de execução" as
   the follow-up: "Phase L Tier 4 + migrations decorators on
   resources (Route A)". The Tier-4 follow-up cycle is naturally
   gated on Tier 4 itself; nothing in Route C blocks it.
4. **Update `docs/next-checklist.md` row 24** (Phase L) only
   after Route C lands, to reflect that `tenant_migration` joins
   the slice's coverage. Row 24's Tier 4 description stays as
   written (commands/resources/queries/records); migrations
   decorators on resources become a sibling row 38+ added once
   Tier 4 + Route A ship.
5. **Cross-link `docs/migrations.md` to this proposal.** The
   `lazuli plan` doc currently describes the planner output as
   if the typed diff existed; after Route C lands, it should
   document `lazuli plan --check <snapshot>` as the available
   surface and reference the Tier-4 follow-up for typed field
   diff.

When Route C is implemented, the Tier-4 follow-up cycle runs on
the shipped substrate and adds the eight resource decorators +
typed field diff. Drusa codegen then has typed input for atlas
plans + migration locking + online column moves.

## Side-quest fence (anti-mission-creep)

This cycle does **not**:

- Introduce a new top-level kind (`migration <name>` as a
  standalone file format). Lazuli re-lowers schema from
  `resource` source. The audit's N-class call
  (`docs/audit/framework-coverage-1400.md:187`) is final.
- Touch `crates/lazuli_codegen_go`. Codegen for migrations folder
  (`dist/<feature>/migrations/*.sql`) is the Tier-4 follow-up.
- Wire `atlas` or `golang-migrate` adapters. Runtime team owns
  this when codegen is ready.
- Add online-migration helpers, zero-downtime column moves,
  blue-green migration sequencing, or any DF-class item from
  audit §8. All Drusa.
- Promote `unique` to an `index` decorator. The two are
  conceptually different (uniqueness is an invariant; index is a
  query optimisation). Promoting `unique` to `index unique`
  hides the invariant intent. Cut.
- Pre-declare partition strategies or generated columns. Both
  need typed `Resource.fields` (Tier 4) to attach to.

When in doubt: if the construct needs `parse_resource` to lower
cleanly, it belongs to the Tier-4 follow-up cycle, not Route C.
