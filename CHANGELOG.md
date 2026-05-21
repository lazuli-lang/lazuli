# Changelog

All notable changes to Lazuli land here. Format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) and the
semver + tier discipline codified in `docs/release-policy.md`.

Three independent version units appear in this file:

- **CLI / project semver** — the section heading (`[0.x.y]` / `[Unreleased]`).
- **`LZIR_SCHEMA`** — the IR JSON ABI version (`crates/lazuli_ir/src/lib.rs`).
  Bumps land under "Changed" with their additive-field list.
- **`lazuli.dev/runtime` Go module** — runtime semver (`runtime/go/go.mod`).
  Behavior changes (501 → live, retry semantics) land under "Changed".

Every `LZIR_SCHEMA` bump must ship a paired
`migrations/recipes/<from>-to-<to>/<recipe>/` directory per
`docs/release-policy.md` §"Migration recipes" (CI gate
`MIGRATION-RECIPE-001`).

## [Unreleased]

### Added

- **Codegen-correctness cycle 3
  (`codegen-correctness-cycle-3-2026-05-21`) — 4 LAZ items closed.**
  Hostpoint stayed at **95/95 pass, 0 skip**, but now with **0
  retained workarounds**: cycle 3 removed every workaround cycle 2 had
  to keep. Atelier moved from "generated Go does not build" to
  **compiles + binary builds** (`atelier-api.exe`, 34.5 MB) with
  generated Go tests green. New gap inherited by cycle 4:
  `LAZ-ATELIER-DUPLICATE-API-LIST`, where `./atelier-api.exe --help`
  panics during init on duplicate API registration for `list`, likely
  from synth-generated `list` colliding with an authored query.
  Closures:
  - **WAR-LAZ-RU-TENANT-UPDATE-OFFSET-01** (RT1/RT2/RT3): runtime SQL
    builders now pass explicit placeholder start indexes into
    `baseScopeConditions`, preventing placeholder collisions between
    `SET`, tenant/policy scope, and explicit `WHERE` loops. Regression
    coverage now locks apply-delete soft delete, list, lookup,
    multi-SET + multi-WHERE, and no-tenant idempotency paths. Hostpoint
    reverted the cycle-2 traveler sub-step `@fn` handler workaround and
    returned to generated `updates Traveler { ... }` plus authored
    `invalidates query.lookup_my_traveler`. (`84fa169` + `b9b0c2e`;
    hostpoint `b4a47f9`)
  - **LAZ-ATELIER-COMPILE** (AE1/AE2/AE3): audit reduced 12 build
    failures to two root causes: missing generated handler-import gates
    and a generated `go.mod` that replaced but did not require
    `lazuli.dev/runtime`. Codegen-go now gates handler imports on
    `feature.has_any_fn_reference()`, emits the runtime requirement,
    fixes `list of T` lowering, and emits implicit `Empty` outputs.
    Atelier's generated server now builds end-to-end. (`e01221e`;
    lazuli-ops `30e1427`, `3ea16d2`; atelier `7510967`, `2387458`)
  - **LAZ-DOCTOR-DOCS-THIN** (DOC1/DOC2): five flagged doctor modules
    now have full module headers with severity and fires/warns
    coverage, and `tests/module_headers.rs` self-enforces the header
    contract across 38 modules. (`1ae45c6` + `e56f066`)
  - **LAZ-HOSTPOINT-WORKAROUNDS-LINGER** (WO1/WO2/WO3): hostpoint's
    `bumpLifecycle` SQL fallback is gone, `progressTravelerTo` uses the
    traveler API end-to-end, the stale-time audit found 0 retained
    `staleTime: 0` workaround sites, and parser support now accepts
    canonical `triggers transition a, b`, legacy `triggers a, b`, and
    block-form triggers. (hostpoint `5ef022b`; `8f87d7f`)

