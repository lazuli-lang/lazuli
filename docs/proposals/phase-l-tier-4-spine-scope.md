# Phase L Tier 4 — Command/Resource/Query/Record Spine Scope (pre-implementation)

**Status**: pre-implementation investigation. Closes the last open
sub-task of `docs/next-checklist.md:60` row 24 so the canonical-indent
slice can finally retire the legacy text-pattern doctor collectors
for commands, resources, queries, and records — and dissolve the
`JobDeclarative.raw_*` carve-out introduced by Tier 3 Route C.

**Audience**: language team (Lazuli core), Lazuli Go runtime team.

**Date**: 2026-05-11.

## Contexto

Phase L now ships three of four planned tiers. Tier 1 lifted `auth`
(commit `e1d8521`, `docs/proposals/auth-lowering-scope.md`). Tier 2
typed `@cap.File(...)` (commit `f60f6bf`,
`docs/proposals/bucket-storage-cycle.md`). Tier 3 (Route C) lifted
`job` / `webhook` / `notification` / `event_group` (commits
`e89ff27 → a4c8bf1`, `docs/proposals/phase-l-tier-3-job-effect-scope.md`).
The slice's skip-list in
`crates/lazuli_syntax/src/parser.rs:1231-1235` is now a single
sentence:

> Any other feature child is skipped silently — Phase L still leaves
> resources/commands/queries/workflows to the text-pattern doctor
> pipeline (Tier 4).

Tier 4 closes that sentence. Four constructs remain in text-pattern
territory:

- `command <name>` blocks (`examples/full-capsule/full-capsule.lzi:226-301`,
  `:753-760`). Carry `route`, `input`, `policy`, `rate_limit`,
  `audit`, `approval` (Cut A.9), `target`, `let`, `creates` /
  `updates` / `deletes`, `emits`, `invalidates`, `tests`.
- `api <name>` blocks (`:303-309`). Sibling of `command` for typed
  HTTP endpoints; carries `method`, `path`, `output`, `policy`,
  `rate_limit`, `handler`.
- `resource <Name>` blocks
  (`examples/full-capsule/full-capsule.lzi:45-67`, `:706-719`).
  Carry `previously`, fields with `@cap` / `@semantic` / `@pii`
  decorators, `has_many`, `derived from`, `soft_delete`,
  `retention`, `validates`.
- `query.list` / `query.lookup` / `query.sql` blocks (`:83-143`,
  `:721-731`). Carry `modifier`, `params`, `scope`,
  `scope override`, `filters`, `search`, `cache`, `paginate`,
  `keys`, `returns`, `sql`.
- `record <Name>` blocks (`:73-81`). Carry inline field/type pairs;
  optional `discriminator` marker on a field for `output discriminator`
  resolution.
- `defaults` block — specifically `defaults.tenancy <axis>`
  (`:20-23`). Currently a no-op for `tenancy_axis_for`
  (`crates/lazuli_cli/src/doctor.rs:5192-5198`).

The legacy pest pipeline at `crates/lazuli_analyzer/src/lib.rs:57-83`
(`lower_document`) still owns these constructs for the IR
projection consumed by codegen and downstream doctor cross-checks.
That pipeline does not carry `defaults.tenancy`, does not surface
the slice's typed AST line offsets, and predates every Cut A
primitive. It is **the** structural reason text-pattern facts still
multiply in `crates/lazuli_cli/src/doctor.rs` (`collect_feature_symbols`,
`collect_command_approvals`, `collect_feature_commands`,
`collect_external_calls_in_block`, `collect_api_paths`,
`collect_feature_resources`).

The Phase L agent's Tier 3 note (commit `f60f6bf`) flagged the
declarative spine (`target` / `let` / `creates` / `updates` /
`emits`) as the largest unresolved surface. Tier 3's Route C
recommendation
(`docs/proposals/phase-l-tier-3-job-effect-scope.md:362-388`)
deferred the spine to Tier 4 explicitly: *"Tier 4 inherits the
declarative shape from a real command parser, not from a job parser
building it first."* This proposal honours that contract.

## Estado IR atual

### What exists today (no extensions needed)

| Type | Location | Notes |
|---|---|---|
| `Command` struct | `crates/lazuli_ir/src/lib.rs:500-521` | `name`, `kind` (Create/Update/Delete/Returns), `route: Vec<RouteSlot>`, `input: CommandInput`, `target: Option<TargetExpr>`, `lets: Vec<LetBinding>`, `effect: CommandEffect`, `policy: PolicyRef`, `emits: Vec<String>`, `tests`, `previous_names`, `span_ref`. Full coverage of the fixture's command surface modulo `audit` + `approval` + `rate_limit` + `invalidates` (see gaps below). |
| `CommandKind` enum | `crates/lazuli_ir/src/lib.rs:523-529` | Closed catalog. |
| `CommandInput` enum | `crates/lazuli_ir/src/lib.rs:537-547` | `Short`/`Typed`/`Empty`. |
| `TargetExpr` / `LetBinding` / `CommandEffect` | `crates/lazuli_ir/src/lib.rs:556-614` | Already shared with `Job.body` (Tier 3 reuse). This is the spine the carve-out defers. |
| `Resource` struct | `crates/lazuli_ir/src/lib.rs:303-330` | `name`, `tenancy`, `soft_delete`, `timestamps`, `fields`, `constraints`, `validate`, `validates`, `previous_names`, `span_ref`. Full coverage of fixture. |
| `Field` struct | `crates/lazuli_ir/src/lib.rs:332-344` | `name`, `type_ref`, `required`, `unique`, `default`, `previous_names`, `span_ref`. Missing: `derived_from` (`is_high_value: Boolean derived from score > 80`, `full-capsule.lzi:56`). |
| `Query` enum | `crates/lazuli_ir/src/lib.rs:629-704` | `List` / `Lookup` / `Sql` with full v0 child coverage (`params`, `scope`, `scope_override`, `filters`, `order`, `paginate`, `modifier`, `keys`, `returns`, `sql_path`). |
| `Defaults` struct | `crates/lazuli_ir/src/lib.rs:1869-1878` | `tenancy: Option<Tenancy>`, `timestamps`, `policy`. Already exists; just unread by the slice. |
| `Tenancy` enum | `crates/lazuli_ir/src/lib.rs:1883-1892` | `Org` / `Team` / `Custom(String)` / `None`. Full coverage. |
| `Workflow` / `Transition` | `crates/lazuli_ir/src/lib.rs:1122-1160` | Exists; not in Tier 4 surface (workflow is structurally a sibling of command, but parses through a smaller grammar — separable). |

