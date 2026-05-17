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