- **Codegen-correctness cycle 2
  (`codegen-correctness-cycle-2-2026-05-21`) — 5 LAZ items plus
  doctor/docs polish closed.** Hostpoint moved from 94/95 with 1 skip
  to **95/95 pass, 0 skip** after regen and traveler happy-path
  unskip (hostpoint `5df6d6f`, `61394ea`, `31e992d`). Cycle 2 has
  **no breaking changes**; the query wire-name rename from cycle 1
  remains the last breaking change in this area. New gap inherited by
  cycle 3: `WAR-LAZ-RU-TENANT-UPDATE-OFFSET-01`, where generated
  tenant-scoped `updates Traveler` SQL collides placeholders after SET
  bindings; hostpoint temporarily reroutes traveler sub-step commands
  through `@fn` handlers. Closures:
  - **LAZ-RU-UPDATED-AT** (RU1/RU2/RU3): runtime `applyUpdates` and
    `applyDeletes` no longer append `"updated_at" = now()` to
    resources that do not declare the column. Codegen now emits
    `Timestamps: true` from the `uses_timestamps()` predicate, and
    doctor diagnostic `@correctness.updates_missing_updated_at` warns
    when an `updates` command targets a resource without `updated_at`.
  - **LAZ-INVALIDATES-AUTHORING** (IA1/IA2/IA3): commands can now
    author an `invalidates` block in `.lzi`. The analyzer normalizes
    `<feature>.query.<name>` targets and warns on unknown targets.
    Codegen-ts merges author-declared and same-feature auto-derived
    invalidates with deduplication.
  - **LAZ-ATELIER-GRAMMAR-DRIFT** (AT1/AT2/AT3): atelier audit
    surfaced 7 unsupported forms. Grammar now accepts
    `index on (col, col)` and compound `unique (col, col)` forms;
    the atelier pilot migrated to canonical syntax, and 9 atelier
    features now lower correctly.
  - **LAZ-RATELIMITBYENV-UNKNOWN** + **LAZ-VIEW-REDACTED-FIELDS**
    (PE1/PE2): resolved the pre-existing baseline `lazuli_cli` lib
    test failures in `ir_stub`-based fixtures.
  - **Doctor + docs polish** (DC1/DC2): 14 correctness modules are
    wired into `DoctorPackage::diagnostics()`, and `docs/diagnostics/`
    now catalogs 103 diagnostic rules.

- **Codegen-correctness cycle (5 LAZ gaps closed, 20 cells, 3 waves).**
  Closed the codegen gaps surfaced by the hostpoint playwright sweep
  (`docs/proposals/codegen-correctness-cycle-2026-05-21.md` —
  proposal lives in the `lazuli-ops` companion repo). Pilot evidence:
  hostpoint 94/95 pre-cycle → 95/95 post-cycle once C1 unskips the
  traveler happy path. Five closures:
  - **LAZ-route-id-codegen** (A1/A2/A3/A4): `command save_X route id: ID`
    now lowers to an emitted Go input-struct field (`Id ID
    \`json:"id" validate:"required"\``) ahead of the typed inputs.
    Codegen-ts contract locked by fixture. Doctor diagnostic
    `@correctness.route_id_unused_in_effect` catches shadow bugs (route
    slot redeclared in input with a divergent type). LSP completion for
    `input.<>` inside `effect.bindings` now suggests route params.
    Deletes the `bumpLifecycle` SQL-bump workaround in
    `hostpoint-app/e2e/helpers/onboarding-progress.ts`. (`bd33755`
    + `a69d9c9` + `cf9db79` + `7d7789d` + `0718de0`)
  - **LAZ-invalidates-codegen** (A5/A6/A7): codegen-ts now derives
    `invalidates` from feature queries for any `Creates` / `Updates` /
    `Deletes` `Effect` (walks the IR's resource→queries index; explicit
    `command.invalidates` still wins for back-compat). Doctor warning
    `@correctness.mutation_without_readback` fires when a mutation
    targets a resource that has zero `lookup_*` / `list_*` queries.
    Runtime test locks `useLazuliCommand` consuming
    `spec.invalidates` and calling
    `queryClient.invalidateQueries({ queryKey: ["lazuli", name] })`.
    Unblocks reverting the `staleTime: 0` workaround in
    `hostpoint-app/src/shell/App.tsx`. (`a69d9c9` + `e69c2a1`)
  - **LAZ-record-column-jsonb** (A8/A9): record-typed resource columns
    now lower to `JSONB` (and `Many<UserDefined<Record>>` to a single
    `JSONB`, not `JSONB[]`, avoiding the pgx scan trap). Doctor
    info-level `@info.record_column_jsonb` surfaces the storage kind
    for every record column so authors don't have to grep migrations.
    (`a69d9c9` + `137cb87`)
  - **LAZ-create-table-alter** (A10/A11/A12): new `schema_diff` module
    (`crates/lazuli_codegen_go/src/emitter/schema_diff.rs`) compares
    the current IR's resource columns against the latest emitted
    migration on disk and surfaces a `SchemaDiff` with adds / drops /
    type-changes. Migration emitter turns the diff into an
    `NNN+1_<feature>_<resource>_alter.sql` (paired `.down.sql`); drops
    gate behind a new `lazuli generate go . --allow-drops` flag (opt-in,
    irreversible, doctor warns when triggered). NOT-NULL adds without a
    default degrade to nullable + `-- TODO` comment for safety. Doctor
    warning `@correctness.migration_out_of_sync` catches authors who
    added a column to the IR and forgot to regen. (`8f88659` + `a69d9c9`
    + `3824eba`)
  - **LAZ-query-wire-name** (B1/B2/B3): query `Name` drops the redundant
    `.query.` infix — wire shape becomes `<feature>.<query>` (matches
    the command pattern; the `/c/` vs `/q/` HTTP prefix already
    disambiguates kind). Touches codegen-go (5 sites) + codegen-ts +
    runtime registry + cache contract literals. Runtime integration
    test (`runtime/go/lazuli/http_query_mount_test.go`) locks the new
    mount path `/api/v1/q/<feature>.<query>` and asserts 404 on the
    legacy `.query.` form. Hostpoint URL probe sweep done in B3 (see
    hostpoint commit `9530fe2`). (`38c7683` + `9222283`)