The IR spine the Phase L agent's Tier 3 note worried about is
**entirely already present**. Tier 4 builds the parser for shapes
the IR has carried since Phase 1a; it does not require new IR
shape work for the spine itself.

### What is missing on `Command`

The fixture writes four shapes the IR drops today (or fixes them as
text-pattern facts):

1. `rate_limit "<N per period per scope>"` (`full-capsule.lzi:231`,
   `:251`, `:270`, `:296`, etc.) — no `rate_limit: Option<String>`
   on `Command`. Today's `Agent.rate_limit`
   (`crates/lazuli_ir/src/lib.rs:1681` after Cut A.7 erratum) and
   `Job` paths (Tier 3 lifted `Job.timeout` but not job rate-limit
   because the fixture never writes one) prove the shape — `String`
   verbatim, parsed by adapters. **Additive `rate_limit: Option<String>`.**
2. `audit <actor>, <target.id>, <input.field>` + `emit_to <event_group>`
   (`full-capsule.lzi:271-272`) — no `audit: Option<AuditSpec>` on
   `Command`. The observability bucket (row 37) already lifted
   `InspectAudit.emit_to: Option<String>` and `audit_emit_to_unknown`
   doctor diagnostic; what's missing is the **IR** `AuditSpec`
   carrying `subjects: Vec<String>` + `emit_to: Option<String>`.
3. `approval` block (Cut A.9, `full-capsule.lzi:273-277`) — no
   `approval: Option<ApprovalSpec>` on `Command`. Today's
   `CommandApprovalFact` text-walker
   (`crates/lazuli_cli/src/doctor.rs:4691-4821`) is the entire
   storage. Cut A.9's commit note (`b0304b4`) explicitly defers
   the IR field to "Phase L migration covers commands". This is
   that migration.
4. `invalidates query.<name>(...)` (`full-capsule.lzi:240-242`,
   `:259-261`, `:282-284`, `:299-301`) — no `invalidates: Vec<InvalidatesSpec>`
   on `Command`. Today's text-side only check is a presence walk
   for cache-invalidation cross-checks (none in the fixture cycle
   today). **Additive `invalidates: Vec<InvalidatesSpec>`** with
   `InvalidatesSpec { query: QualifiedName, args: Vec<NamedArg> }`
   (reuses `NamedArg`/`Expr` from the spine).

### What is missing entirely (new IR struct)

`Api` struct — surface authored
(`full-capsule.lzi:303-309` and parallel uses), no IR struct
today. `Api` is structurally a sibling of `Command` but with HTTP
transport bound: `method: HttpMethod`, `path: String`, plus
`policy`, `rate_limit`, `output: TypeRef`, `handler: PathRef`.
`HttpMethod` already exists (`crates/lazuli_ir/src/lib.rs` after
Cut A.7). **New struct `Api { name, method, path, policy, rate_limit, output, handler, span_ref }`**
on `Feature.apis: Vec<Api>`. Today the only consumer is `collect_api_paths`
(`crates/lazuli_cli/src/doctor.rs:4309-4373`) which lifts to
`ApiPathFact`; that fact becomes IR-driven after Tier 4.

### What is missing on `Field`

`derived_from: Option<Expr>` — the fixture writes
`is_high_value: Boolean derived from score > 80`
(`full-capsule.lzi:56`). Today's resource lowering treats this as
opaque text (no diagnostic, no projection). **Additive
`derived_from: Option<Expr>`** reusing the existing `Expr` enum.

### What is missing on `Resource`

`retention: Option<RetentionSpec>` — the fixture writes
`retention 7y then anonymize` (`full-capsule.lzi:60`). No IR
shape today; the rule is text-only. **Additive
`retention: Option<RetentionSpec>` with `RetentionSpec { duration: String, action: RetentionAction }`** (closed
catalog `Anonymize | Delete | Archive`). Doctor cross-check
becomes IR-readable.

### Net IR work

- **3 additive fields on `Command`** (`rate_limit`, `audit`,
  `approval`, `invalidates` → 4 fields).
- **2 additive fields on `Resource`/`Field`** (`derived_from`,
  `retention`).
- **1 new struct** (`Api`) + 1 new field on `Feature` (`apis`).
- **3 small support shapes** (`AuditSpec`, `ApprovalSpec`,
  `RetentionSpec`).
- **Zero new spine work** — `TargetExpr`, `LetBinding`,
  `CommandEffect`, `Expr`, `NamedArg` all already exist.

This is the **smallest** of the four tiers in IR shape work because
the spine already lives in `crates/lazuli_ir/src/lib.rs:556-614`
from Phase 1a. The work is concentrated in the **parser** and in
**retiring text-pattern collectors**, not in IR design.

## Estado atual de doctor / inspect para commands/resources/queries/records

### Text-pattern facts that retire when Tier 4 lands

These collectors are pure consequences of the slice's skip-list.
Once Tier 4 lifts the headers + bodies, the facts become trivial
field reads on lowered IR. Each is anchored:

