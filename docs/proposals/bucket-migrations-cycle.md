# Bucket Cycle: Migrations (L0→L2)

**Status**: design proposal. Stages 3–9 of the `bucket=migrations`
pipeline. Implementation deferred to a separate run with
`mode=implement`. **Gated on Route C** as decided in
`docs/proposals/bucket-migrations-scope.md` — Route A (the full
typed resource-decorator lift) waits for Phase L Tier 4.

**Audience**: language team (Lazuli core), runtime team (Drusa).

**Date**: 2026-05-11.

## Contexto

Migrations is pilot bucket §1.6 of the roadmap
(`docs/roadmap.md:111-114`). The canonical fixture authors two
classes of migration-adjacent surface today:

- **Field-level identity hints** at
  `examples/full-capsule/full-capsule.lzi:46`
  (`previously migrated Account`) and `:51`
  (`previously migrated status` on `lifecycle_stage: CustomerStatus = lead`).
  The IR carries them (`Resource.previous_names`,
  `Field.previous_names` at `crates/lazuli_ir/src/lib.rs:327,341`)
  but no doctor rule consumes them.
- **Deploy-side migration policy** at
  `examples/full-capsule/app.lzi:91-95` and
  `examples/full-capsule/profiles.lzi:16,26`. Already lifted to
  `AppDeploy.migrations` / `migration_lock` /
  `destructive_migrations` / `rollback`
  (`crates/lazuli_ir/src/lib.rs:1463-1467`). Inspect projects;
  doctor does not cross-check the field values.

What is missing is **everything else from §1.6**:

- The eight resource-level decorators (`index`, `foreign_key`,
  `constraint` typed, `enum_column`, `extension`, `trigger`,
  `generated_column`, `partition`) — **none authored** anywhere
  in the fixture.
- The `tenant_migration` kind — **not authored**.
- Schema drift detection doctor rule — **does not exist**.
- `lazuli plan` CLI command — **does not exist**
  (`docs/migrations.md:34-50` describes the intent; `crates/lazuli_cli/src/main.rs`
  has zero matches for `plan`).

The scope-out (`docs/proposals/bucket-migrations-scope.md`)
documents the **hard blocker**: `resource` is text-pattern
everywhere (LSP, CLI inspect, doctor, even `parse_feature_skeleton`
which records only the resource name string). Five text-pattern
harvesters in LSP (`crates/lazuli_lsp/src/lib.rs:5761,6139,6244,6354,10109`)
and four in CLI inspect (`crates/lazuli_cli/src/main.rs:1625,2460,2466,3325`)
re-walk source per call. Phase L Tier 4 (`docs/next-checklist.md:60`,
row 24) is the documented home for `parse_resource`; until Tier 4
lands, the eight decorators cannot attach to a typed
`Resource.fields` cleanly.

The route decision (canonical input for this run): **Route C** —
ship the high-value subset that does **not** depend on Tier 4
(`previously` cross-checks, `tenant_migration` as a feature
child, deploy-block expansion, declarative checkpoint pinning,
`lazuli plan --check` snapshot integrity). Defer the eight
resource decorators + typed field diff to a Tier-4 follow-up
cycle (Route A). The closed-cycle criterion (≥6 doctor
diagnostics, `--expand=migrations` projection, LSP hover/completion
on every new construct, snapshot-integrity command) is the
acceptance gate for the Route C cycle.

## Baseline (Stages 1-2 inventory)

| Layer | Status | Anchor |
|---|---|---|
| `previously migrated <old>` (resource header) | lifted; IR slot exists, no doctor consumes | `crates/lazuli_ir/src/lib.rs:327`, fixture `:46` |
| `previously migrated <old>` (field) | lifted; IR slot exists, no doctor consumes | `crates/lazuli_ir/src/lib.rs:341`, fixture `:51` |
| `previously` shape rule (LSP) | enforces `migrated|alias` keyword + indent discipline | `crates/lazuli_lsp/src/lib.rs:1352-1404` |
| `deploy.migrations` policy fields | lifted; inspect projects | `crates/lazuli_ir/src/lib.rs:1463-1467`, fixture `app.lzi:91-95` |
| Resource decorators (`index`, `foreign_key`, `constraint` typed, `enum_column`, `extension`, `trigger`, `generated_column`, `partition`) | **none authored, zero IR slots, zero doctor** | roadmap §1.6 lists |
| `tenant_migration` kind | **not authored, no IR struct** | roadmap §1.6 lists |
| `lazuli plan` command | **not implemented** | `docs/migrations.md:34-50` describes intent |
| Schema drift detection rule | **not implemented** | roadmap §1.6 + audit §8 list |
| Resource parser | text-pattern only (5 LSP walkers + 4 inspect walkers) | `crates/lazuli_lsp/src/lib.rs:5761,6139,6244,6354,10109`; `crates/lazuli_cli/src/main.rs:1625,2460,2466,3325` |
| `Resource` IR struct | `name`, `tenancy`, `soft_delete`, `timestamps`, `fields`, `constraints`, `validate`, `validates`, `previous_names`, `span_ref` only | `crates/lazuli_ir/src/lib.rs:304-330` |
| Codegen for migrations | **zero** — no `dist/<feature>/migrations/*.sql` produced | confirmed via `crates/lazuli_codegen_go` grep |
| Drusa runtime | **zero** — no `runtime/go/lazuli/migrations/` package | confirmed via ls |
| Adapter slots | `atlas` and `golang-migrate` declared as roadmap §3.1 targets, neither wired | `docs/roadmap.md:594` |