- **Roadmap §1 vertical audit (Wave 5) — 38 `[x]` flips + 11 partial
  clarifications.** Evolve manager-probe cycle
  `roadmap-§1-vertical-audit-2026-05-17` confirmed the predecessor
  pattern (~95% closure for non-pilot-gated items) extends to the
  full §1 expansion list. 38 silently-shipped primitives flipped to
  `[x]` with evidence anchors in `lazuli-ops/docs/roadmap.md`.
  Headline finds: `openapi` built-in generation, `lazuli_changelog`
  crate, `webhook_event` registry + replay + dlq, 6 security headers,
  `secret_rotation`, full i18n stack (`locale`/`translation`/
  `locale_negotiate`), `aggregate`/`invariant`/`slug` DDD trio,
  `cache` profile, `plan`/`subscription` billing. The audit
  surfaced one naming reconciliation hit: `missing-translation` lint
  thought missing is actually shipped as
  `rule_message_ref_unresolved` + `translation_key_unused` (same
  false-negative-by-code-name pattern as the 6 stale-named
  diagnostics from `phase-l-tier-4-spine-scope.md`).
- **`@scope.owner` / `@scope.same_org` policy-to-SQL lowering on
  command effects.** When a command's policy atoms include either
  scope axis, codegen now auto-injects the WHERE binding on
  `Updates` / `Deletes` effects, constraining the affected row at
  the database (not just at the policy-check gate). Closed-catalog
  column priorities: `@scope.owner` resolves `user_id` > `user` >
  `owner_id` > `owner` → `ctx.user.id`; `@scope.same_org` resolves
  `org_id` > `org` > `tenant_id` > `tenant` → `ctx.user.org_id`.
  Closes the SHIP-NOW row-ownership gap surfaced by the hostpoint
  pilot 2026-05-17 capability matrix. Resources without a matching
  column silently skip (defense-in-depth opt-in). (`c0a4609`)
- **`resource_unique_qualifier_unknown` + `resource_validates_path_unknown`
  doctor lints (future-ready).** Two of the three NEW Tier 4c lints from
  the naming-reconciliation proposal §4. Walker code shipped + wired
  into the doctor pipeline; both stay silent today because
  `lower_resource_decl` hardcodes `Resource.constraints` and
  `Resource.validates` to empty Vec. When the analyzer wires those
  slots from the AST, the lints start firing automatically. The
  third lint (`field_derived_from_unresolved`) shipped in `4b03b66`
  and IS firing. Tier3FeatureFacts grows an `extensions` slot to
  support the `@validator.<name>` cross-reference. (`81c1e2e`)
- **`@scope.self` atom — ctx-as-key WHERE.** When the policy includes
  `@scope.self`, codegen binds the row's `id` directly to
  `ctx.user.id` (the acting user IS the target row). Suppresses the
  route/input id binding to avoid double-binding. Unblocks
  `account.choose_role` per the hostpoint Phase 4 audit. (`14e2642`)
- **Bulk-delete mode for `@scope.*` policies.** When a `deletes`
  command has NO route and `Command.input` is `Empty` AND a scope
  atom is present, codegen drops the legacy `{"id": FromInput("ID")}`
  fallback. SQL composes `DELETE WHERE <scope_col> = $N AND <tenancy>`
  without per-row id constraint, surfacing a `// bulk: ...` comment.
  Unblocks `account.logout`. (`14e2642`)