| Collector | Location | What it harvests | Tier 4 fate |
|---|---|---|---|
| `collect_feature_symbols` | `crates/lazuli_cli/src/doctor.rs:3417-3501` | `enum <Name>`, `record <Name>`, `command <name>`, `query.{list,lookup,sql} <name>` headers + per-record fields. | **Retires.** Replaced by `feature.enums`, `feature.records`, `feature.commands.iter()`, `feature.queries.iter()` reads from the lowered IR. |
| `collect_feature_commands` | `crates/lazuli_cli/src/doctor.rs:1228-1288` | `command <name>` headers + `policy`, `route` slots. | **Retires.** Replaced by `feature.commands` + `Command.policy`, `Command.route`. |
| `collect_command_approvals` | `crates/lazuli_cli/src/doctor.rs:4712-4821` | `command <name>.approval` blocks + `by`/`timeout`/`then`/`required_when`. | **Retires.** Replaced by `Command.approval: Option<ApprovalSpec>` read. |
| `collect_external_calls_in_block` (command branch) | `crates/lazuli_cli/src/doctor.rs:1049-1080` (selector: `trimmed.starts_with("command ")`) | `calls <slot>.<op>` inside commands. | **Retires for commands.** Job branch already retired by Tier 3 via `Job.external_calls`. Adds `Command.external_calls: Vec<ExternalCallRef>` (reusing the Tier 3 IR type). |
| `collect_api_paths` | `crates/lazuli_cli/src/doctor.rs:4309-4373` | `api <name>` headers + `method`/`path`. | **Retires.** Replaced by `feature.apis` + `Api.method`, `Api.path`. |
| `collect_feature_resources` | `crates/lazuli_cli/src/doctor.rs:5207-5320` | `resource <Name>` headers + fields + capability decorators + modifiers. | **Retires.** Replaced by `feature.resources` reads. `@cap.File` already typed (Tier 2); `@cap.Hashed` / `@cap.Encrypted` / `@cap.Token` remain text-pattern until their bucket cycles type them (orthogonal). |
| `tenancy_axis_for` | `crates/lazuli_cli/src/doctor.rs:5192-5198` (currently no-op) | Reads `defaults.tenancy` from text. | **Lifts.** Reads `feature.defaults.tenancy` after Tier 4 parses `defaults`. Removes the conservative "missing `tenant_from`" fallback that Tier 3's `tenant_from` diagnostic still rides on. |
| `collect_policy_atoms` | `crates/lazuli_cli/src/doctor.rs:1290-1326` | `policies` block + per-rule `@role.*` / `@scope.*` / `@actor.*` atoms. | **Partial retirement.** `feature.policies` IR shape exists (`crates/lazuli_ir/src/lib.rs:1671` after audit). Tier 4 parser must lift the `policies` block. Currently lowered by the legacy `aggregate` path only. |

Seven collectors land in retire-or-promote bucket. Two
(`collect_feature_symbols`, `collect_feature_commands`) overlap —
both walk for `command` headers — so the retirement compaction is
larger than the count suggests: doctor.rs gives back ~600-800 lines
once they all migrate.

### Inspect projections that lift to IR-driven

| Today | Tier 4 fate |
|---|---|
| `inspect_features` text-walks `lines` to build `InspectFeature` (`crates/lazuli_cli/src/main.rs:1315-1338+`). | Becomes a lookup against the lowered `ir::Feature` for the canonical-indent-parseable surface; legacy-pipeline lowering kept only for the constructs Tier 4 doesn't yet cover (workflows, surfaces, extensions, escape_routes — out of Tier 4 scope). |
| `--expand=commands` / `--expand=queries` / `--expand=resources` (none exist; all command/query/resource projections live in `summary` only). | New `--expand=commands` / `--expand=queries` / `--expand=resources` mirrors `--expand=jobs` / `--expand=auth` shape — projects the typed IR. |
| `--expand=api` (text-derived, `collect_api_paths` only). | New `--expand=api` reading `feature.apis`. |

### LSP coverage

The LSP already runs file-local checks on commands/resources/queries
(see `crates/lazuli_lsp/src/lib.rs` — many diagnostics walk source
text directly). Tier 4 does **not** retire LSP text-walks because
the LSP must stay responsive without a full workspace lower. What
Tier 4 enables is doctor cross-feature promotion: every text-walk
diagnostic that currently runs *only* file-local can grow a doctor
sibling that runs cross-feature against the lifted IR. That
promotion is opportunistic and per-diagnostic; it does not need to
ship in Tier 4 itself.

## Rotas A vs B vs C

Three ways to close the Tier 4 gap. All honour the language/runtime
boundary.

### Route A — full lift in one run (all four constructs + spine + defaults)

`parse_command` + `parse_api` + `parse_resource` + `parse_query` +
`parse_record` + `parse_defaults` recognise every child the fixture
authors today, including the full declarative spine that Tier 3's
Route C carve-out deferred. IR additive fields land first (4 on
`Command`, 2 on `Resource`/`Field`, 1 new struct `Api`, 3 support
shapes); parser/lowering second; inspect projections third; doctor
text-pattern retirements fourth. The `JobDeclarative.raw_*`
carve-out is replaced by `JobDeclarative { target: Option<TargetExpr>,
lets: Vec<LetBinding>, effect: CommandEffect }` (the original
shape Phase 1a designed) in the same commit window.

Concrete IR work:

1. Extend `Command` with `rate_limit`, `audit`, `approval`,
   `invalidates`, `external_calls` (5 fields).
2. Extend `Resource` with `retention` (1 field).
3. Extend `Field` with `derived_from` (1 field).
4. Add `Api` struct + `Feature.apis: Vec<Api>` (1 struct, 1 field).
5. Add `AuditSpec`, `ApprovalSpec`, `RetentionSpec`,
   `InvalidatesSpec`, `RetentionAction` (5 small new types).
6. Replace `JobDeclarative.raw_target/raw_lets/raw_effect` with
   typed `target: Option<TargetExpr>`, `lets: Vec<LetBinding>`,
   `effect: CommandEffect` (3 fields swap; type swap, not new IR).

Parser work: six new top-level recognisers in `parse_feature_skeleton`:

- `parse_command` — declarative body (`target`, `let`,
  `creates`/`updates`/`deletes`, `emits`) + transport children
  (`route`, `input`, `policy`, `rate_limit`, `audit`,
  `approval`, `calls`, `invalidates`, `tests`).