**Cross-cutting fact**: every `tenancy org` resource in the fixture
(`Customer`, `CustomerSession`, `CustomerImportBatch`, etc.)
implies that schema migration must run **per tenant** in the
fanout deploy model — exactly the contract `tenant_migration`
encodes. Today the deploy block's `migrations before_deploy`
runs once globally; nothing in IR or doctor expresses the per-tenant
shape. This gap is what the Route C `tenant_migration` kind plus
the `tenant_migration_target_axis_unknown` doctor rule close.

## Linguagem (Stage 3)

Surface for Route C is **two new constructs** (`tenant_migration`
kind, `checkpoint` field on deploy) plus **expansion fields** on
the existing `deploy.migrations` block.

### Formal grammar (EBNF, draft for `docs/grammar.lzi.md`)

```ebnf
tenant_migration_block = "tenant_migration" ident NEWLINE
                         INDENT tenant_migration_child+ DEDENT ;

tenant_migration_child = target_tenants_line
                       | idempotency_line     (* required *)
                       | retry_line           (* optional *)
                       | timeout_line         (* optional *)
                       | handler_line ;       (* required *)

target_tenants_line    = "target" "tenants" ident NEWLINE ;
```

```ebnf
checkpoint_field       = "checkpoint" ident string_literal NEWLINE ;

(* extends existing deploy block grammar *)
deploy_child           +=
    "strategy" ("online" | "offline") NEWLINE
  | "lock_timeout" string_literal NEWLINE
  | "pre_migration_hook" string_literal NEWLINE
  | "post_migration_hook" string_literal NEWLINE
  | checkpoint_field ;
```

### Canonical authoring sample (Route C fixture extension)

```lazuli
feature customer
  domain
    resource Customer
      previously migrated Account
      # ... fields ...
      lifecycle_stage: CustomerStatus = lead
        previously migrated status
      # ... rest ...

  tenant_migration backfill_customer_score
    target tenants org
    idempotency by tenant_id
    retry 3 backoff exponential
    timeout "5m"
    handler "./migrations/backfill_customer_score.go"
```

```lazuli
# app.lzi
app FullCapsule
  deploy
    migrations before_deploy
    strategy online
    lock_timeout "30s"
    pre_migration_hook "./hooks/migration_pre.sh"
    post_migration_hook "./hooks/migration_post.sh"
    migration_lock required
    destructive_migrations require_approval
    rollback on_failed_healthcheck
    checkpoint baseline "./tests/fixtures/full-capsule.snapshot.json"
```

### Closed catalogs

- `deploy.strategy ∈ {online, offline}`. Closed; doctor rejects
  unknown values.
- `deploy.migrations ∈ {before_deploy, after_deploy, skip}` — already
  closed (existing IR field).
- `deploy.migration_lock ∈ {required, optional, none}` — already
  closed.
- `deploy.destructive_migrations ∈ {require_approval, allow, block}` —
  already closed.
- `tenant_migration` body is closed to 5 children (`target tenants
  <axis>`, `idempotency by`, `retry N backoff <strategy>`,
  `timeout "<duration>"`, `handler "<path>"`). No `emits`, no
  `target query.`. Schema migrations are by-design free of business
  effects.

### Side-quest fences (not in Route C scope)

- No `index <fields>` / `foreign_key` / `constraint check ...` /
  `enum_column` / `extension` / `trigger` / `generated_column` /
  `partition` decorators. These need typed `Resource.fields`
  (Tier 4) to attach to.