- **`field_derived_from_unresolved` doctor lint (Tier 4c).** Warns
  when a resource field's `derived from <expr>` references
  identifiers that don't resolve to sibling fields on the same
  resource. Tokenises the expression, drops keywords / numerics /
  string literals / dotted paths, checks each remaining bare
  identifier against the resource's fields. First of three net-new
  Tier 4c lints per the naming-reconciliation proposal. (`4b03b66`)
- **Alt-key WHERE binding for `Updates` / `Deletes` codegen.** New
  `resolve_where_keys` helper picks the WHERE binding from
  `Command.route` (composite key for multi-route commands) OR a
  single typed input slot, falling back to `{"id": FromInput("ID")}`
  for routeless / multi-input commands. Closes the hardcoded
  `"id": FromInput("ID")` assumption surfaced by the hostpoint Phase 4
  audit. Unblocks 2 more SHIP-NOW handlers (`unregister_web_push`,
  `dev_auto_approve_charge`); the other 6 still gate on status-guards
  / ctx-as-key / bulk-delete / negation / INSERT-with-constants
  extensions. (`e50c846`)
- **Relation-traversal `@scope.owner` (subquery WHERE form).** When a
  command's policy includes `@scope.owner` AND the target resource
  has no direct owner column BUT has a field referencing another
  local resource that DOES have an owner column, codegen emits
  `lazuli.FromCtxOwnedVia(related_table, owner_column, ctx_path)`.
  Runtime composes `<fk> IN (SELECT id FROM <related_table> WHERE
  <owner_column> = $N)` and AND-chains it with the standard id +
  tenancy WHERE clauses. One-hop only; deeper chains gate on a 3rd
  pilot. New runtime `sourceCtxOwnedVia` Source kind +
  `ownedViaSubquery` payload; new `whereConditionFragment` SQL
  helper. Unblocks the 8 BLOCKED hostpoint handlers per the Phase 4
  capability audit. (`11fc4af`)
- **`SCOPE-OWNER-COLUMN-001` doctor warning.** Companion to the
  codegen lowering: fires when a command's policy includes
  `@scope.owner` or `@scope.same_org` but the targeted resource has
  no matching ownership / tenant column. Surfaces the codegen's
  silent-skip at design time so authors don't ship a policy that's
  only enforced at the role-check gate. (`fa5dfb5`)
- **`lazuli inspect <symbol> --format=lazuli`** now renders a compact
  human-readable view of the symbol lookup (kind + feature +
  path:line + previously trailer + imported-via trailer). The JSON
  format stays normative. (`7b48a04`)
- **Cross-feature `imported_via` resolution** in
  `lazuli inspect <symbol>`. When the qualifier names a feature that
  `uses` another feature owning the symbol, the output's
  `imported_via` field carries the owning feature + the `uses_at`
  source location. (`117b624`)
- `lazuli inspect --expand=defaults` is now IR-driven (reads
  `Tier3FeatureSlice.defaults` instead of re-walking source). Legacy
  text-walker retained as `inspect_defaults_legacy` for documents that
  do not lower. (`a4080ba`)
- `lazuli inspect --expand=commands` — projects `feature.commands:
  Vec<lazuli_ir::Command>` verbatim, including every Command field
  (`rate_limit`, `audit`, `approval`, `invalidates`, `external_calls`,
  `timeout`, `retry`, `idempotency`). (`1327348`)
- `lazuli inspect --expand=apis` — projects `feature.apis:
  Vec<lazuli_ir::Api>`. Accepts both `api` and `apis` tokens.
  (`1327348`)
- `lazuli inspect --expand=resources` — projects `feature.resources:
  Vec<lazuli_ir::Resource>`. (`09f15de`)
- `lazuli inspect --expand=queries` — projects `feature.queries:
  Vec<lazuli_ir::Query>`. (`09f15de`)
- `lazuli inspect --expand=records` — projects `feature.records:
  Vec<lazuli_ir::Record>`. (`09f15de`)
- Runtime registry hooks for webhook + notification wiring:
  - `webhooks.RegisterEventPublisher(p EventPublisher)` — breaks the
    `lazuli ↔ webhooks` import cycle; installed by `lazuli.init`
    in `runtime/go/lazuli/http.go`.
  - `webhooks.RegisterIdempotencyChecker(fn)` — installs an inbound
    dedupe hook (mirrors the prelude/increment hook pattern).
  - `notifications.Registry.RegisterThrottleStore(store ThrottleStore)`
    — optional binding consulted before each dispatch.
  - `notifications.Registry.RegisterDigestStore(store DigestStore)` —
    optional binding for `digest` mode. (`f4d1149`)