- `parse_api` — flat list of children (`method`, `path`, `output`,
  `policy`, `rate_limit`, `handler`).
- `parse_resource` — nested fields with capability/semantic/pii
  decorators (already typed in syntax for `@cap.File` via Tier 2;
  others stay as `Unresolved` text until their bucket cycles type
  them — orthogonal to Tier 4).
- `parse_query` — three sub-grammars (`list`/`lookup`/`sql`) each
  with closed children.
- `parse_record` — flat field/type pairs with optional
  `discriminator` marker.
- `parse_defaults` — `tenancy`, `timestamps`, `policy_for jobs,
  webhooks: <atom>`.

Plus the **shared declarative-body recogniser** the spine demands
(`parse_target_expr`, `parse_let_binding`, `parse_command_effect`,
`parse_emits_block`) factored into module-local helpers so
`parse_job` (Tier 3) reuses them — completing the Tier 3 carve-out
in the same commit window.

**Cost (in cells, baseline Tier 1+2 ≈ 2 cells each, Tier 3 ≈ 4.5
cells)**:

- IR extensions + new structs: ~1 cell (smallest of all tiers — see
  §"Estado IR atual" — every new field is structurally trivial;
  the spine swap on `JobDeclarative` is a 1-for-3 type swap).
- Parser `parse_command` (largest of the six — has the spine
  recogniser, plus `audit`, `approval`, `invalidates`, `calls`,
  `tests` children): ~2 cells.
- Parser `parse_api`: ~0.5 cells.
- Parser `parse_resource` (fields + decorators + constraints +
  `previously` + `soft_delete` + `retention` + `validates`):
  ~1.5 cells.
- Parser `parse_query` (three sub-grammars): ~1.5 cells.
- Parser `parse_record`: ~0.25 cells.
- Parser `parse_defaults`: ~0.25 cells.
- Shared spine recognisers (factored helpers; replace Tier 3
  carve-out): ~0.75 cell.
- Inspect projections (`--expand=commands` / `--expand=resources`
  / `--expand=queries` / `--expand=api`; mechanical mirror of
  Tier 3): ~0.75 cell.
- Doctor retirements (deleting `collect_*` walkers + replacing
  reads with `feature.*.iter()`): ~1 cell across ~6 collectors.

**Route A total: ~9.5 cells.**

**Risk**: the largest surface ever lifted in a single Phase L tier
(roughly 2× Tier 3). The shared spine recogniser is load-bearing —
if its shape diverges from what Tier 3 implicitly produced, the
`JobDeclarative.raw_*` → typed swap regresses Tier 3 diagnostics
(`scheduled_job_tenancy_diagnostics`, `event_job_tenant_from_diagnostics`).
Mitigation: the fixture exercises both shapes (`recompute_score_after_invoice`
declarative-body job in `:392-402` is the canary), so the parser
test surface stays fixed.

### Route B — spine-first (commands + defaults + Tier 3 carve-out only; resources/queries/records deferred)

Smallest possible Tier 4 that **closes Tier 3's debt**. Lift
`parse_command`, `parse_api`, `parse_defaults`, and the shared
spine recognisers; replace `JobDeclarative.raw_*`. Defer
`parse_resource`, `parse_query`, `parse_record` to a hypothetical
Tier 5.

Concrete scope:

1. IR extensions on `Command` (5 fields).
2. New `Api` struct + `Feature.apis`.
3. Replace `JobDeclarative.raw_*` with typed spine.
4. Lift `defaults.tenancy` (1 line in IR; `Defaults` already
   exists).
5. Parser: `parse_command`, `parse_api`, `parse_defaults`, shared
   spine helpers.
6. Doctor retirements: `collect_feature_commands`,
   `collect_command_approvals`, `collect_external_calls_in_block`
   (command branch), `collect_api_paths`, `tenancy_axis_for`
   no-op fix.

**Cost**: ~5 cells.

- IR extensions: ~0.5 cell.
- Parser `parse_command` + spine helpers: ~2.5 cells.
- Parser `parse_api`: ~0.5 cell.
- Parser `parse_defaults`: ~0.25 cell.
- Inspect (`--expand=commands` only): ~0.5 cell.
- Doctor retirements (4 collectors): ~0.75 cell.

**Risk**: Phase L's row 24 stays open after the cut lands; the slice's
skip-list shrinks from 4 constructs to 2 (`resource`/`query`/
`record` remain). Doctor.rs still carries `collect_feature_resources`,
`collect_feature_symbols` (records + enums + queries branches),
`collect_policy_atoms` partial — that's ~400 lines of text-pattern
that survives. The text-pattern habit lives until the deferred tier.

### Route C — surface-first per construct (incremental, one PR per construct)

Split Tier 4 into four sequential PRs:

- **Tier 4a**: `parse_defaults` + lift `defaults.tenancy` (fixes
  the canonical `tenancy_axis_for` no-op). ~0.5 cell.
- **Tier 4b**: `parse_command` + `parse_api` + shared spine
  helpers + retire `JobDeclarative.raw_*`. ~3.5 cells.
- **Tier 4c**: `parse_resource` + retire `collect_feature_resources`.
  ~2 cells.
- **Tier 4d**: `parse_query` + `parse_record` + retire
  `collect_feature_symbols` query/record branches. ~2 cells.

Each PR ships independently, each retires a bounded subset of
text-pattern facts.

**Cost**: same ~9.5 cells in total, split across four PRs.

**Risk**: minimal per PR; maximum at the seam between Tier 4b and
Tier 4c — until 4c lands, the lowered `Command` references
resources by name only (`QualifiedName`), and cross-checks between
e.g. `command.creates Customer` and `resource Customer.fields` run
against half the pipeline (lowered `Command`) and half the
text-pattern (text-walked `resource`). The seam is **already
present** today (Tier 3 lifted jobs but not the resources jobs
write to), so 4b doesn't introduce new asymmetry — it just keeps
it for one more PR window. That asymmetry resolves naturally when
4c ships.

