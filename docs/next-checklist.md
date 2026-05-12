# Lazuli Next Checklist

This is the live working checklist for upcoming language cuts. Keep it small,
practical, and updated after each implementation so design pressure does not
get lost in chat history.

## Current Position

- `app.lzi` is the app entrypoint and operational contract.
- `registry.lzi` is the package-level catalog for env groups, capabilities,
  integrations, adapters, packs, and other global bindings.
- `env group` exists to organize app env schema without changing `env.NAME`
  references.
- `integrations` exists as a provider-neutral registry contract, not as a
  provider operation spec. It may live in `registry.lzi`, or temporarily in
  `app.lzi` for small apps.
- Service boundaries are logical ownership contracts; the Lazuli runtime may materialize the
  same graph as a monolith, modular monolith, or split services.
- Adapter and dependency injection mechanics are runtime concerns. Lazuli
  owns the registry contract and typed bindings; it should not grow a
  `container.lzi` until real plugin/runtime pressure proves that `registry.lzi`
  cannot express the contract.
- `workspace.lzi` is the optional distributed-system contract for multi-app,
  polyrepo, external-service, and gateway graphs. It is not required for normal
  apps and does not replace per-app `app.lzi`.
- Top-level `.lzx route` blocks now use `route <name>: <Type>` for path slots
  and `route.<name>` for references, matching command/view locator syntax.
  Legacy `stack` is removed. `params` only names query/API read arguments;
  `path` only names URL strings.
- `lazuli inspect` on a `.lzx` file reports `routes`, `experiences`, and
  `surfaces` alongside any `app` manifest.

## Next Implementation Cuts