### Changed

- **BREAKING — query wire `Name` shape.** The redundant `.query.` infix
  on emitted query `Name`s is gone. Every pilot's `dist/go/<feature>/
  query.gen.go` and `dist/web/<feature>/<feature>.gen.ts` regenerates
  with the shorter form. Pilot consumers must regen + sweep any
  hardcoded URL probes (`/api/v1/q/<feature>.query.<name>` →
  `/api/v1/q/<feature>.<name>`); see the cycle's B3 cell. Verified
  by `runtime/go/lazuli/http_query_mount_test.go` which 404s on the
  legacy form. (`38c7683`)

- **`LZIR_SCHEMA` bumped from `0.14.0` to `0.15.0`** (additive,
  serde-default-skipped). New fields/types:
  - `Command.rate_limit`, `Command.audit`, `Command.approval`,
    `Command.invalidates`, `Command.external_calls`.
  - New types: `Api`, `AuditSpec`, `ApprovalSpec`, `ApprovalThen`,
    `InvalidatesSpec`, `Feature.apis`.
  - `JobDeclarative` spine: `raw_target` / `raw_lets` / `raw_effect`
    strings replaced with typed `target: Option<TargetExpr>`,
    `lets: Vec<LetBinding>`, `effect: CommandEffect` (canonical Phase
    1a shape).
  - All additions carry `#[serde(default, skip_serializing_if = "...")]`
    so `0.14.0` fixtures deserialize unchanged.
  - 6 fixture pins updated `"0.14" → "0.15"` across `doctor` / `lsp` /
    `lazurite/templates/default/app/app.lzi.tmpl`. (`ebe8050`)
- Doctor's command-level `audit emit_to` health check is now IR-driven
  (`audit_event_health_diagnostics` walks
  `Command.audit.emit_to` from `&tier3_facts`). Webhook / job / poller
  bodies remain on the text walker pending Tier 4d audit-IR. (`ebe8050`)
- **Second-wave L0 audit closed (language side)**: cache /
  notifications / webhooks / migrations / openapi all reach IR +
  doctor + LSP completion. Admin remains pilot-gated by design
  (no L0 surface; consult the swarm operating manual for the
  rationale).
- **Runtime: notifications + webhooks no longer return 501.**
  - `notifications/dispatch.go:Send` — wire-thin orchestration
    (marshal → resolve recipient via gjson → resolve `tenant_from`
    + content-addressed dispatch envelope ID → throttle check →
    digest path → channel fanout → retry via `jobs.NextDelay` →
    `lazuli.Publish` per emit). Throttle exhaustion returns `nil`
    (intentional skip per `contract.go:142`).
  - `webhooks/receive.go:handleOne` — wire-thin
    (`io.ReadAll` → `VerifyHmacSignature` → optional replay window
    via `time.ParseDuration` + RFC3339 → `json.Unmarshal` →
    `tenant_from` resolve → idempotency dedupe hook → handler
    invoke → emit + `runWebhookIncrement` → `200`).
    `runtime/go/lazuli/webhooks/receive_test.go::TestHandleOneNotImplementedStatusGone`
    regression-guards the elimination.
  - `notifications/throttle_store.go` rewritten as a wire of
    `golang.org/x/time/rate.Limiter` (one bucket per
    `(notification, recipient, channel)`; `Allow()` consults
    `ReserveN`, returns `retryAt` from `r.DelayFrom(now)`,
    and `CancelAt`s the reservation on reject so the future slot is
    not consumed). `parseDuration` helper preserved (DSL-literal
    parsing, unrelated to throttle logic).
  - `runtime/go/go.mod` adds `golang.org/x/time v0.15.0` as a direct
    dependency and `github.com/tidwall/gjson v1.19.0` (promoted from
    indirect).
  - All four L0 runtime stubs (Tier 4 + this cycle) are now live;
    pilots emitting notifications or consuming inbound webhooks no
    longer hit `501`. (`f4d1149`)

### Internal

- Phase L Tier 4 fully closed end-to-end. All six expansion axes
  (`defaults` / `commands` / `apis` / `resources` / `queries` /
  `records`) read from `Tier3FeatureSlice` and serialize their typed
  IR verbatim. Doctor + LSP + inspect now share one canonical-indent
  slice as the source of truth for these axes; no text re-walks
  remain on the IR-driven paths. (`a4080ba` + `ebe8050` + `1327348` +
  `09f15de`)