- No `migration <name>` standalone file format. Lazuli re-lowers
  from source; audit §8 N-class is final.
- No `seed_loader` kind. Drusa concern.
- No typed field diff inside `lazuli plan`. Tier-4 follow-up.

## IR (Stage 4)

Three additive shapes plus one new struct.

### `TenantMigration` (new struct)

```rust
pub struct TenantMigration {
    pub name: String,
    pub target: TenantMigrationTarget,
    pub idempotency: IdempotencyKey,             // mandatory
    pub retry: Option<RetryPolicy>,
    pub timeout: Option<String>,
    pub handler: PathRef,
    pub previous_names: Vec<String>,
    pub span_ref: Option<SpanRef>,
}

pub struct TenantMigrationTarget {
    pub axis: String,  // matches `defaults.tenancy <axis>` in feature
}
```

Lives on `Feature.tenant_migrations: Vec<TenantMigration>`
(additive field on `Feature`). Mirrors `jobs: Vec<Job>` shape
exactly. Serde-default-empty so existing fixtures keep parsing.

### `AppDeploy` extensions (additive)

```rust
pub struct AppDeploy {
    // existing
    pub migrations: Option<String>,
    pub migration_lock: Option<String>,
    pub destructive_migrations: Option<String>,
    pub rollback: Option<String>,

    // new (Route C)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_migration_hook: Option<PathRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_migration_hook: Option<PathRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<DeployCheckpoint>,
}

pub struct DeployCheckpoint {
    pub name: String,
    pub path: PathRef,
}
```

All additive on the existing struct. `serde(default,
skip_serializing_if)` per field; no breaking change to inspect JSON
consumers.

### `Resource.previous_names` / `Field.previous_names` — no IR change

The IR slots already exist. Route C consumes them via doctor
rules; no struct change.

## Parser (Stage 5)

Route C lives **outside** `parse_resource` (which doesn't exist
yet). Two surfaces to lift:

### `parse_tenant_migration` (new, in `parse_feature_skeleton`)

Mirrors `parse_job`. Sits next to `parse_job` /
`parse_webhook` / `parse_notification` /
`parse_event_group` in `crates/lazuli_syntax/src/parser.rs`. The
slice already recognises 4-space + `tenant_migration <name>` as a
feature child sibling header; recogniser fans into 5 child
parsers (`parse_tm_target`, `parse_tm_idempotency`,
`parse_tm_retry`, `parse_tm_timeout`, `parse_tm_handler`). Each
child has a closed shape; the recogniser pattern matches
`parse_auth_password` exactly.

### `parse_app_manifest` extension (existing function)

Add recognition for `strategy`, `lock_timeout`,
`pre_migration_hook`, `post_migration_hook`, `checkpoint <name>
"<path>"` under the existing `deploy` block. Each is an additive
child line; existing `migrations` / `migration_lock` /
`destructive_migrations` / `rollback` recognisers stay untouched.

### Estimated parser cost

~1 cell for `parse_tenant_migration` (5 child recognisers, all
leaf-grammar). ~0.5 cell for `deploy` block extension. No new
sub-IR carve-out (unlike `JobDeclarative.raw_*`).

## Doctor (Stage 6)

Eight new IR-driven cross-checks. All consume typed IR; none
relies on text-pattern walking.

| Code | Rule | Reads | Severity |
|---|---|---|---|
| `PREVIOUSLY-FWD-001` | `Resource.previous_names` / `Field.previous_names` references a name that exists nowhere in the package (typo, stale rename). | All `Resource.previous_names` + `Field.previous_names` across the package. | Warning. |
| `PREVIOUSLY-CYCLE-001` | Rename cycle: `A previously B`, `B previously A`. | Same source as above. | Error. |
| `PREVIOUSLY-DUP-001` | Two current names claim the same `previously` source. | Same source as above. | Error. |
| `TM-AXIS-001` | `tenant_migration` `target tenants <axis>` references an axis not declared in any `defaults.tenancy` in the same feature. | `Feature.tenant_migrations` + `Feature.defaults.tenancy`. | Error. |
| `TM-IDEMP-001` | `tenant_migration` lacks `idempotency by` (mandatory; schema migrations are not safely re-runnable without an idempotency key). | `TenantMigration.idempotency`. | Error. |
| `DEPLOY-CHECKPOINT-001` | `checkpoint` path does not resolve to a file relative to `app.lzi`. | `AppDeploy.checkpoint.path`. | Error. |
| `DEPLOY-CHECKPOINT-002` | `checkpoint` file exists but its IR version is older than the analyzer's expected version (snapshot is stale; user should regenerate). | `AppDeploy.checkpoint` + loaded snapshot file. | Warning. |
| `DEPLOY-STRATEGY-001` | `strategy` not in closed catalog `{online, offline}`. | `AppDeploy.strategy`. | Error. |