| Order | Cut | Status | Notes |
|-------|-----|--------|-------|
| 1 | Feature-level integration requirements | done | Add `requires integration gateway: PaymentGateway` so reusable features depend on abstract capabilities, not concrete providers. |
| 2 | App bindings | done | Bind `payments.gateway = integrations.mercadopago` or equivalent without making every feature import provider details. |
| 3 | External calls | done | `calls gateway.operation` now works in commands/jobs, appears in inspect, and is checked by LSP/doctor against feature integration slots with timeout/retry/job-idempotency guards. |
| 4 | Integration doctor rules | partial | Missing app binding, undeclared integration, type mismatch, undeclared call slot, missing timeout, missing retry, and missing job idempotency are covered. PII/legal basis/audit waits for external operation data-classification contracts. |
| 5 | Registry layout decision | done | Use native `registry.lzi` package convention with explicit import reserved for future non-standard layouts. |
| 6 | Profiles | done | `profile <environment>` now models URL, binding, integration environment/adapter, and provider-neutral deploy topology overrides with inspect and doctor coverage. |
| 7 | Pack registry | done | `registry.lzi` catalogs packs and app `packs` enables them; doctor lets enabled packs satisfy `uses` and requires bindings for pack integration slots. |
| 8 | Adapter binding provenance | done | Adapter sources now derive `the Lazuli runtime`, `plugin`, or `local` provenance from `@runtime/...`, `@plugin/publisher/name`, `@adapter.<local>`, or local paths; doctor rejects unknown source shapes. |
| 9 | Workspace contract | done | `workspace.lzi` now models local/external apps, shared registry, event boundaries, communication propagation, and provider-neutral gateways with IR/inspect/doctor/LSP coverage. |
| 10 | External contract imports | done | `contract <name>` now models imported OpenAPI/AsyncAPI/Proto/JSON Schema/Avro plus authored records, operations, and events with IR/inspect/doctor/LSP coverage. Core the Lazuli runtime should generate Go transport bindings, not make SDK a language concept. |
| 11 | Gateway/proxy contract | done | Workspace `gateway` covers provider-neutral ingress to apps. Raw proxy, sidecar, service mesh, and provider routing mechanics stay in runtime/adapters. |
| 12 | Syntax highlighting audit | done | TextMate scopes cover current integration/binding/calls/profile/pack/workspace/contract syntax, adapter package refs, top-level `route` declarations, and the realigned `route <name>: <Type>` route slot syntax. Legacy `stack` removed. |
| 13 | IR/inspect coverage audit | done | App, registry, packs, requirements, bindings, external calls, profiles, workspace, contracts, and `.lzx` routes/experiences/surfaces all appear in inspect/doctor. |
| 14 | Final vocabulary cleanup | done | Top-level `.lzx route` blocks now declare path slots with `route <name>: <Type>` and reference them as `route.<name>`, matching command/view route locator syntax. Legacy `stack` removed. `params` is reserved for query/API read arguments. `path` only names URL strings. |
| 15 | AI primitives Cut A | done | `tools` child of `agent`, discriminated `output`, `evals` block with `case`/`requires`/`forbids`. Shipped across Phases 1–7 (commits d2a6202 → b934207). 8 doctor diagnostics + 3 LSP + `--expand=tools` projection. See `docs/proposals/ai-primitives-v0.md` + `ai-primitives-v0-implementation.md`. |
| 16 | `.lzi` formal grammar | done | Canonical indent-form EBNF in `docs/grammar.lzi.md` covers current syntax + Cut A primitives. Lexical layer documented (INDENT/DEDENT contract, token classes, reserved words). Sibling grammars for `.lzx`, `app.lzi`, `registry.lzi`, `workspace.lzi`, `contract.lzi` deferred. |
| 17 | AI primitives Cut A.7 — `expose http` on agent | done | Auto-mounts trivial agent-dispatch endpoints. Removes the duplicated `api customer_summary_stream` boilerplate from the canonical fixture. 2 doctor + 4 LSP diagnostics; new `--expand=expose` projection; `HttpMethod` enum in IR. Commit 3be8611. See `docs/proposals/ai-primitives-cut-a-7.md`. |
| 18 | AI primitives Cut A.8 — `agent_run` trace event (language-side) | done | IR registry `built_in_trace_events()` reserves `agent_run` with canonical payload (tokens/duration/cost/finish_reason/tools[]/safety_decision). Doctor rejects authored redeclarations + subscriber payload-drift. Runtime instrumentation is parallel runtime team work. Commit ac0241d. See `docs/proposals/ai-primitives-cut-a-8.md`. |
| 19 | AI primitives Cut A.9 — `approval` on commands | done | Third write-tool guard alongside `safety` (Cut A) and `idempotency by` (Cut B reserved). 3 doctor diagnostics + 1 LSP; write-tool guard extension so agents dispatching approval-gated commands satisfy the guard without their own `safety`. Text-pattern facts until Phase L migration covers commands in canonical-indent slice. Commit b0304b4. See `docs/proposals/ai-primitives-cut-a-9.md`. |
| 20 | AI primitives Cut A.10 — golden file evals | done | `golden "./path.jsonl" min_score N` reference inside an eval `case` alongside `requires`/`forbids`. Adapter loads + scores; language records the path verbatim. AST + IR + parser + analyzer + LSP. Commit 3f7fcd3. |
| 21 | AI primitives Cut A.11 — CORS in `app.lzi` | done | Allowlist + credentials + max_age declared at `app.lzi` level alongside `urls`. 3 doctor diagnostics (unknown environment, credentials/wildcard conflict, origin not in `urls`) + 1 LSP shape check. Per-endpoint overrides + `allow_methods` customisation deferred to pilot evidence. Commit b3fc39e. See `docs/proposals/ai-primitives-cut-a-11.md`. |
| 22 | AI primitives Cut A.5 (safety list) | pilot-gated | Doctor cross-checks union of `@validator.<name>` coverage against union of `tools[].resolved_pii_classes`. IR shape (`safety: Vec<QualifiedName>`) already lands by Cut A. Gates on first pilot product with multi-class PII fan-in. See `docs/proposals/ai-primitives-cut-a-5.md`. |
| 23 | AI primitives Cut A.6 (tool result schema) | pilot-gated | `RegistryToolEntry.result_record` so adapters expose typed tool results referenceable from prompts/evals (`tools.<x>.<field>`). Gates on first pilot referencing a tool result field. See `docs/proposals/ai-primitives-cut-a-6.md`. |
| 24 | Phase L — canonical-indent slice covers commands/resources/queries/records | **done (2026-05-11), 100% closed** | **Tier 4 complete.** Tier 4a (`parse_defaults`, retire `tenancy_axis_for` no-op, commit `e9f368d`); Tier 4b.1 (`parse_command` + `parse_api` + shared declarative spine, retire `JobDeclarative.raw_*` carve-out, commit `65d0a3b`); Tier 4c (`parse_resource` + `Resource.retention` + `Field.derived_from`, commit `1b54627`); Tier 4d (`parse_query.list/lookup/sql` + `parse_record` + new `ir::Record` shape). **Tier 4b follow-up retirements landed (2026-05-11)**: `collect_command_approvals` retired in favour of IR `Command.approval` reads from `Tier3FeatureFacts` (a minimal `ApprovalBlockPresence` walker survives only for `approval_contract_diagnostics` missing-children — parse-error blocks never reach the IR); `collect_api_paths` + `ApiPathFact` deleted, replaced by `Tier3FeatureFacts.apis` + `api_lines`; `FeatureSymbols.queries` dead map dropped (the `resolve_tool` query branch matched the catch-all arm one-to-one). **Tier 4 follow-up second wave (2026-05-11)** extended the IR + retired four walkers: (A) `RouteSlot.from: Option<String>` lifted; `collect_feature_commands` + `command_route_slot` text-walker retired via `populate_commands_from_ir` (commits `e3488a3` + `5c9ba9f`); (B) `CapabilityRef::Hashed/Encrypted/Token` typed + `BuiltinType::SemanticPhone/Url/Uuid` added; `collect_feature_resources` + `parse_resource_field` retired via `populate_feature_resources_from_ir` reading typed `Field.type_ref` + typed `Field.unique` (commits `142ed4b` + `dcec17b`); (C) `Command.timeout/retry/idempotency` typed (mirror Job); `collect_external_calls_in_block` command branch retired via `populate_command_external_calls_from_ir` (commits `114012a` + `3f6aa2b`); (D) `Tier3FeatureFacts.records` lifted from typed `Feature.records`; record branch of `scan_feature_range` retired (`parse_record_field` + `RecordFact` + `RecordFieldFact` removed); discriminator-field check now reads typed `Record.discriminator_field` + projects field type via `type_ref_name` (commit `b72dba5`). **Tier 4 follow-up third wave (2026-05-11) closes the slice 100%**: (W1) `parse_policies_decl` + `FeatureSkeleton.policies` + `lower_policies_decl`; `collect_policy_atoms` + `feature_line_slice` retired (`populate_commands_from_ir` reads `feature.policies.categories` directly, commit `d77eb59`); (W2) `parse_enum_decl` + `FeatureSkeleton.enums` + `lower_enum_decl`; `scan_feature_range` enum branch retired + `FeatureSymbols.enums` slot dropped; `agent_discriminator_diagnostics` + `check_record_discriminator` walk `Tier3FeatureFacts.enums` (commit `b63ccb9`); (W3) `ExternalCallRef.span_ref` typed; `collect_external_calls_in_block` + `collect_feature_external_calls` + `parse_external_call_header` + `block_has_prefixed_line` deleted; `populate_job_external_calls_from_ir` reads typed `Job.external_calls[*]` with span-driven anchors (commit `71a4454`). **Tier 4 follow-up fourth wave (2026-05-11) cleans up two W3 survivors**: (W4a) inspect-side `collect_policy_atoms` text-walker in `crates/lazuli_cli/src/main.rs` retired — `Tier3FeatureSlice.policies: Policies` now feeds the `category -> atoms` lookup in `inspect_feature`; gating extended so `--expand=policies`/`--expand=tests`/`--expand=migrations` also build the slice; (W4b) `scan_feature_range` + `collect_feature_symbols` + `scan_block_for_policy` text-walkers in `crates/lazuli_cli/src/doctor.rs` retired — `populate_feature_symbols_from_ir` reads `Tier3FeatureFacts.commands[*].policy` via `policy_ref_surface_text` (mirroring the verbatim text the walker captured). **No text-pattern walker remains for Tier 4 constructs (commands / resources / queries / records / apis / policies / enums / external_calls).** See `docs/proposals/phase-l-tier-4-spine-scope.md`. |
| 25 | AI primitives Cut B (flow + budget tokens + knowledge + quota cost) | pilot-gated | Each sub-cut has its own gate spelled in `ai-primitives-v0.md` §"Cut B — deferred". Don't merge without independent re-grade ≥ 8.5 on AI-first axes. |
| 26 | Auth bucket cycle Route A — canonical-indent slice covers `auth` | done | Shipped as **Phase L Tier 1** (commit `e1d8521`, 2026-05-11). `parse_auth` + `parse_auth_identity/password/sessions/mfa/oauth` lift the `auth` block from text-pattern to typed AST/IR. `FeatureSkeleton.auth: Option<Auth>` wired through `lower_feature_skeleton`. IR additive extensions: `AuthPassword.algorithm`, `AuthMfa.enroll`/`verify`. New `--expand=auth` projection in `crates/lazuli_cli/src/main.rs`. See `docs/proposals/bucket-auth-cycle.md` §Linguagem/§IR + `docs/proposals/auth-lowering-scope.md`. |
| 27 | Auth bucket cycle — 4 doctor diagnostics + LSP coverage | done | `auth_password_algorithm_hash_mismatch`, `auth_sessions_resource_unknown`, `auth_identity_field_unknown`, `auth_oauth_adapter_unbound` shipped in `crates/lazuli_cli/src/doctor.rs` + registered in `is_security_enforcement_code`. 13 LSP hovers (including `hash`) + 9 closed-catalog completion values (`argon2id`/`bcrypt`/`google`/`github`/`microsoft`/`apple`/`totp`/`true`/`false`). 8 inline doctor tests + 7 LSP tests + fixture at `crates/lazuli_cli/tests/fixtures/auth/algorithm_mismatch.lzi`. See `docs/proposals/bucket-auth-cycle.md` §Doctor/LSP. |
| 28 | Auth bucket cycle — golden evals + Go expiry test | done | 3 JSONL golden evals in `tests/golden/auth/` (`login_password`, `mfa_totp`, `oauth_google`) + Lazuli Go runtime stubs in `runtime/go/lazuli/auth/` (`password.go`, `session.go`, `oauth.go`, `mfa.go`) carrying typed errors + contract structs. `auth_test.go` pins shape + sentinel catalog; `testing/synctest` expiry test deferred until runtime team implements `IssueSession`/`ResolveSession` bodies. See `docs/proposals/bucket-auth-cycle.md` §Evals/Testes/§Runtime. |
| 29 | Storage bucket cycle Route B — typed `@cap.File` lowering | done | Shipped as **Phase L Tier 2** (commit `f60f6bf`, 2026-05-11). `TypeRef::Capability(CapabilityRef::File)` added to `crates/lazuli_ir/src/lib.rs` (incl. `FileCapability`/`FileSize`/`MimeType`/`FileVisibility`). `type_ref_from_syntax` in `crates/lazuli_analyzer/src/lib.rs` reuses the LSP `capability_args` helper to parse `@cap.File(args)`. Two new args added: `visibility`, `signed_ttl`; existing `max_size`/`accept` promoted to typed. New `--expand=storage` projection. See `docs/proposals/bucket-storage-cycle.md` §Linguagem/§IR + `docs/proposals/bucket-storage-scope.md`. |
| 30 | Storage bucket cycle — 5 doctor diagnostics + LSP coverage | done | `cap_file_visibility_undeclared`, `cap_file_accept_input_output_mismatch`, `cap_file_visibility_signed_ttl_mismatch`, `cap_file_size_unit_invalid` (typed promotion), `cap_file_mime_family_unknown`. LSP hovers for 5 keywords (`@cap.File`, `max_size`, `accept`, `visibility`, `signed_ttl`) + 4 closed-catalog completions. Commits 49bdc7b + 18afc08. See `docs/proposals/bucket-storage-cycle.md` §Doctor/LSP. |
| 31 | Storage bucket cycle — Lazuli Go runtime + local/S3 adapters | partial | `runtime/go/lazuli/storage/{contract,upload,signed,fetch_private}.go` stubs landed: typed `FileContract`/`MimeType`/`FileVisibility`/`Key`/`Metadata` mirroring IR + 5 typed errors (`ErrFileSizeExceeded`/`ErrFileMimeRejected`/`ErrFileNotFound`/`ErrSignedURLExpired`/`ErrVisibilityMismatch`) + `ObjectStore` interface + functional `LocalStore` adapter + `S3Store` type surface (real `aws-sdk-go-v2` impl owned by `@runtime/s3` adapter). `storage_test.go` covers round-trip, size/MIME rejection, visibility mismatch, and signed-TTL expiry via `testing/synctest`. The runtime team owns production adapter packs. See `docs/proposals/bucket-storage-cycle.md` §Runtime/§Evals. |
| 32 | Jobs/Webhooks/Notifications IR lift (scope-out) | done | Phase L Tier 3 (Route C) lifted four feature children of the canonical-indent slice (`parse_job` / `parse_webhook` / `parse_notification` / `parse_event_group`) and extended IR with `Notification`, `EventGroup`, `TenantFromSpec`, `FanoutSpec`, `VerifySpec`, `ExternalCallRef`, plus additive `Job` / `Webhook` fields (`tenant_from`, `fanout`, `timeout`, `external_calls`, `structured_verify`). New `--expand=jobs` / `--expand=webhooks` / `--expand=event_groups` projections mirror `InspectAgent`'s shape. Declarative job bodies preserved as raw strings (`JobDeclarative.raw_*`) until Tier 4 lifts the shared declarative spine. Commits e89ff27 → a4c8bf1. See `docs/proposals/phase-l-tier-3-job-effect-scope.md`. |
| 33 | Jobs bucket cycle — L0→L2 closure | done (language side) | Six IR-driven doctor diagnostics shipped (`JOB-TIMEOUT-001`, `JOB-FANOUT-001`, `JOB-FANOUT-002`, `WEBHOOK-SCOPE-001`, `NOTIF-CHANNEL-001`, `EVENTGROUP-NESTING-001`) + sharpened LSP hovers + Lazuli Go runtime stubs in `runtime/go/lazuli/{jobs,webhooks,notifications}/` (typed `JobContract` + `Dispatcher` + retry helpers; `WebhookContract` + `VerifySpec` + chi-mounted `Mount` + `VerifyHmacSignature`; `NotificationContract` + `Registry` + `ChannelDispatcher` + closed channel catalog). Codegen for `dist/go/<feature>/{jobs,webhooks,notifications}.gen.go` + River/Sendgrid pilot bindings belong to runtime team. Commits 53a5d1a → bridged with Tier 3 commits. See `docs/proposals/bucket-jobs-cycle.md`. |
| 34 | `event_group` doctor pattern-prefix rule | done | Promoted to `event_group_pattern_prefix_diagnostics` in `crates/lazuli_cli/src/doctor.rs`. Reads from the lifted `EventGroup` IR; fires only when an authored event already carries a prefix matching *another* group in the same feature (avoids false positives on short-name promotion). Commits 53a5d1a + 299878e. |
| 35 | Observability bucket cycle — 3 new built-in trace events (`command_run` / `job_run` / `webhook_run`) + `@trace.<name>` namespace | done | `built_in_trace_events()` extended from 1 entry (`agent_run`, A.8) to 4. New `TraceFiresPer::CommandDispatch`. New reference namespace `@trace.<name>` for subscriber jobs. `event_trace_reserved_name_diagnostics` now rejects all 4 reserved names. New `trigger_trace_unknown_diagnostics` rejects unresolved `@trace.<X>` / `trigger event.trace <X>`. Commit bd3e6ac. See `docs/proposals/bucket-observability-cycle.md` §3.5. |
| 36 | Observability bucket cycle — `app.logging` + `app.tracing` blocks | done | New `AppLogging` + `AppTracing` IR structs on `AppManifest` (which now derives `PartialEq` only — `Option<f64>` sample-rate fields prevent `Eq`). 6 doctor diagnostics (`app_logging_level_invalid`, `app_logging_format_invalid`, `app_logging_redact_unknown`, `app_logging_sample_rate_range`, `app_tracing_sample_rate_range`, `app_tracing_exporter_unbound`). 8 LSP hovers + `OBSERVABILITY_CATALOG_VALUES` completion. New `--expand=tracing`/`--expand=logging` projections. `parse_app_manifest` + LSP `app_child_block` recognise both blocks. Commit 71a889a. See `docs/proposals/bucket-observability-cycle.md` §3.1 §3.2. |
| 37 | Observability bucket cycle — audit `emit_to` + `event.trace level` + health probe wiring | done | `InspectAudit.emit_to: Option<String>` resolves to feature `event_group` or reserved `audit_log` / `audit_stream`. `Event.level: Option<String>` (additive serde-default) for trace events. 4 doctor diagnostics (`audit_emit_to_unknown`, `event_trace_level_invalid`, `event_trace_level_on_domain_event`, `health_probe_path_invalid`). 1 new LSP hover (`emit_to`). Fixture exercises both axes. Runtime stubs landed in `runtime/go/lazuli/observability/` (6 files: `logging.go`, `tracing.go`, `health.go`, `trace_emit.go`, `audit.go`, `panic.go`); the runtime team owns the production exporter/handler wiring + the `http.go:28` hardcoded `/healthz` rewire. Commit b1b5d7f + 31eeb3a. See `docs/proposals/bucket-observability-cycle.md` §3.3 §3.4 §Runtime. |
| 41 | Migrations bucket cycle Route C — `tenant_migration` kind + IR/parser/analyzer | done | New `ir::TenantMigration` + `TenantMigrationTarget` (sibling of `Job`) and `Feature.tenant_migrations: Vec<TenantMigration>`. `syntax::TenantMigration` AST + `parse_tenant_migration` in the canonical-indent slice (closed body: `target tenants`, `idempotency by`, `retry`, `timeout`, `handler`). `lower_tenant_migration` mirrors `lower_job`; `lower_resource_decl`/`lower_resource_field` strip the `migrated`/`alias` mode prefix into bare `previous_names`. `AppDeploy` extends additively with `strategy` / `lock_timeout` / `pre_migration_hook` / `post_migration_hook` / `checkpoint`. `parse_app_manifest` recognises all five new deploy children. Commit fb41e5b. See `docs/proposals/bucket-migrations-cycle.md` + `docs/proposals/bucket-migrations-scope.md`. |
| 42 | Migrations bucket cycle Route C — 8 doctor diagnostics + LSP + inspect + `lazuli plan --check` | done | Eight IR-driven diagnostics: `PREVIOUSLY-FWD-001`, `PREVIOUSLY-CYCLE-001`, `PREVIOUSLY-DUP-001`, `TM-AXIS-001`, `TM-IDEMP-001`, `DEPLOY-CHECKPOINT-001`, `DEPLOY-CHECKPOINT-002`, `DEPLOY-STRATEGY-001`. `Tier3FeatureFacts` carries `tenant_migrations` + resource/field rename facts + per-feature name index. LSP gains hovers for `tenant_migration`/`strategy`/`lock_timeout`/`pre_migration_hook`/`post_migration_hook`/`checkpoint`; `DEPLOY_STRATEGY_VALUES` closed catalog enters the completion endpoint. `--expand=migrations` projects every lifted `TenantMigration` per feature. `lazuli plan --check <name>` validates checkpoint path + snapshot `lazuli_version`. 8 fixtures under `tests/fixtures/migrations/` + 8 inline doctor tests. Commits 740c7da + afd3b1c + 9d3c168 + 1173e76. |
| 43 | Migrations bucket cycle Route C — Lazuli Go runtime stub + canonical fixture | done | `runtime/go/lazuli/migrations/` package: typed `TenantMigrationContract` + `Checkpoint` + `DeployPolicy` + closed-catalog `DeployStrategy` mirroring IR; five typed errors (timeout / lock_timeout / max_retries / tenant_axis_unknown / checkpoint_stale); `TenantMigrator` / `TenantDirectory` / `Planner` adapter contracts; reference `Dispatcher` with `Register` + sequential `ApplyAll`; smoke tests for fanout / unknown-axis / handler-error. Canonical fixture `examples/full-capsule/` gains `tenant_migration backfill_customer_score` on `customer`, `strategy rolling` + `lock_timeout` + pre/post hooks + `checkpoint baseline` on `app.lzi`, plus `checkpoints/full-capsule.snapshot.json` snapshot stub and `hooks/migration_{pre,post}.sh`. Concrete atlas/golang-migrate adapter wiring stays in `@runtime/...` packages. Commits e3a0d24 + 9d3c168. SPECULATIVE deferred to Tier-4 follow-up (Route A): 8 resource decorators (`index`, `foreign_key`, `constraint` typed, `enum_column`, `extension`, `trigger`, `generated_column`, `partition`), online-migration helpers, typed field-level diff in `lazuli plan`. |
| 44 | Webhooks expanded cycle — `webhook_event` registry kind + typed `payload from` | done (language side) | New `WebhookEvent` + `WebhookEventField` IR (registry-side catalog of external envelope shapes); new field `AppRegistry.webhook_events`. `parse_app_registry` lifts indent-4 envelope headers + indent-6 typed fields verbatim (provider-side, opaque types). `Webhook.payload_from: Option<WebhookEventRef>` — Atrito #2: structured ref, not opaque string. Parser enforces the `webhook_events.` surface prefix so the catalog hop is obvious to a cold-reading author. `--expand=webhooks` projection surfaces `payload_from: { name, path: "webhook_events.<name>" }`. 1 LSP hover + 2 LSP completions (`webhook_events`, `payload_from`) + extended `registry-contract` allow-list. See `docs/proposals/bucket-webhooks-expanded-cycle.md`. |
| 45 | Webhooks expanded cycle — `replay` + `dlq` + inbound `retry` decorators on `webhook` | done (language side) | Four additive fields on `ir::Webhook` (`payload_from`, `replay`, `dlq`, `retry`). Two new IR structs (`ReplaySpec` with `ReplayMode { Allow, Deny }`; `DlqSpec` with closed `{ Emit, Handler, Drop }` discriminator). Inbound `retry` reuses jobs `RetryPolicy` verbatim — Atrito #5: single-pathed parser/doctor/codegen. Parser supports short (`replay allow within "24h"`) and long forms; mutual exclusion on `dlq` enforced at parse time. Eight new IR-driven doctor diagnostics (`WEBHOOK-PAYLOAD-001/002`, `WEBHOOK-REPLAY-001/002`, `WEBHOOK-DLQ-001/002/003`, `WEBHOOK-EVENT-001`) + 7 LSP hovers + 7 LSP completion keywords. 6 inline doctor tests. Fixture extension on `webhook crm_customer_upsert` exercises the full surface. Lazuli Go runtime extensions to `runtime/go/lazuli/webhooks/` (retry path on `jobs.Adapter`, typed `ReplaySpec`/`DlqSpec`/`PayloadType`, decode+replay-window+DLQ lifecycle) belong to the runtime team. See `docs/proposals/bucket-webhooks-expanded-cycle.md`. |
| 46 | OpenAPI bucket cycle — `deprecated` IR + emitter | done (language side) | `Command.deprecated: Option<Deprecation>` typed (`since`/`replacement`/`sunset`); `DeprecationReplacement` enum classifies `LocalCommand`/`Qualified`/`Url`. `parse_command_decl` extended with `parse_command_deprecated` (in-order key parser: `deprecated since "..." replacement <ref> sunset "..."`). Fixture: `command reassign` flagged with full decoration. tmLanguage adds `deprecated`/`since`/`sunset`/`replacement` keywords. See commit fc28925. |
| 47 | OpenAPI bucket cycle — `lazuli generate openapi` + `lazuli changelog` | done (language side) | New crate `crates/lazuli_openapi` (~620 LOC) walks `Module` features and emits OpenAPI 3.1.0 YAML via a purpose-built printer (no `serde_yaml`). Mapping: commands → POST/PATCH/DELETE per `CommandKind`; route slots → path params; CommandInput Typed/Short/Empty → requestBody; effect → response codes (201 Creates / 200 Updates / 204 Deletes); policy/rate_limit/audit/approval/emits/deprecation → `x-lazuli-*` extensions; resources/records/enums → `components.schemas`; RFC 7807 error envelope → `components.responses.Problem`. Agent `expose http` + `api` blocks mounted. New crate `crates/lazuli_changelog` (~430 LOC) diffs two inspect JSON modules: classifies Added / Removed / Deprecated / Breaking / Non-breaking. Two CLI verbs (`lazuli generate openapi <input>`, `lazuli changelog --from <a.json> --to <b.json>`). See commit fc28925. |
| 48 | OpenAPI bucket cycle — doctor + LSP coverage | done | Five IR-driven doctor diagnostics: `deprecated_replacement_unknown` (LocalCommand/Qualified/Url resolution), `deprecated_sunset_date_invalid` (ISO-8601 shape check), `deprecated_sunset_in_past` (warning vs pivot date), `openapi_text_pattern_api_block` (warning for un-lifted api blocks, dedupes per package), `api_changelog_breaking_change` (deferred to `lazuli_changelog` CI gate). Four LSP `KEYWORD_HOVER` entries (`deprecated`/`since`/`replacement`/`sunset`) at `crates/lazuli_lsp/src/lib.rs:11829`. Four test fixtures under `crates/lazuli_cli/tests/fixtures/openapi/`. Anchored at `crates/lazuli_cli/src/doctor.rs:openapi_deprecated_diagnostics`. |
| 49 | Cache bucket cycle — typed `QueryCache` IR | done (language side) | `ListQuery.cache: Option<QueryCache>` + `SqlQuery.cache: Option<QueryCache>` (additive). `QueryCache { key, ttl, tags, namespace }` + `CacheTtl { Literal(CacheTtlLiteral) | Quoted(String) }` + closed-unit `CacheTtlLiteral { Seconds | Minutes | Hours | Days }`. Analyzer's `lower_query_cache` walks AST raw cache body lines into the typed IR; `parse_cache_ttl` recognises digit-prefixed `<n>(s|m|h|d)` literals. `Command.invalidates: Vec<InvalidatesSpec>` already typed by Tier 4b (no regression). Fixture extended to exercise `tags`/`namespace`; `registry.lzi` declares `cache shared` capability. See commit 1fc0627. |
| 50 | Cache bucket cycle — Lazuli Go runtime stubs + LSP hovers | done (language side) | `runtime/go/lazuli/cache/{contract,tags,adapter}.go`: typed `QuerySpec` + `InvalidationTarget` interface (with `QueryTarget`/`QueryWildcardTarget`/`TagTarget` constructors) + `Backend` interface + `IntersectTags` helper + `Bind`/`Active`/`ErrNotBound` singleton resolution. Concrete LRU / Redis / Memcached / Valkey adapter packs (`@runtime/local`, `@runtime/redis`, etc.) belong to runtime team. LSP hovers: sharpened `cache` + closed unit catalog on `ttl` + new `tags` + new `namespace` hover entries. See commit 1fc0627. |
| 51 | Cache bucket cycle — doctor diagnostics | done | Five IR-driven diagnostics consuming `QueryCache` + `Command.invalidates` + `AppRegistry.capabilities`: `cache_ttl_unit_invalid` (defensive on `CacheTtl::Quoted` empty payload), `cache_invalidates_target_unresolved` (resolves `invalidates query.<name>` against the target feature's `query.list`/`query.lookup`/`query.sql` set, with `query.` qualifier short-form mapping), `cache_tags_referenced_but_undeclared` (parser-gated until `InvalidationTarget::Tag` variant lands), `cache_namespace_collision` (warning when two features share a namespace label), `cache_capability_undeclared` (errors when any query carries a cache but the registry has no `cache <name>` capability). Three fixture files + four inline tests. The legacy text-pattern LSP rules retain file-local coverage for live editing. Anchored at `crates/lazuli_cli/src/doctor.rs:cache_diagnostics`. |
| 52 | i18n bucket cycle — typed `AppLocale` IR + parser | done (language side) | New `AppManifest.locale: Option<AppLocale>` with `default`/`supported`/`fallbacks` shape; `LocaleFallback { from, to }`. Parser (`parse_app_manifest`) recognises `locale` indent-2 block; `default <tag>`/`supported <tag>...`/`fallback <src> -> <dst>` indent-4 children. `app_child_block` + LSP `app_operational_contract_diagnostics` extended to accept the `locale` block without warning. Bare `default_locale` scalar still parses (back-compat). Fixture's `app.lzi` migrated from scalar `default_locale "pt-BR"` to the typed block with two supported tags + one fallback edge. tmLanguage adds `locale`/`supported`/`fallback`/`tags`/`namespace` keywords. See commit (next). |
| 53 | i18n bucket cycle — Lazuli Go runtime stub + LSP hovers | done (language side) | `runtime/go/lazuli/i18n/{contract,negotiate,context}.go`: typed `LocaleContract { Default, Supported, Fallbacks }` mirroring IR; `Resolve(tag)` walks the fallback graph with cycle detection; `NegotiateAcceptLanguage` implements RFC 4647 best/prefix matching; `Middleware` mounts the negotiation step on `http.Handler`; `WithLocale`/`LocaleFrom` context plumbing. `ErrLocaleNotSupported` typed error. ICU MessageFormat renderer + CLDR plural rules + Lokalise/Crowdin/Phrase TMS adapters stay in runtime/adapter packs. LSP hovers for `locale`/`supported`/`fallback`. Golden eval files `tests/golden/i18n/{locale_negotiate,translation_render}.jsonl`. See commit (next). |
| 54 | i18n bucket cycle — `translation` kind + doctor coverage | done | Typed IR for `Translation` / `TranslationKey` / `TranslationVariant` / `TranslationPluralArm` on `Feature`, `LocaleNegotiate` on `AppRuntimeUnit` + `Api`, `Rule.message_ref` reserved. Parser: `parse_translation_decl` + `parse_locale_negotiate_decl` + `parse_app_manifest` runtime-unit branch. CLI: `lazuli translate extract --check` walks the package and writes per-locale catalog stubs. `@translation` namespace registered in `is_allowed_reference_namespace`. 15 doctor diagnostics in `i18n_diagnostics`: `app_locale_default_unsupported`, `app_locale_fallback_unknown_source`, `app_locale_fallback_unknown_dest`, `app_locale_fallback_cycle`, `locale_negotiate_source_invalid`, `locale_negotiate_strategy_invalid`, `translation_catalog_path_missing`, `translation_locale_unsupported`, `translation_locale_missing_for_default`, `translation_locale_missing_for_supported`, `translation_key_unused`, `rule_message_ref_unresolved`, `notification_template_placeholder_unknown`, `cldr_plural_arm_invalid`. Eight LSP hovers + closed-catalog `LOCALE_NEGOTIATE_SOURCE_VALUES`/`_STRATEGY_VALUES`/`CLDR_PLURAL_ARM_VALUES`/`BCP47_POPULAR_TAGS`. tmLanguage extended. Fixture lift: `app.lzi` mounts `locale_negotiate`; `customer` feature carries a `translation` block with three keys; three rules migrated to `message @translation.<key>`; `examples/full-capsule/i18n/{customer.pt-BR,customer.en-US}.json` stubs committed. Five test fixtures + five inline tests. Surface labels in `.lzx` and ICU MessageFormat semantics stay pilot-gated per proposal §3.7 and §Speculative. |
| 55 | Notifications expanded bucket cycle — typed `digest` / `throttle` sub-blocks on `notification` (IR + AST + parser + lowering + inspect) | done | Two additive sub-blocks on the shipped Tier 3 `Notification` IR (`crates/lazuli_ir/src/lib.rs:2766`). `digest { every, group_by, max_size, template_strategy: merge\|append }` + `throttle { max_per, per_recipient, per_channel, burst }` — distinct keywords from scalar `rate_limit "N per <window>"` (preserved across `agent` / `auth password` / `command` / `expose http`) by design so a cold-reading LLM sees the contract axis (per-call vs per-recipient/channel) without doc lookup. Parser (`parse_notification_digest` + `parse_notification_throttle` at `crates/lazuli_syntax/src/parser.rs:5159` ~ `5253`) gates indent-6 children behind indent-4 sub-block headers. Analyzer (`crates/lazuli_analyzer/src/lib.rs:1539` ~ `1572`) lowers field-for-field with closed-catalog enum mapping for `template_strategy`. `Tier3FeatureSlice.notifications` lifted; `--expand=notifications` flag added so typed digest/throttle surface in inspect. Commit `068b470`. |
| 56 | Notifications expanded bucket cycle — six new doctor diagnostics + LSP hovers + tmLanguage | done | Six IR-driven cross-checks in `tier3_notification_diagnostics` (`crates/lazuli_cli/src/doctor.rs:2972`): `NOTIF-DIGEST-001` (group_by resolves against trigger event's payload union; cross-feature index built once from each feature's `events` + `event_groups` raw_payload), `NOTIF-DIGEST-002` (`every` matches `<N> (seconds\|minutes\|hours\|days)`), `NOTIF-DIGEST-003` (`max_size` ∈ 1..=10000), `NOTIF-THROTTLE-001` (same duration shape), `NOTIF-THROTTLE-002` (warning when `throttle` has no axis), `NOTIF-THROTTLE-003` (error when `burst > 0` without `per_recipient`). `Tier3FeatureFacts` gains additive `events: Vec<lazuli_ir::Event>` slot (no new fact family). Six inline doctor tests + ten LSP hovers (`digest`, `throttle`, `every`, `group_by`, `max_size`, `template_strategy`, `max_per`, `per_recipient`, `per_channel`, `burst`) + closed-catalog `NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES` completion + ten new tmLanguage keywords. The `throttle` hover explicitly distinguishes from scalar `rate_limit`. Commits `5a16171` + `34b14fc`. |
| 57 | Notifications expanded bucket cycle — Lazuli Go stubs + fixture lift | done (language side) | `runtime/go/lazuli/notifications/`: typed `NotificationDigest` / `NotificationThrottle` / `DigestStrategy` (`merge`\|`append`) on `NotificationContract` (`contract.go`). Three new typed errors: `ErrDigestFull`, `ErrThrottleExceeded`, `ErrInvalidDuration`. Two adapter contracts: `DigestStore { Add, Flush, PendingKeys }` (`digest_store.go`) with reference `MemoryDigestStore`; `ThrottleStore { Allow }` (`throttle_store.go`) with reference `MemoryThrottleStore` implementing fixed-window token-bucket and DSL-side duration parser. Production stores (Redis, Postgres) ship as `@adapter.notification.{digest,throttle}.<store>` packs and stay outside Lazuli core. Fixture lifted: `notification welcome_email` in `examples/full-capsule/full-capsule.lzi:849` now exercises both `digest` (every 15 minutes, group_by customer_id, max_size 50, template_strategy merge) and `throttle` (max_per 1 hour, per_recipient + per_channel + burst 3). `docs/invariants.md:174` lists `digest` / `throttle` alongside the existing optional children and notes `delivery_receipt` / `read_receipt` stay SPECULATIVE. |
| 58 | Hostpoint Phase Prep — Codegen Lazuli → Go design proposal | proposal landed (2026-05-11), grade 9.18/10 | `docs/proposals/codegen-lazuli-go.md` (780+ lines) covers CLI verb (mirroring `lazuli generate openapi`), handwritten printer architecture mirroring `crates/lazuli_openapi`, per-kind template mapping for all 13 Tier 4 IR shapes (Resource, Record, Command, Query.{List,Lookup,Sql}, Api, Auth, Job, Webhook, Notification, Storage, Translation, TenantMigration, EventGroup) with green/yellow/red readiness, gap catalog for Lazuli Go lib (`Command.approval/external_calls/timeout/retry/idempotency/deprecated`, `Api[I,O]`, expanded `WebhookContract`, `NotificationDigest/Throttle`, `i18n.Catalog`, `EventGroup` value), Go module layout (`dist/go/<feature>/<kind>.gen.go`, root `go.mod` pinning `lazuli.dev/runtime/lazuli`), plugin namespace resolver (§6.1 closed-catalog mapping `@runtime/`/`@plugin/` → import paths; `@plugin/<product>/<name>` form retired per 2026-05-11 policy), closed error-code catalog (`CODEGEN-GO-{PLUGIN-001,UNRESOLVED-002,ADAPTER-003,SEMANTIC-004,CAP-005,FN-006}` §6.2.1), smoke harness plan (`LAZULI_GO_SMOKE=1`, gofmt -l deterministic trigger §10.4), 15-cell budget (cells 1-6 green-slice MVP, 7-12 fan-out, 13-15 blocked on Lazuli Go lib gaps), Hostpoint §5 decisions materialisation (GeoPoint→postgis.Point + GiST index + ST_DWithin filter, PostGIS extension migration, Google Maps as `@plugin/google-maps` private repo, MercadoPago as `@plugin/mercadopago` private repo, Expo Push as `@plugin/expo-push` private repo). Architecture.md glossary updated to canonical `lazuli.dev/runtime/lazuli/<bucket>` module path. Implementation cells (I1→I4 + E1→E4 + G1→G7) kick after proposal commit. |
| 59 | Hostpoint Phase Prep — Lazuli Go wire of auth+storage+jobs buckets | done (2026-05-11) | Three commits land real wire across `runtime/go/lazuli/{auth,storage,jobs}/`, replacing the TODO stubs with library calls from already-mature Go libs (no reimplementation, ~345 LOC bodies total). `db3a0d1` auth: `golang.org/x/crypto/argon2.IDKey` PHC `$argon2id$v=19$m=65536,t=3,p=4$<salt>$<hash>` (OWASP params; `crypto/subtle.ConstantTimeCompare` verify) + bcrypt fallback for legacy migration via `golang.org/x/crypto/bcrypt`; `golang.org/x/oauth2` + `golang.org/x/oauth2/google` builder (`BuildOAuthConfig`) + `crypto/rand` state minting (`GenerateOAuthState`); `github.com/pquerna/otp/totp` enroll+verify (TOTP only in v0, `Issuer` defaults to `Lazuli`). `5422d4a` storage: `aws-sdk-go-v2/service/s3` Put/Get/Delete + `s3.NewPresignClient` with `PresignGetObject`/`PresignPutObject` + `WithPresignExpires`; NoSuchKey → `ErrFileNotFound`; new `IssueSignedUploadURL` + `PresignedURLWriter` extension. `2af6623` jobs: `github.com/riverqueue/river` + `riverdriver/riverpgxv5` dispatcher wrapping `RiverInserter` + `*river.Workers`; unified `LazuliRiverKind = "lazuli"` so all Lazuli jobs share one River Worker (router by `JobKind` inside `Work()`) — avoids per-kind codegen explosion at the cost of per-Kind retention/priority knobs. Inline `DispatchJob` path kept for timeout + retry budget semantics (`ErrJobTimeout` / `ErrJobMaxRetries`). 33 new test cases (11 auth + 11 storage + 11 jobs) all green. Signature drifts flagged by the wire agent + resolved post-merge: `VerifyMFA` gained `secret` arg (caller loads from identity resource — avoids pgx coupling in `auth/`); `bucket-auth-cycle.md` example updated. OAuth state-stash helpers (`StashOAuthState`/`LoadOAuthState`) exported so transports can thread state through ctx between Redirect/Callback requests via same-site cookies. Dependencies added to `runtime/go/go.mod`: `x/crypto v0.51.0`, `x/oauth2 v0.36.0`, `pquerna/otp v1.5.0`, `aws-sdk-go-v2 v1.41.7` + `service/s3 v1.101.0`, `riverqueue/river v0.37.0`. Go toolchain auto-bumped 1.24 → 1.25. **Out of scope** (Phase Prep follow-ups): pgx-backed Session adapter (`IssueSession`/`ResolveSession`/`InvalidateSession` still stubs — DB bucket); River `PeriodicJobs` registration for scheduled jobs (`EnqueueSchedule` is a no-op until adapter package owns Client construction). |
| 60 | Hostpoint Phase Prep — Codegen Lazuli → Go cells I1 + E1 (CLI verb + emitter scaffold) | done (2026-05-11) | `2ef36a1` (I1): `GenerateKind::Go` variant + four new flags on `Generate` subcommand (`--module`, `--lazuli-go-version`, `--check`, `--out` clap alias for `--output`); new `generate_go` function (~80 LOC) dispatches to `lazuli_codegen_go::generate_v1`; `default_module_name`/`to_kebab_case` helpers derive Go module from `app.name` (e.g. `app AcmeCRM` → `lazuli/acme-crm`); legacy `generate()` renamed `generate_legacy_demo()` for `lazuli compile` back-compat. `crates/lazuli_cli/src/main.rs` +135/-12, `crates/lazuli_codegen_go/src/lib.rs` +73/-2. `ddc4913` (E1): five new files under `crates/lazuli_codegen_go/src/emitter/` (~754 LOC + 26 inline tests + 4 integration tests). `printer.rs` — `GoPrinter` with line/blank/indent/dedent/banner/package/aligned_rows (column-aligned struct tags via widest-first algorithm); `imports.rs` — `ImportSet` with three-bucket classifier (stdlib / `lazuli.dev/runtime/lazuli/*` / third-party, sorted via `BTreeSet`, three blank-line-separated groups mirror goimports default); `types.rs` — `go_type_for(&TypeRef) -> (String, Option<&'static str>)` closed-catalog mapping for all built-ins + capabilities (TODO at `SemanticGeoPoint` since the IR variant doesn't exist yet — proposal §10.1 needs to add `BuiltinType::SemanticGeoPoint` before geo codegen lands); `module.rs` — `emit_module` walker drives per-feature stub emission + root `go.mod` (`module <kebab>`, `go 1.25`, `require lazuli.dev/runtime/lazuli v0.1.0`); `mod.rs` re-exports. `generate_v1` wires to `emitter::emit_module`. `crates/lazuli_codegen_go/tests/emit_v1.rs` builds minimal `Module` + verifies invariants (4 integration tests). Test count: `lazuli_codegen_go` 2 → 31. Smoke: `lazuli generate go examples/full-capsule --out dist/go --check` lists 6 files (1 go.mod + 5 feature stubs); write-mode produces them with correct banners + `package <feature>`. Directory-input resolves `app AcmeCRM` → `module lazuli/acme-crm`; single-file falls back to `module lazuli/app`. **Open follow-ups** (next cells E2-E4): kinds emission (Resource/Record, Command, Query) — fill the empty feature stubs. **Flagged** for a separate cell: `LAZURITE_GO_VERSION` source-of-truth — currently floats as a Rust const in `lazuli_codegen_go/src/lib.rs`, no canonical pin in `runtime/go/go.mod`. **Flagged** for IR cycle: add `BuiltinType::SemanticGeoPoint` (proposal §10.1 + Hostpoint §9.1 PostGIS materialisation). |
| 61 | Hostpoint Phase Prep — Codegen Lazuli → Go cell E2 (Resource + Record kind emission) | done (2026-05-11) | `677adc5` (E2): new `crates/lazuli_codegen_go/src/emitter/resource.rs` (~894 LOC + 19 inline tests) + 13 LOC of wire-up in `module.rs` + 51 LOC printer `aligned_struct_rows` 3-column helper + 27 LOC `types.rs` sanitiser upgrade for `UserDefined`/`EnumRef`. Emits `dist/go/<feature>/resource.gen.go` per feature containing every `ir::Resource` and `ir::Record` of that feature. Resource shape: typed Go struct with 3-column-aligned `db:"col" json:"col"` tags (pgx + `RowToStructByName[T]` compatible per `project_db_driver_choice` memory) + `var <name>Resource = lazuli.Resource[T]{ Name, Feature, Tenancy, SoftDelete, Retention }` value. Records emit struct only (no Resource[T] value — value-typed, no row identity). Special handling: optional fields → `*T` pointer + `json:"…,omitempty"`; `Resource.soft_delete: true` → `DeletedAt *lazuli.Time`; `Resource.timestamps: true` → `CreatedAt/UpdatedAt lazuli.Time`; `Field.derived_from` → renders as struct-level comment (column lives in DDL surface, not the struct). `@semantic.GeoPoint` field gets `db:"col,type:geography(point,4326)"` modifier + `cridenour/go-postgis` import (proposal §9.1 materialisation). Test count: `lazuli_codegen_go` 31 → 53 (+22). Smoke: `lazuli generate go examples/full-capsule --out dist/go` now writes 10 files (root `go.mod` + 5 `<feature>.gen.go` stubs + 5 `resource.gen.go` per-feature). `dccccea` follow-up: `@semantic.Money` analyzer arm (was missing — fell through to `_semantic_Money` placeholder); types.rs module doc updated to remove pre-GP-1 stale claim. **Necessary side-effect**: `lazuli_openapi::builtin_to_openapi` + `lazuli_cli::format_type_ref` non-exhaustive match patches (+5/+1 LOC) — GP-1 left these two callers behind; flagged in row 60 follow-ups, addressed here because they blocked the E2 build. **Lazuli Go lib gaps surfaced by smoke `go build ./...`** (closed in row 62): `lazuli.{Email,JSON,Money,Phone,URL,UUID,Date,Secret,HashedRef,EncryptedRef,TokenRef}` semantic/capability aliases absent in `runtime/go/lazuli/types.go`; `storage.FileRef` absent in `runtime/go/lazuli/storage/`; `TenancyTeam`/`TenancyCustom` absent (emitter fell back to `TenancyNone`). **IR/analyzer gap (still open)**: cross-feature `UserDefined(qname)` references ignore `qname.feature` — `customer_auth` references `Customer` from `customer` feature but the emitter treats it as same-package. Cell I3/I4 territory. **Next cells (proposal §8 order)**: E3 (Command/Create/Update/Delete), I2 (root `main.go` + `lazuli_app.gen.go`), then E4 (Query.{List,Lookup,Sql}). |
| 62 | Hostpoint Phase Prep — Lazuli Go lib type-alias backfill + go.mod require fix | done (2026-05-11) | Two commits close the Lazuli Go lib gaps surfaced by row 61's smoke run. `fa0cc65` (require fix): the emitter wrote `require lazuli.dev/runtime/lazuli <version>` in the generated `go.mod`, but the Lazuli Go module is `lazuli.dev/runtime` (verified `runtime/go/go.mod:1`) — `lazuli` is a subpackage, not a module. `go mod tidy` rejected the bad path with "unrecognized import path". Fixed: `require lazuli.dev/runtime <version>`; generated imports (`lazuli.dev/runtime/lazuli`, `lazuli.dev/runtime/lazuli/storage`) resolve correctly. `0e92397` (backfill): adds eight semantic value aliases (`Email`, `Phone`, `URL`, `UUID`, `Date`, `Currency`, `Money`, `JSON`) + four capability reference aliases (`Secret`, `HashedRef`, `EncryptedRef`, `TokenRef`) in `runtime/go/lazuli/types.go` as type aliases (not new named types) for pgx scan + json marshal triviality. `TenancyMode` const block widens to `TenancyTeam`/`TenancyCustom` (full IR catalog mirror; `TenancyCustom` is a structural marker — the per-resource custom axis name is not yet threaded through `Resource[T]`; the runtime widens this when a pilot product needs per-row custom-axis scoping). `storage.FileRef` lands in `runtime/go/lazuli/storage/contract.go` as the typed reference stored in resource rows for `@cap.File(...)` fields (carries `Key`/`ContentType`/`Size`; distinct from `FileContract` which is the language-side declaration). Emitter's `tenancy_const` in `crates/lazuli_codegen_go/src/emitter/resource.rs:453` updated to emit each constant directly instead of the previous Org-or-None collapse. Smoke (`lazuli generate go examples/full-capsule --out <dir>` + `go mod tidy` with a local `replace lazuli.dev/runtime => runtime/go` directive) now resolves the lazuli.dev/runtime imports cleanly. Remaining compile failures are the cross-feature `UserDefined(qname)` gap (qname.feature ignored — still row 61 follow-up) and missing Enum emission (Cell E2.5 / E3 territory). `runtime/go/lazuli/{auth,storage,jobs}` tests stay green (additive types). |

## Registry Decision Pressure

The open question is whether a root `registry.lzi` should be a native Lazuli/
the Lazuli runtime package artifact or just an arbitrary file imported by `app.lzi`.

### Option A: Native `registry.lzi` Convention

`registry.lzi` lives next to `app.lzi` and is discovered by the package loader.

Pros:

- Keeps `app.lzi` thin without adding import noise.
- Fits Lazuli's opinionated, token-efficient style.
- Gives the Lazuli runtime and `lazuli doctor` a stable place for global env, capabilities,
  integration registry, adapter bindings, and pack registry.
- Avoids top-of-file import boilerplate in every `.lzi` and `.lzx`.

Cons:

- Introduces filename convention as semantics.
- Needs clear package root rules in monorepos.
- Needs an escape hatch for non-standard layouts.

### Option B: Explicit Imports Everywhere

Every source file imports what it needs.

Pros:

- Fully explicit dependency graph.
- Easy to understand with no package-level conventions.
- Flexible for unusual layouts.

Cons:

- Pollutes files with boilerplate and harms token economy.
- Pushes Lazuli toward a general module language instead of an opinionated
  contract language.
- Makes reusable feature files visually noisier and easier for agents to edit
  inconsistently.

### Option C: Hybrid Package Convention

Use native package discovery for conventional files and allow explicit imports
only as an override.

Recommended default:

```text
app.lzi
registry.lzi
features/*.lzi
experiences/*.lzx
profiles/*.lzi
```

Rules:

- `app.lzi` is the composition root.
- `registry.lzi` is a package-level catalog of capabilities, env groups,
  integrations, packs, adapters, and other global bindings.
- Feature files do not import provider registries. They declare abstract
  requirements such as `requires integration gateway: PaymentGateway`.
- `app.lzi` or `registry.lzi` binds abstract requirements to concrete registry
  entries.
- Explicit `import` may exist later for non-standard package layouts, generated
  libraries, or monorepo cross-package dependencies, but it should not be the
  default authoring style.

Decision: **Option C**. It preserves opinionated defaults and token economy
while still leaving room for a deterministic future escape hatch.

## Workspace Decision Pressure

`workspace.lzi` is the optional semantic coordination artifact for real
multi-app pressure.

Intended split:

- `workspace.lzi`: semantic contract for a distributed system or monorepo,
  including apps, external contracts, shared registry, app graph, event edges,
  and gateway contracts.
- `lazuli.toml`: operational the Lazuli runtime config such as remote repo URLs, branches,
  provider ids, CI wiring, deploy providers, local ports, adapter provider
  choices, and other concrete mechanics.

It models distributed contract shape, not repository automation.

Expected ownership model:

- A small or medium app has one `app.lzi` and one package-level `registry.lzi`.
- A monorepo with multiple deployable apps may have one `app.lzi` /
  `registry.lzi` pair per app package.
- A distributed system spanning multiple repos may add a root `workspace.lzi`
  that references apps, external services, sidecars, shared registries, event
  edges, and public ingress/gateway contracts.
- `lazuli.toml` remains operational glue: repo URLs, branches,
  provider ids, CI/deploy wiring, and concrete mechanics.

Do not make `workspace.lzi` mandatory for normal apps. It is a semantic
coordination artifact for distributed systems, not a replacement for `app.lzi`.

Naming decision:

- Use `workspace.lzi` for the semantic distributed-system contract.
- Use `lazuli.toml` for the Lazuli runtime's operational/tooling configuration.
- Avoid `the Lazuli runtime-workspace.toml` as the default name because it competes with
  `workspace.lzi` and makes the source of truth less obvious.

Polyglot contract rule:

Lazuli does not require every service to be implemented with Lazuli or the Lazuli runtime.
It requires every service participating in the workspace graph to have a
contract.

Examples:

```lazuli
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"

  shared_registry "./registry.lzi"

  boundaries
    crm publishes customer.*
    ai consumes customer.*

  communication
    propagate actor, tenant, trace_id, request_id
    default sync internal rpc
    default async event_bus

  gateway public_api
    route "/api/customers/*" to app crm
      auth propagate
      tenant propagate
```

The `ai` service might be Python/FastAPI, Java, Node, Rust, or another stack.
Lazuli owns the API/event/schema contract and context propagation guarantees.
the runtime materializes that contract mostly as Go runtime wiring: typed HTTP/RPC
clients, event publishers/consumers, webhook receivers, mocks, gateway config,
and contract tests. Adapters own HTTP, gRPC/Connect, Kafka, NATS, RabbitMQ,
SQS, Pub/Sub, Envoy, Kubernetes ingress, and other concrete transports.

SDK exports for Python/TypeScript/etc. are optional contract-publication
artifacts for external teams or partners. They are not the central runtime
model for Lazuli apps.

Contract inputs now include:

- Lazuli-authored `contract.lzi`.
- OpenAPI for HTTP APIs.
- AsyncAPI for broker/event contracts.
- Proto/Buf for RPC contracts.
- JSON Schema or Avro when an enterprise broker/schema registry requires it.

Canonical authoring:

```lazuli
contract acme.ai.v1
  purpose "AI inference service."
  compatibility backward
  import openapi "./contracts/ai.openapi.json"

  record CustomerSummaryRequest
    customer_id: ID required
    email: @semantic.Email @pii.contact optional

  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    auth service
    timeout "10s"

  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
      summary: Text required
```

## Adapter And Container Decision

`registry.lzi` is the native language-level catalog. It may contain bindings to
adapters supplied by the Lazuli runtime, third-party plugins, or local app code.

Canonical model:

```lazuli
registry
  integrations
    crm: CRMProvider
      adapter @adapter.crm

    payments: PaymentGateway
      adapter @runtime/mercadopago

    bureau: CreditBureau
      adapter @plugin/acme/serasa

    ai: AiInference
      adapter "./integrations/ai.go"
```

Allowed adapter sources:

- `@runtime/<adapter>` for first-party (`@runtime/...`) adapters.
- `@plugin/<publisher>/<adapter>` for third-party plugin adapters.
- `@adapter.<name>` for local adapter extension references.
- Local paths for app-owned adapters.

`lazuli inspect` exposes `adapter_provenance` as `the Lazuli runtime`, `plugin`, or
`local`. `lazuli doctor` rejects unknown source shapes.

Do not add a `container.lzi` yet.

Reason:

- Dependency inversion belongs in the language contract: features require
  abstract integrations/capabilities, and app/registry bindings choose concrete
  implementations.
- Dependency injection mechanics belong in the Lazuli runtime: construction order,
  lifetimes, logger/database/client instances, test doubles, and runtime
  wiring.
- Provider details belong in adapters/config: HTTP endpoints, optional provider
  SDK setup inside Go adapters, connection pools, logger sinks, database driver
  settings, and cloud ids.

If real adapters need static checks that cannot be expressed through
`registry.lzi`, promote the missing part as a small registry primitive before
creating a broad container language.

## Gateway And Proxy Decision Pressure

Distributed apps will need a way to model ingress and cross-service edges, but
the language should avoid becoming Envoy/Kubernetes config.

Likely split:

- Lazuli language: `gateway` or `proxy` contract for public ingress, route
  ownership, auth propagation, tenant propagation, timeout/retry policy, and
  service exposure.
- the runtime: generated gateways, service clients, local dev routing,
  request context propagation, and reverse proxy/runtime wiring.
- Adapters: Envoy, Kubernetes ingress, Cloud Run, Fly proxy, service mesh,
  gRPC/Connect transport, and provider-specific routing.

Keep the word `proxy` under consideration, but prefer a higher-level language
term such as `gateway` if the construct represents application ingress rather
than raw proxy mechanics.

## Guardrails

- Do not put concrete providers such as MercadoPago, Serasa, Stripe, AWS, or
  Kubernetes into core syntax.
- Do not make every `.lzi` file repeat imports for common package context.
- Do not let `app.lzi` become an implementation file. It composes the app.
- Do not let `registry.lzi` become a provider operation schema. It catalogs
  what exists and how global bindings resolve.
- Do not introduce `container.lzi` as a runtime DI config unless registry
  contracts fail under real adapter/plugin pressure.
- `workspace.lzi` and provider-neutral `gateway` are now implemented. Keep raw
  `proxy`, sidecar, service mesh, and provider routing mechanics in
  runtime/adapters unless future static-analysis pressure justifies a language
  primitive.
- Any magic package discovery must be visible in `lazuli inspect`, `doctor`, and
  LSP diagnostics so it does not become hidden runtime behavior.