- Doctor sentinel `__TIER_4A__` sealed (placeholder in
  `tenancy_axis_for` docstring removed). (`a4080ba`)
- `Tier3FeatureSlice` lifecycle gate now admits `defaults`,
  `commands`, `apis`, `resources`, `queries`, and `records` so a
  standalone `--expand=<axis>` invocation populates the lookup
  (Tier 4a oversight where `defaults` was lifted but not gated;
  corrected together in 4b.2). (`1327348`)
- `migrations/recipes/0.14-to-0.15/schema-bump-additive/recipe.toml`
  authored to satisfy `docs/release-policy.md` §"Migration recipes"
  (CI gate `MIGRATION-RECIPE-001`). Recipe kind is `additive` —
  identity input/output since the bump is binary-additive via
  serde-default. (`836222b`)
- `runtime/go/lazuli/internal/wiresmoke` meta-regression test guards
  against `StatusNotImplemented` / "not yet implemented" / "not
  implemented" literals re-appearing in `notifications/` or
  `webhooks/` impl files. (`84c6217` + `8dfad59`)
- 5 per-axis JSON-shape tests added covering each new
  `--expand=<axis>` projection (commands / apis / resources /
  queries / records). 529 `lazuli_cli` lib tests green (+5 from
  baseline 524). (`8626334`)
- `notifications.TestSendNotYetImplementedErrorGone` functional
  meta-regression mirrors `webhooks.TestHandleOneNotImplementedStatusGone`.
  LSP rich hovers for `command` / `api` / `query.list` /
  `query.lookup` / `query.sql` plus the simple hovers for
  `aggregate|entity` / `record` / `defaults` cross-reference their
  `lazuli inspect --expand=<axis>` flag so cold-reader LLMs discover
  the inspect projections from the keyword itself. (`99d9c5f`)
- LSP hovers extended for `approval` / `invalidates` /
  `external_calls` — the 3 new `Command` fields shipped in Tier 4b
  whose docs were thin or absent. (`357c664`)
- Auth session mock in `runtime/go/lazuli/auth/session_test.go`
  updated to match the 3-column `SELECT id, "user", expires_at`
  SQL that `ResolveSession` now emits (WAR-RUNTIME-CTX-01 closure).
  Two pre-existing test failures
  (`TestIssueResolveInvalidateSessionRoundTrip`,
  `TestResolveSessionExpired`) cleared. (`379b2cd`)

### Known gaps

- **Codegen-correctness cycle Wave C (pilot regens) not yet closed at
  cycle-close time.** C1 (hostpoint regen + unskip
  `onboarding-traveler-flow.spec.ts:19` + revert `staleTime: 0` +
  drop `bumpLifecycle` SQL workaround → 95/95 playwright pass), C2
  (pleiades regen verify), C3 (atelier regen verify), and C4
  (erudito + hostpoint-OS regen verify) remain in flight. Pilot pass
  count stays at hostpoint 94/95 with one skip until C1 lands. See
  `docs/proposals/codegen-correctness-cycle-2026-05-21-close.md`
  (lazuli-ops repo) for the per-cell status table and any deferred
  items surfaced by the cycle.
- **`RateLimitByEnv.unknown_envs` doctor lint missing.** Cell A6
  (mutation-without-readback) noted in passing that the env-qualified
  `rate_limit` IR shipped earlier this week has no diagnostic that
  fires when an author names an `EnvName` outside the closed catalog
  (`dev`/`staging`/`prod`/`test`). The parser already rejects unknown
  envs at lex time, so this is defense-in-depth — not blocking. Filed
  for a follow-up cycle.
- The 6 doctor diagnostics named in
  `phase-l-tier-4-spine-scope.md` acceptance lists
  (`resource_unique_qualifier_unknown`,
  `query_scope_override_missing_reason`, …) do not exist by those
  literal codes. Deferred to a follow-up cycle as a diagnostic-naming
  reconciliation, not slice/IR work. (Noted in commit `09f15de`
  message.)
- `cache_tags_referenced_but_undeclared` doctor lint remains a
  placeholder at `crates/lazuli_cli/src/doctor.rs:12314`. Closes when
  the parser lifts `invalidates tag:<label>` into a typed
  `InvalidationTarget::Tag` variant. Not blocking pilots; LSP
  fallback continues to cover the surface.