Cross-check `PREVIOUSLY-FWD-001` is the highest-value new rule:
it catches the canonical author error where a renamed field's
`previously migrated <old>` references a name that was itself
already renamed away — a silent-data-loss footgun in the planner
output that `docs/migrations.md` describes.

Doctor file: extend `crates/lazuli_cli/src/doctor.rs` with three
new fact families (`PreviouslyFact`, `TenantMigrationFact`,
`DeployCheckpointFact`) that **read from typed IR** (not text)
because the source slots already exist. The fact-family name is
preserved for consistency with existing doctor architecture; the
underlying read is IR-driven.

## LSP (Stage 7)

Five new hover targets + closed-catalog completions on three
keywords.

| Token | Hover content | File |
|---|---|---|
| `tenant_migration` (keyword) | Brief explanation: "Per-tenant idempotent schema migration. Closed body: `target tenants`, `idempotency by`, `retry`, `timeout`, `handler`." | `crates/lazuli_lsp/src/lib.rs` |
| `target tenants <axis>` | "Fan-out axis. Must match a `defaults.tenancy <axis>` declared in the same feature." | same |
| `strategy <mode>` (under `deploy`) | "Migration execution strategy. `online` = zero-downtime (atlas-driven column splits, dual-writes); `offline` = downtime window required." | same |
| `lock_timeout "<duration>"` | "Max time to wait for the migration advisory lock before aborting. Parsed as a duration literal." | same |
| `checkpoint <name> "<path>"` | "Pinned IR snapshot path. `lazuli plan --check <name>` validates the snapshot's integrity. Schema drift is detected by diffing the snapshot against the current source IR (Tier-4 follow-up)." | same |

Completions:

- After `strategy ` → `online`, `offline`.
- After `tenant_migration ` → no completion (free identifier).
- After `target tenants ` → all axis names declared in the same
  feature's `defaults.tenancy` (closed dynamic set).

No new lint walkers; the LSP shape rule
`previously_mode_diagnostics` (already at
`crates/lazuli_lsp/src/lib.rs:1352-1404`) covers the indent
contract for `previously` and stays as-is.

## Inspect (Stage 8)

New projection: `lazuli inspect --expand=migrations`.

Output (per feature):

```json
{
  "feature": "customer",
  "tenant_migrations": [
    {
      "name": "backfill_customer_score",
      "target": { "axis": "org" },
      "idempotency": { "by": "tenant_id" },
      "retry": { "count": 3, "backoff": { "kind": "exponential" } },
      "timeout": "5m",
      "handler": "./migrations/backfill_customer_score.go"
    }
  ]
}
```

Output (per app, on `app.lzi` inspection):

```json
{
  "deploy": {
    "migrations": "before_deploy",
    "strategy": "online",
    "lock_timeout": "30s",
    "pre_migration_hook": "./hooks/migration_pre.sh",
    "post_migration_hook": "./hooks/migration_post.sh",
    "migration_lock": "required",
    "destructive_migrations": "require_approval",
    "rollback": "on_failed_healthcheck",
    "checkpoint": {
      "name": "baseline",
      "path": "./tests/fixtures/full-capsule.snapshot.json"
    }
  }
}
```

`previously_names` projection on Resource/Field is **already**
exposed in `lazuli inspect --format=json`; no new key needed.

## CLI (Stage 8.5)

New command: `lazuli plan --check <snapshot_name>`.

Semantics:

1. Read `AppDeploy.checkpoint.path` for the named snapshot.
2. Load the IR JSON at that path.
3. Verify the snapshot's `lazuli_version` field matches the
   current analyzer's expected version.
4. Verify the snapshot is parseable as `Package` IR.
5. If all 3 pass: print "checkpoint <name>: ok". Exit 0.
6. If any fail: print the specific error. Exit non-zero.

**Out of scope for Route C**: typed field-level diff
(`Rename Customer.status -> Customer.lifecycle_status`, the planner
output `docs/migrations.md:34-50` describes). That requires
`parse_resource` to produce typed `Resource.fields` so the diff
algorithm has structured input. Tier-4 follow-up.

The naming choice (`lazuli plan --check`) preserves room for the
Tier-4 follow-up to add `lazuli plan diff` and `lazuli plan apply`
without renaming.