### Comparison

| Axis | Route A (full) | Route B (spine-first) | Route C (incremental) |
|---|---|---|---|
| Upfront cost (cells) | ~9.5 | ~5 | ~9.5 total (~0.5 + ~3.5 + ~2 + ~2) |
| Phase L row 24 close | Yes — single commit window. | No — `resource`/`query`/`record` survive as Tier 5. | Yes — incrementally, over 4 PRs. |
| `JobDeclarative.raw_*` retirement | Yes. | Yes (via 4b). | Yes (via 4b). |
| Doctor text-pattern collectors retired | All 7. | 4 of 7 (commands + api + tenancy + command-side external_calls). | All 7, incrementally. |
| Risk of single failed review | High — large diff, six new recognisers. | Low — fewer constructs, but seam stays open longer. | Lowest per PR; same total surface as A. |
| Phase L compat | Aligned — row 24 closes. | Misaligned — creates implicit Tier 5. | Aligned — row 24 closes after 4d. |
| Spine reuse with Tier 3 | Yes — Job parser reuses the new helpers immediately. | Yes (via 4b). | Yes (via 4b). |
| LSP / inspect promotion eligible | All file-local diagnostics for command/resource/query/record bodies become doctor-cross-feature candidates. | Only command-side. | Same as A, incrementally. |
| Bisect-friendliness | One commit owns the regression. | One commit owns Tier 4 work; deferred Tier 5 still owns the rest. | Four commits; per-PR bisect easy. |
| Author cognitive load | Heavy — six grammars at once. | Light — one grammar (`command` + leaf siblings). | Light per PR. |

### Recomendação

**Route C (incremental).** Three reasons:

1. **Phase L's discipline has been one tier per commit window so
   far.** Tier 1 was ~2 cells, Tier 2 ~2 cells, Tier 3 ~4.5 cells.
   Route A's ~9.5-cell single tier breaks that cadence and risks
   review compression. Tier 3's Route C recommendation
   (`docs/proposals/phase-l-tier-3-job-effect-scope.md:506-540`)
   explicitly drew a *natural line* between leaf grammars and the
   spine; Route C honours the same discipline by drawing a second
   natural line between the spine (commands + defaults) and the
   sibling constructs (resources, queries, records) that lift
   independently once the spine exists.
2. **Tier 4b is the load-bearing PR.** It closes the
   `JobDeclarative.raw_*` carve-out — the single piece of debt
   Tier 3 explicitly owed Tier 4. Once 4b lands, the slice has a
   typed declarative spine usable by both `parse_command` and
   `parse_job`; Tier 3's promise is fulfilled. 4c and 4d become
   mechanical follow-ups; if either is deferred, the slice is
   already in good shape.
3. **Tier 4a is a 0.5-cell trivial cut that retires a known
   no-op.** `tenancy_axis_for`'s comment
   (`crates/lazuli_cli/src/doctor.rs:5192-5198`) explicitly names
   Tier 4 as the fix. Shipping it standalone proves the cadence
   and unlocks the Tier 3 fallback (`tenant_from` diagnostic
   currently rides on the conservative "axis unknown → only check
   presence" branch). Doing it first means every subsequent
   Tier 4 PR runs with `defaults.tenancy` already typed, removing
   one source of test-fixture friction.

Route B is rejected because it leaves Phase L row 24 open by
design — it's a half-tier dressed as a tier. Once the spine lands
(via 4b), there is no reason to permanently defer 4c/4d; sequencing
them as Route C does costs nothing extra and closes the row.

Route A is rejected as the largest tier ever attempted in Phase L
in a single commit window; the gain (single-commit close) does not
justify the review risk when Route C achieves the same end state
across four PRs that each fit Phase L's established cadence.

**Implementation order (Route C)**:

1. **Tier 4a — `parse_defaults`** (smallest, fixes a known no-op).
2. **Tier 4b — `parse_command` + `parse_api` + spine helpers + retire `JobDeclarative.raw_*`** (largest of the four, but the keystone).
3. **Tier 4c — `parse_resource`** (fields + decorators + constraints; the second-largest, but independent of command lift).
4. **Tier 4d — `parse_query` + `parse_record`** (smallest pair; queries reuse the spine indirectly through `Filter`/`Predicate`).

Why this order:

- **4a first**: 0.5 cell, retires the canonical no-op comment, gives
  every later tier a typed `feature.defaults.tenancy` to read.
- **4b second**: closes Tier 3's `raw_*` debt — highest-priority
  outstanding commitment. Until 4b lands, every consumer reading
  `JobDeclarative` must special-case `raw_*`.
- **4c third**: resources are referenced by commands
  (`command.creates Customer` requires `resource Customer`). Lifting
  resources after commands means the cross-check
  (`command_create_target_unknown` style) lights up.
- **4d last**: queries and records are referenced by commands
  (`target query.by_id(...)`) and agents (`output discriminator
  CustomerStatus` against `record` discriminator field). Like 4c
  they unlock cross-checks; smaller than 4c because the grammars
  have fewer children.

After 4d, **row 24 of `docs/next-checklist.md` closes**. The slice's
skip-list comment in `crates/lazuli_syntax/src/parser.rs:1231-1235`
collapses to a single sentence: *"Workflows and escape routes stay
in the legacy pipeline for now."*

## PILOT-NEEDED vs SPECULATIVE

Classification of every command/resource/query/record additive field
against fixture evidence.

### PILOT-NEEDED — exercised by the canonical fixture today

| Field | Construct | Fixture evidence | Tier 4 fate |
|---|---|---|---|
| `Command.rate_limit: Option<String>` | `rate_limit "10 per minute per ip"` | `:231`, `:251`, `:270`, `:296`, `:308`, `:756` | **Lift in 4b.** Already used by every command/api in the fixture. |
| `Command.audit: Option<AuditSpec>` | `audit actor, target.id, input.owner_id` + `emit_to audit_log` | `:271-272` | **Lift in 4b.** Observability bucket already lifted `InspectAudit.emit_to`; IR catches up. |
| `Command.approval: Option<ApprovalSpec>` | `approval` block with `required_when`, `by`, `timeout`, `then` | `:273-277` | **Lift in 4b.** Cut A.9's commit note explicitly defers IR to Phase L. |
| `Command.invalidates: Vec<InvalidatesSpec>` | `invalidates query.list`, `invalidates query.by_id(id: route.id)` | `:240-242`, `:282-284`, `:299-301` | **Lift in 4b.** Reuses existing `QualifiedName` + `NamedArg`. |
| `Command.external_calls: Vec<ExternalCallRef>` | `calls <slot>.<op>` | Authored in jobs today (`:766-768`); commands authoring this pattern is the canonical runtime-bucket expectation for the `customer_outreach` feature. Tier 3 already typed `Job.external_calls`; Tier 4 mirrors for commands. | **Lift in 4b.** Reuses `ExternalCallRef` IR type from Tier 3. |
| `Api` struct + `Feature.apis: Vec<Api>` | `api customer_export` with `method`, `path`, `output`, `policy`, `rate_limit`, `handler` | `:303-309` | **Lift in 4b.** Replaces `collect_api_paths` text-pattern. |
| `Field.derived_from: Option<Expr>` | `is_high_value: Boolean derived from score > 80` | `:56` | **Lift in 4c.** Reuses existing `Expr` enum. |
| `Resource.retention: Option<RetentionSpec>` | `retention 7y then anonymize` | `:60` | **Lift in 4c.** Closed catalog `Anonymize | Delete | Archive`. |
| `Defaults.tenancy` | `defaults: tenancy org` | `:20-21`, `:438-439`, every feature in the fixture | **Lift in 4a.** Already exists in IR; just unread by slice. |
| `Defaults.timestamps` | `defaults: timestamps` | `:22`, `:440` | **Lift in 4a.** Same. |
| `Defaults.policy_for jobs, webhooks: @actor.system` | `policy_for jobs, webhooks: @actor.system` | `:23` | **Lift in 4a.** `Defaults.policy` IR field exists; needs the qualifier-list grammar (`policy_for <kinds>: <atom>`). Bucket-jobs cycle already used this text-side. |
| `command body` (target / let / creates / updates / deletes / emits) | `target query.by_id(...)`, `let resolved_owner = ...`, `updates Customer`, `emits customer_reassigned` | `:268-281` and every command | **Lift in 4b.** Shared with `parse_job` via factored helpers; closes Tier 3's carve-out. |
| `command input` (typed and short forms) | `input` block with typed lines; `input file` short form | `:227-229`, `:266-267`, `:293-294`, `:753-754` | **Lift in 4b.** `CommandInput::{Typed,Short,Empty}` already in IR. |
| `command route <name>: <Type>` slots | `route id: ID` | `:265`, `:292` | **Lift in 4b.** `RouteSlot` already in IR. |
| `resource fields + decorators` | `@semantic.Email @pii.contact required`, `@cap.File(...)`, `@cap.Encrypted(...)` | `:49`, `:55`, `:708` | **Lift in 4c.** `@cap.File` already typed (Tier 2); `@cap.Hashed`/`Encrypted`/`Token` stay `Unresolved` until their bucket cycles. |
| `resource constraints` (`unique X per Y`) | `unique email per org` | `:70-71` | **Lift in 4c.** `UniqueConstraint.per: Option<String>` already in IR. |
| `resource has_many` | `has_many notes: CustomerNote inverse customer` | `:57` | **Lift in 4c.** `has_many` IR shape exists (`crates/lazuli_ir/src/lib.rs:380+`). |
| `resource soft_delete` | `soft_delete` | `:59` | **Lift in 4c.** `Resource.soft_delete: bool` exists. |
| `resource previously` | `previously migrated Account` | `:46` | **Lift in 4c.** `previous_names` exists. |
| `resource validates @validator.<x>` | `validates @validator.tier_check` | `:62` | **Lift in 4c.** `validate: Option<PathRef>` exists. |
| `query.list params + filters + search + cache + paginate + order + modifier + scope` | `:83-104`, `:110-124` | **Lift in 4d.** Every `Query::List` field already typed. |
| `query.list scope override + reason` | `scope override\n  reason "..."` | `:116-118` | **Lift in 4d.** `scope_override: bool` exists; `reason` is text-only today — additive `scope_override_reason: Option<String>`. |
| `query.lookup by <key>: <Type>` | `query.lookup by_id by id: ID` | `:106`, `:108` | **Lift in 4d.** `LookupQuery.keys: Vec<KeyClause>` exists. |
| `query.sql returns + scope + sql` | `:126-143` | **Lift in 4d.** `SqlQuery.returns` + `sql_path` exist. |
| `record <Name> + fields` | `record CustomerLtv` with three fields | `:73-81` | **Lift in 4d.** No IR struct today; **additive** `Record` struct mirroring `Resource` shape minus `tenancy`/`constraints`/`soft_delete`. |

### SPECULATIVE — not in the fixture; defer until pilot evidence

| Field | Status | Why defer |
|---|---|---|
| `Command.lifecycle` (before/after/around hooks) | Roadmap §1.3 (`docs/roadmap.md:87`); not in fixture. | Pilot needed: a product authoring command lifecycle hooks. Today's `validates resource` + `extensions.hook` cover the same surface in a smaller way. |
| `Resource.lock` decorator (optimistic/pessimistic) | Roadmap §1.5 (`docs/roadmap.md:97`); not in fixture. | Pilot needed: a product hitting concurrent-update conflicts in evals. Today no fixture command writes contended state under load. |
| `Resource.outbox` / `inbox` kinds | Roadmap §1.5; not in fixture. | Pilot needed: a product whose event publication needs at-least-once contract beyond what `emits` + Tier 3 retry covers. |
| `Query.lock for_update` modifier | Not in roadmap; not in fixture. | Pilot needed: same as `Resource.lock`. |
| `Record.tagged_union` discriminator beyond Cut A | Cut A already uses records as discriminator targets (`output discriminator CustomerStatus`); fixture only authors single-enum discriminators. | Pilot needed: an agent whose output type is a tagged-union record (not an enum). |
| `command transactional` / `command saga` annotations | Not in fixture; not in roadmap §1 directly. | Pilot needed: a product whose command coordinates multiple resources transactionally and needs the contract elevated to language level. Today's text-pattern `idempotency by` + Tier 3 retry/timeout covers single-resource cases. |
| `api openapi_export` | Roadmap §1.10; not in fixture. | Pilot needed: a product publishing OpenAPI artifacts from authored `api` blocks. The output is codegen, not language. |
| `resource derived_from` for multi-field computed columns | Fixture authors single-expression `derived from score > 80`. | Already PILOT-NEEDED (above) as `Field.derived_from`. Multi-field / cross-resource is SPECULATIVE — defer until a fixture exercises it. |

Net result: **17 PILOT-NEEDED items** land across Tier 4a-4d.
**7 SPECULATIVE items** defer to pilot evidence. The pilot-vs-
speculative split is the *smallest* of any Phase L tier because
Tier 4 lifts constructs that have been authored in the canonical
fixture since Phase 1a; there is very little speculation.

## Closed-cycle criterion para Tier 4

Adapted from Tier 1's auth criterion
(`docs/proposals/auth-lowering-scope.md:192-238`) and Tier 3's
job criterion
(`docs/proposals/phase-l-tier-3-job-effect-scope.md:443-504`).

The criterion is split per-PR (Route C); a tier-4 PR can ship only
if its row is checked.

### Tier 4a — `parse_defaults`

- [ ] Fixture's every `defaults` block parses through the slice.
      `parse_feature_skeletons` returns `FeatureSkeleton` carrying a
      populated `defaults: Defaults` (additive on `FeatureSkeleton`).
- [ ] `lazuli inspect --expand=defaults` projects `Defaults` for
      every feature. New projection.
- [ ] `tenancy_axis_for(&Feature)`
      (`crates/lazuli_cli/src/doctor.rs:5192-5198`) reads
      `feature.defaults.tenancy` instead of returning `None`. The
      no-op comment retires.
- [ ] `event_job_tenant_from_diagnostics` (Tier 3) and
      `webhook_tenant_from_diagnostics` (Tier 3) cross-check the
      axis correctly — the current "axis unknown → only check
      presence" fallback retires.
- [ ] No regression in existing tests.

### Tier 4b — `parse_command` + `parse_api` + spine

- [ ] Fixture's every `command` / `api` block parses through the
      slice. `FeatureSkeleton.commands: Vec<Command>` and
      `FeatureSkeleton.apis: Vec<Api>` populated.
- [ ] `JobDeclarative.raw_target/raw_lets/raw_effect` replaced by
      typed `target: Option<TargetExpr>`, `lets: Vec<LetBinding>`,
      `effect: CommandEffect` (the original Phase 1a shape).
- [ ] `IR_SCHEMA` minor-bumped to record the spine swap +
      `Command`/`Api` additive fields. (`crates/lazuli_ir/src/lib.rs:33`).
- [ ] `lazuli inspect --expand=commands` projects the typed
      command surface; `--expand=api` projects `Api`.
- [ ] Six doctor diagnostics promote from text-pattern to
      IR-driven:
      - `approval_contract_diagnostics` (Cut A.9)
      - `approval_timeout_shape_diagnostics` (Cut A.9)
      - `approval_then_invalid_diagnostics` (Cut A.9)
      - `command_external_call_*` (the `INT-CALL-*` family today
        on `ExternalCallFact`)
      - `audit_emit_to_unknown` (observability bucket)
      - `api_path_collision_diagnostics`
- [ ] `collect_command_approvals`, `collect_api_paths`, and
      `collect_external_calls_in_block` (command branch) delete.
- [ ] `collect_feature_commands` deletes; `commands_by_key` reads
      from `feature.commands.iter()`.
- [ ] No regression in Tier 3 diagnostics
      (`scheduled_job_tenancy_diagnostics`,
      `event_job_tenant_from_diagnostics`,
      `webhook_tenant_from_diagnostics`,
      `webhook_verify_diagnostics`,
      `idempotency_key_diagnostics`).

### Tier 4c — `parse_resource`

- [ ] Fixture's every `resource` block parses through the slice.
      `FeatureSkeleton.resources: Vec<Resource>` populated.
- [ ] `Field.derived_from` and `Resource.retention` lift.
- [ ] `lazuli inspect --expand=resources` projects the resource
      surface (with typed fields, capability decorators, constraints,
      `has_many`).
- [ ] Three doctor diagnostics promote from text-pattern to
      IR-driven:
      - `resource_unique_qualifier_unknown` (already exists
        text-side)
      - `resource_validates_path_unknown` (already exists
        text-side)
      - `field_derived_from_unresolved` (new — cross-field name
        resolution becomes possible with typed fields).
- [ ] `collect_feature_resources` deletes; doctor reads from
      `feature.resources.iter()`.

### Tier 4d — `parse_query` + `parse_record`

- [ ] Fixture's every `query.list` / `query.lookup` / `query.sql`
      block parses through the slice.
      `FeatureSkeleton.queries: Vec<Query>` populated.
- [ ] Fixture's every `record` block parses through the slice.
      `FeatureSkeleton.records: Vec<Record>` populated. New IR
      struct `Record { name, fields, discriminator_field, span_ref }`.
- [ ] `lazuli inspect --expand=queries` and `--expand=records`
      project the typed surfaces.
- [ ] Four doctor diagnostics promote from text-pattern to
      IR-driven:
      - `query_scope_override_missing_reason` (already exists
        text-side)
      - `query_filter_param_unknown` (already exists text-side)
      - `query_search_field_unknown` (already exists text-side)
      - `record_discriminator_unknown` (already exists text-side
        via `collect_feature_symbols`)
- [ ] `collect_feature_symbols` query/record branches delete (enum
      branch stays for now — enums are tiny and used elsewhere).
- [ ] `collect_policy_atoms` deletes if `parse_query` lifts the
      `policies` block (or stays pending until a Tier 5 if not —
      policies are sibling of feature, not of query).
- [ ] **`docs/next-checklist.md` row 24 closes** after this PR.

The cross-PR closed-cycle criterion: after 4d ships, the slice's
skip-list in `crates/lazuli_syntax/src/parser.rs:1231-1235`
collapses; `lower_feature_skeleton` returns a `Feature` with
every fixture-authored field populated; the legacy pest pipeline
(`lower_document` / `lower_aggregate`) retires for the
canonical-indent code path (it stays available for the legacy
`aggregate { ... }` examples in `examples/crm.lzi`, which is a
separate compatibility surface).

## Doctor text-pattern facts that become trivial after Tier 4

For audit completeness, the explicit list of collectors that retire
or convert to one-line IR reads:

### Full retirement (collector deletes)

1. `collect_feature_commands` (`crates/lazuli_cli/src/doctor.rs:1228-1288`).
   Replaced by `for command in &feature.commands`.
2. `collect_command_approvals` (`:4712-4821`).
   Replaced by `feature.commands.iter().filter_map(|c| c.approval.as_ref())`.
3. `collect_api_paths` (`:4309-4373`).
   Replaced by `for api in &feature.apis`.
4. `collect_feature_resources` (`:5207-5320`).
   Replaced by `for resource in &feature.resources`.
5. `collect_external_calls_in_block` command branch (`:1049-1080`).
   Replaced by `feature.commands.iter().flat_map(|c| &c.external_calls)`.

### Partial retirement (collector slims)

6. `collect_feature_symbols` (`:3417-3650+`). Record/query/command
   branches delete; enum branch survives until Tier 5 (enums lift
   alongside workflows/surfaces in a later cut).
7. `collect_policy_atoms` (`:1290-1326`). Either deletes when
   `parse_feature_skeleton` lifts the `policies` block (4d), or
   stays pending until a Tier 5 if `policies` is deferred. (The
   tier 4d closed-cycle criterion lists both options.)

### Conversion (no-op fix)

8. `tenancy_axis_for` (`:5192-5198`). Inner body becomes
   `feature.defaults.tenancy.as_ref().map(...)`. The TODO comment
   retires.

After Tier 4 lands in full (4a + 4b + 4c + 4d), `doctor.rs` shrinks
by an estimated **~600-800 lines** of text-pattern collector code,
and the remaining text-pattern collectors are bounded to:

- `collect_api_paths` (deletes; listed above)
- `collect_lzx_*` (orthogonal — `.lzx` is its own surface)
- Enum / workflow / surface / extension / escape_route walkers —
  the next Phase L tier's scope.

## Recomendação

1. **Take Route C (incremental, four PRs)**. Estimated total scope:
   **~9.5 cells** (4a ~0.5 + 4b ~3.5 + 4c ~2 + 4d ~2). Larger than
   Tier 3 (~4.5 cells) but split across four PRs that each fit the
   established cadence.
2. **Implementation order**: 4a → 4b → 4c → 4d. Reasons in
   §"Recomendação" above; the keystone is 4b (closes Tier 3's
   `JobDeclarative.raw_*` carve-out + ships the shared spine).
3. **Land the IR extensions inside each PR**, ordering inside each
   PR follows Tier 1's erratum (`docs/proposals/auth-lowering-scope.md:21-23`):
   IR extensions first, then parser, then inspect, then doctor
   retirements. Lowering that writes IR fields the IR doesn't
   carry yet wastes a review cycle.
4. **Retire doctor text-pattern collectors inside the same PR that
   lifts their construct.** Each retirement is mechanical once the
   IR field exists; deferring them costs review attention later.
   The conversion of `tenancy_axis_for` from no-op to real read
   ships **inside 4a** so the tier-3 diagnostics fallbacks retire
   immediately.
5. **Defer 7 SPECULATIVE items** (`Command.lifecycle`,
   `Resource.lock`, `outbox`/`inbox`, `Query.lock`,
   tagged-union records, `command transactional`,
   `api openapi_export`). Each gates on pilot evidence the §0
   bucket cycle would surface in a subsequent run.
6. **Update `docs/next-checklist.md` row 24** progressively:
   - After 4a: row 24 note adds "Tier 4a done — `defaults.tenancy`
     lifted, `tenancy_axis_for` no-op retired".
   - After 4b: row 24 note adds "Tier 4b done — commands, apis,
     and Tier 3 `JobDeclarative.raw_*` carve-out retired. Spine
     shared between job and command parsers".
   - After 4c: row 24 note adds "Tier 4c done — resources lifted".
   - After 4d: **row 24 closes** with status `done`. The slice's
     skip-list shrinks to `workflow`/`surface`/`extensions`/
     `escape_route`, which become a possible Tier 5 (out of scope
     here).
7. **No `IR_SCHEMA` jump beyond minor bumps.** Every additive
   field is `#[serde(default, skip_serializing_if = "...")]` so
   downstream JSON consumers see a backward-compatible IR. The
   `JobDeclarative.raw_*` → typed spine swap is **the** ABI risk;
   mitigate by making it a serde-compatible additive transition
   (keep `raw_*` serde-deprecated for one release window before
   removing).

When Tier 4 ships in full, **every feature child the canonical
fixture authors lowers through `parse_feature_skeleton`**. The
canonical-indent slice becomes the only producer for the IR;
text-pattern collectors retire from doctor; LSP file-local
diagnostics gain doctor cross-feature siblings on demand. The
language/runtime boundary stays as defined: nothing in this proposal
introduces provider names, DI mechanics, transport, or sidecar
IR — every additive field is a closed catalog or a `String`
verbatim consumed by adapters.

The fixture's canonical surface is then **complete** in the slice's
contract. Phase L's premise — that every text-pattern fact is
implicit IR debt — is fully discharged.