## Codegen + Runtime (Stages 9-10)

**Out of language scope**. Drusa team owns:

- `dist/<feature>/migrations/tenant_migration_*.gen.go` — a typed
  `TenantMigrationContract` struct mirroring IR, a `Run(ctx,
  tenantID)` entry point, retry loop honouring the declared
  `RetryPolicy`, idempotency key lookup against a
  `tenant_migration_log` table per tenant database/schema.
- `runtime/go/lazuli/migrations/` — package with `TenantMigrator`
  interface, `Mount(*chi.Mux)` for status endpoints, atlas /
  golang-migrate adapter bindings.
- Adapter wiring: `@runtime/atlas` for plan/apply; `@runtime/golang-migrate`
  as the secondary alternative.

Language deliverable for this cycle: **the IR JSON is stable**.
Codegen reads it; runtime instantiates against it.

## Evals + Tests (Stage 11)

Doctor fixtures (in `crates/lazuli_cli/tests/fixtures/migrations/`):

- `previously_forward_unresolved.lzi` → triggers `PREVIOUSLY-FWD-001`.
- `previously_cycle.lzi` → triggers `PREVIOUSLY-CYCLE-001`.
- `previously_duplicate_claim.lzi` → triggers `PREVIOUSLY-DUP-001`.
- `tenant_migration_axis_unknown.lzi` → triggers `TM-AXIS-001`.
- `tenant_migration_no_idempotency.lzi` → triggers `TM-IDEMP-001`.
- `deploy_checkpoint_path_invalid.lzi` → triggers `DEPLOY-CHECKPOINT-001`.
- `deploy_checkpoint_stale.lzi` → triggers `DEPLOY-CHECKPOINT-002`.
- `deploy_strategy_invalid.lzi` → triggers `DEPLOY-STRATEGY-001`.

Inspect golden: pin
`tests/fixtures/full-capsule-migrations.golden.json` as the
authoritative shape of `--expand=migrations`. Mirror the auth /
storage Tier 1+2 pattern.

No golden eval needed (no LLM in the loop).

## Anti-mission-creep fences

This cycle deliberately does not:

- Touch the `parse_resource` Tier 4 work. Mentioned only as the
  precondition for the follow-up cycle.
- Introduce any of the eight resource decorators
  (`index`, `foreign_key`, `constraint` typed, `enum_column`,
  `extension`, `trigger`, `generated_column`, `partition`).
- Introduce online-migration helpers, blue-green column moves, or
  zero-downtime sequencing. All DF-class per audit §8.
- Wire `atlas` or `golang-migrate` adapters. Runtime team.
- Promote `unique` constraints to `index unique` decorators.
- Add seed loader, db create/drop/reset CLI. Drusa CLI work.

If a question arises during implementation that touches any of
the above, the answer is "Tier-4 follow-up cycle, not this one."

## Próximo passo

This proposal is the design substrate for `bucket=migrations
mode=implement`. The implementer should:

1. Land IR extensions first (`TenantMigration` + `DeployCheckpoint`
   + `AppDeploy` field expansion). Verify `cargo build` + serde
   round-trip.
2. Add `parse_tenant_migration` to the canonical-indent slice
   next to `parse_job`. Wire through `parse_feature_skeleton`.
3. Add the four new `deploy` child recognisers in
   `parse_app_manifest`.
4. Add the 8 doctor rules in `crates/lazuli_cli/src/doctor.rs`.
5. Add LSP hovers + completions.
6. Add `--expand=migrations` projection in
   `crates/lazuli_cli/src/main.rs`.
7. Add `lazuli plan --check <name>` subcommand.
8. Extend the canonical fixture with the `tenant_migration` block
   on the `customer` feature + the four new `deploy` fields + the
   `checkpoint baseline` declaration + pin the snapshot file.
9. Write 8 doctor fixtures + inspect golden.
10. Update `docs/next-checklist.md` row 24 (Phase L) with a note
    that `tenant_migration` joined the slice's coverage.
11. Add a new row in `docs/next-checklist.md` describing the
    Tier-4 follow-up cycle ("Phase L Tier 4 + resource decorators
    (Route A) — gates on Tier 4 + atlas adapter wiring").

When Route C lands, the migrations bucket has typed IR for
`tenant_migration` + `deploy` policy + `previously` cross-checks +
checkpoint snapshot integrity. Drusa codegen can consume the
typed IR for the per-tenant atlas/golang-migrate dispatch. The
roadmap §1.6 resource decorators wait for Tier 4 — which is the
correct ordering, not a regression.
