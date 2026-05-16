# Lazuli LSP-vs-Doctor Diagnostic Gap Audit

Date: 2026-05-15
Branch: `main`
Scope: catalog every diagnostic emitted by `lazuli doctor` (CLI) vs the LSP file-local diagnostics (VS Code squiggles), classify the gap, and rank the doctor-only checks that are cheap to port.

This is a **read-only audit**. No source files were modified.

## Method

- Doctor codes were collected from `crates/lazuli_cli/src/doctor.rs` and every file under `crates/lazuli_cli/src/doctor/`. Patterns walked:
  - `code: "X".to_owned()` inside `DoctorDiagnostic { … }`
  - `pub const CODE: &'static str = "X"` (and `INPUT_SHAPE_CODE` / `REQUIRE_MULTI_CODE` siblings used by `lzx/bulk_actions.rs`)
  - First string-literal arg to helper emitters (`app_missing_contract_diagnostic(app, "X", msg)`)
- Test-region matches (`#[test]` / `mod tests`) were filtered out heuristically.
- LSP codes were collected from `crates/lazuli_lsp/src/lib.rs` by extracting the 4th argument (`code: &str`) of every `simple_canonical_diagnostic(...)` call.

## Summary

- **Doctor unique codes: ~292** (215 in `doctor.rs` + 77 in submodules). Severity skew: ~70% `Error`, ~25% `Warning`, ~5% `Info/Hint`.
- **LSP unique codes: 147** across 300 `simple_canonical_diagnostic` call-sites in `crates/lazuli_lsp/src/lib.rs`.
- **Exact code-name overlap: 3** (`approval_contract_diagnostics`, `eval_nondeterministic_warning`, `event_trace_reserved_name_diagnostics`).
- **Semantic-normalized overlap: 5** (adds `CONTRACT-EVENT-001 ↔ contract-event`, `headers-contract ↔ headers_contract_diagnostics`).
- **Net doctor-only after semantic match: ~287** of 292.

The two diagnostic catalogs are essentially **disjoint**. They are not parallel implementations of the same checks; they target different surfaces:

| Layer | What it owns | Vocabulary style |
|---|---|---|
| LSP | Per-line **shape / vocabulary / category-membership** checks on the open buffer. | Category codes (`app-runtime-contract`, `cache-contract`, `headers_contract_diagnostics`) — broad "contract violated here" banners. |
| Doctor | Whole-project **consistency / correctness / capability / lifecycle** checks. | Point codes (`APP-RUNTIME-001`, `LIFECYCLE-NO-INITIAL-STATE`, `HOOK-TARGET-001`, `VOCAB-CAP-MISSING-001`) — specific rules with rationale. |

So the user pain is real: a `.lzi` can be syntactically + shape-clean (LSP green) and still produce a doctor error stack. The fix is not "rename codes to match" — it is to **port the file-local doctor rules into LSP**, keeping the LSP category code separate from the doctor's catalog code (or letting LSP emit the doctor code directly).

## Portability classification

Every doctor-only code was classified into one of three buckets by reading its source file (especially the public `check` signature):

| Bucket | Signature pattern | LSP can fire? |
|---|---|---|
| **File-local-portable** | `check(feature: &Feature, path: &Path) -> Vec<Finding>` — pure function over a single lowered feature. | Yes, cheaply. LSP already calls `lazuli_analyzer::lower_document` for parse-error mode; lift it for the canonical path. |
| **Multi-file but local-OK** | Needs `app.lzi` + `feature.lzi` + `registry.lzi` together — but a partial check (e.g., "this app block has bare jobs and no `runtime` child") catches the same typo. | Yes, degraded but useful. |
| **Cross-file inherent** | Needs the full `(manifest, profiles, registry, all features, plugin lockfile, …)` to even attempt — e.g., cross-feature type resolution, plugin namespace lookup. | No. Out of scope for a per-file LSP without a project indexer. |

## Top priority — port these to LSP next (file-local-portable, ERROR severity)

These are the highest-ROI ports. All follow the `check(feature: &Feature, path: &Path) -> Vec<Finding>` pattern and live in dedicated submodules, so the wire-up is a one-call extension to `diagnostics_for_with_profile` in `crates/lazuli_lsp/src/lib.rs:413`.

| Doctor code | Severity | Source | Description |
|---|---|---|---|
| `HOOK-TARGET-001` | Error | `crates/lazuli_cli/src/doctor/correctness/hook_target_001.rs` | `hook x: Hook[Foo]` references `Foo`, but no command/query/job/record/event/resource of that name exists **in the same feature**. Classic dangling-reference typo — currently invisible in editor. |
| `COMMAND-INPUT-SHADOWS-FIELD-001` | Error | `crates/lazuli_cli/src/doctor/correctness/command_input_shadows_field_001.rs` | `command.input` shadows a resource field with a different `TypeRef` — handler silently narrows/widens. |
| `COMPOSITE-KEY-CONTRACT-001` | Error | `crates/lazuli_cli/src/doctor/correctness/composite_key_contract_001.rs` | Composite-key declaration violates shape rules. |
| `CHANNEL-PAYLOAD-001` | Error | `crates/lazuli_cli/src/doctor/correctness/channel_payload_unresolved_001.rs` | Channel payload type unresolved (within feature). |
| `RESOURCE-LOCK-CONTRACT-001` | Error | `crates/lazuli_cli/src/doctor/correctness/resource_lock_contract_001.rs` | `lock` block shape violation. |
| `FULL-TEXT-TYPE-001` | Error | `crates/lazuli_cli/src/doctor/correctness/full_text_type_001.rs` | `full_text` index on non-text field. |
| `LIFECYCLE-NO-INITIAL-STATE` | Error | `crates/lazuli_cli/src/doctor/lifecycle/no_initial_state.rs` | Lifecycle declares states but none marked `initial`. |
| `LIFECYCLE-STATE-DUPLICATE` | Error | `crates/lazuli_cli/src/doctor/lifecycle/state_duplicate.rs` | Two states with same name. |
| `LIFECYCLE-ENUM-DUPLICATE` | Error | `crates/lazuli_cli/src/doctor/lifecycle/enum_duplicate.rs` | Generated lifecycle enum collides. |
| `LIFECYCLE-FIELD-DOUBLE-DECLARED` | Error | `crates/lazuli_cli/src/doctor/lifecycle/field_double_declared.rs` | Discriminator field declared twice. |
| `LIFECYCLE-INVARIANT-PARAM-UNRESOLVED` | Error | `crates/lazuli_cli/src/doctor/lifecycle/invariant_param_unresolved.rs` | `invariant` param doesn't resolve to a state/transition. |
| `LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION` | Error | `crates/lazuli_cli/src/doctor/lifecycle/terminal_has_outgoing.rs` | `terminal` state declared with outgoing transition. |
| `LIFECYCLE-TIMESTAMP-TYPE` | Error | `crates/lazuli_cli/src/doctor/lifecycle/timestamp_type.rs` | Lifecycle timestamp field has wrong type. |
| `LIFECYCLE-TRANSITION-FROM-UNDECLARED` | Error | `crates/lazuli_cli/src/doctor/lifecycle/transition_from_undeclared.rs` | `from:` references state not in lifecycle. |
| `LIFECYCLE-TRANSITION-TO-UNDECLARED` | Error | `crates/lazuli_cli/src/doctor/lifecycle/transition_to_undeclared.rs` | `to:` references state not in lifecycle. |
| `LIFECYCLE-UNREACHABLE-STATE` | Error | `crates/lazuli_cli/src/doctor/lifecycle/unreachable_state.rs` | State has no incoming transition. |
| `AGGREGATE-CONTAINS-UNKNOWN` | Error | `crates/lazuli_cli/src/doctor/domain/aggregate_contains_unknown.rs` | `aggregate ... contains X` where `X` undeclared in feature. |
| `AGGREGATE-ROOT-UNKNOWN` | Error | `crates/lazuli_cli/src/doctor/domain/aggregate_root_unknown.rs` | `aggregate root` references undeclared resource. |
| `INVARIANT-PREDICATE-INVALID` | Error | `crates/lazuli_cli/src/doctor/domain/invariant_predicate_invalid.rs` | Invariant predicate shape violation. |
| `VOCAB-CAP-MISSING-001` | Error (strict) | `crates/lazuli_cli/src/doctor/vocab/vocab_cap_missing_001.rs` | Field with `@pii.<class>` but no `@cap.Hashed/Encrypted/Token`. **High frequency** in real apps. |
| `VOCAB-UNION-001` / `VOCAB-UNION-002` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_union_001.rs`, `vocab_union_002.rs` | Discriminated-union shape violations. |
| `VOCAB-LIFECYCLE-001` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_lifecycle_001.rs` | Status-enum reimplements what `lifecycle` declares declaratively. Textbook in 4 features per memory `project_product_vocab_audits_2026-05-14.md`. |
| `VOCAB-EVENT-PAYLOAD-001` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_event_payload_001.rs` | Domain event shape uses anonymous record instead of named payload. |
| `VOCAB-EVENT-PRODUCER-001` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_event_producer_001.rs` | Event has no `emit` producer in feature. |
| `VOCAB-EVENT-ORPHAN-001` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_event_orphan_001.rs` | Event declared but never emitted/consumed within feature. |
| `VOCAB-DERIVED-READ-001` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_derived_read_001.rs` | Read-only derived field written via command. |
| `VOCAB-JSON-TYPED-001` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_json_typed_001.rs` | `JSON` used where a typed record would suffice. |
| `VOCAB-GRAMMAR-FORM-001` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_grammar_form_001.rs` | Wrong grammatical form for kind (e.g., `kind` keyword vs decorator). |
| `VOCAB-AUDIT-001` / `VOCAB-AUDIT-002` | Error | `crates/lazuli_cli/src/doctor/vocab/vocab_audit_001.rs`, `vocab_audit_002.rs` | Audit-trail vocabulary drift. |
| `ENC-E2EE-EVENT-001` | Error | `crates/lazuli_cli/src/doctor/encryption/e2ee_event.rs` | E2EE-flagged event has plaintext field. |
| `ENC-KEY-MISSING-001` | Error | `crates/lazuli_cli/src/doctor/encryption/key_missing.rs` | `@cap.Encrypted` field references a key not declared in the feature/registry crypto block. |
| `ENC-ROTATION-001` | Error | `crates/lazuli_cli/src/doctor/encryption/rotation.rs` | Rotation schedule shape. |
| `ENC-SOURCE-ENV-001` | Error | `crates/lazuli_cli/src/doctor/encryption/source_env.rs` | Source-env shape on crypto key. |
| `ENC-TEMPLATE-AXIS-001` | Error | `crates/lazuli_cli/src/doctor/encryption/template_axis.rs` | Template axis crypto shape. |
| `ENC-TENANCY-001` | Error | `crates/lazuli_cli/src/doctor/encryption/tenancy.rs` | Tenancy axis mismatch on crypto. |
| `POLLER-CURSOR-MISSING-001` | Error | `crates/lazuli_cli/src/doctor/poller/cursor_missing_001.rs` | Poller declares cursor mode but no `cursor` field. |
| `POLLER-CURSOR-FIELD-TYPE-001` | Error | `crates/lazuli_cli/src/doctor/poller/cursor_field_type_001.rs` | Cursor field wrong type. |
| `POLLER-DUAL-SCHEDULER-001` | Error | `crates/lazuli_cli/src/doctor/poller/dual_scheduler_001.rs` | Two scheduler blocks declared. |
| `POLLER-IDEMPOTENCY-ATTEMPTS-MISSING-001` | Error | `crates/lazuli_cli/src/doctor/poller/idempotency_attempts_001.rs` | Poller missing required attempts/idempotency wiring. |
| `POLLER-NO-TERMINAL-001` | Error | `crates/lazuli_cli/src/doctor/poller/no_terminal_001.rs` | Poller has no terminal state. |
| `POLLER-TERMINAL-FIELD-ENUM-001` | Error | `crates/lazuli_cli/src/doctor/poller/terminal_field_enum_001.rs` | Terminal field is not an enum. |
| `POLLER-TERMINAL-NO-EMIT-001` | Error | `crates/lazuli_cli/src/doctor/poller/terminal_no_emit_001.rs` | Terminal state has no `emits`. |
| `POLLER-QUIRK-CATALOG-MISMATCH-001` | Error | `crates/lazuli_cli/src/doctor/poller/quirk_catalog_001.rs` | Poller quirk not in catalog. |
| `POLLER-MAX-RETRIES-UNBOUNDED-001` | Error | `crates/lazuli_cli/src/doctor/poller/max_retries_unbounded_001.rs` | `max_retries` declared without cap. |
| `REPORT-COLUMN-MISMATCH-001` | Error | `crates/lazuli_cli/src/doctor/report/report_column_mismatch_001.rs` | Report column doesn't match source resource fields. |
| `REPORT-COLUMNS-EMPTY-001` | Error | `crates/lazuli_cli/src/doctor/report/report_columns_empty_001.rs` | Report declares no columns. |
| `REPORT-FILENAME-TOKEN-UNKNOWN-001` | Error | `crates/lazuli_cli/src/doctor/report/report_filename_token_unknown_001.rs` | `{token}` not in known catalog. |
| `REPORT-FORMAT-UNKNOWN-001` | Error | `crates/lazuli_cli/src/doctor/report/report_format_unknown_001.rs` | Unknown report format. |
| `REPORT-SIGNED-NO-STORAGE-001` | Error | `crates/lazuli_cli/src/doctor/report/report_signed_no_storage_001.rs` | Signed report missing storage block. |
| `REPORT-SIGNED-TTL-FORBIDDEN-001` | Error | `crates/lazuli_cli/src/doctor/report/report_signed_ttl_forbidden_001.rs` | TTL forbidden for this signed config. |
| `REPORT-SIGNED-TTL-MISSING-001` | Error | `crates/lazuli_cli/src/doctor/report/report_signed_ttl_missing_001.rs` | Signed-URL report missing TTL. |
| `REPORT-SOURCE-KIND-001` | Error | `crates/lazuli_cli/src/doctor/report/report_source_kind_001.rs` | Wrong `source.kind`. |
| `REPORT-STORAGE-AMBIGUOUS-001` | Error | `crates/lazuli_cli/src/doctor/report/report_storage_ambiguous_001.rs` | Storage tier ambiguous. |
| `lzx-bulk-action-input-shape` | Error | `crates/lazuli_cli/src/doctor/lzx/bulk_actions.rs` | Bulk action input shape. |
| `lzx-bulk-actions-require-multi` | Error | same | Bulk action requires `selection multi`. |
| `lzx-cell-slot-orphan` | Error | `crates/lazuli_cli/src/doctor/lzx/cell_slot_orphan.rs` | Cell `slot=X` not declared in parent. |
| `lzx-cells-mixed-form` | Error | `crates/lazuli_cli/src/doctor/lzx/cells_mixed_form.rs` | Mixed compact + block cells. |
| `lzx-list-cells-or-columns` | Error | `crates/lazuli_cli/src/doctor/lzx/cells_or_columns.rs` | List has both `cells` and `columns`. |
| `lzx-command-input-mismatch` | Error | `crates/lazuli_cli/src/doctor/lzx/command_input_mismatch.rs` | LZX action input shape mismatches command. |
| `lzx-drawer-source-shape` | Error | `crates/lazuli_cli/src/doctor/lzx/drawer_source.rs` | Drawer source shape. |
| `lzx-filter-type-resolves` | Error | `crates/lazuli_cli/src/doctor/lzx/filter_resolves.rs` | Filter type can't be resolved against source. |
| `lzx-route-param-missing-binding` | Error | `crates/lazuli_cli/src/doctor/lzx/route_param_missing_binding.rs` | Route param has no binding inside the route. |
| `lzx-route-param-orphan` | Error | `crates/lazuli_cli/src/doctor/lzx/route_param_orphan.rs` | Binding inside route references a param not in URL. |
| `lzx-sort-source-accepts` | Error | `crates/lazuli_cli/src/doctor/lzx/sort_source.rs` | Sort source not accepted by parent. |
| `lzx-source-resource-mismatch` | Error | `crates/lazuli_cli/src/doctor/lzx/source_resource_mismatch.rs` | LZX `source` doesn't match a known resource. |

(46 codes — file-local error-severity ports.)

## Medium priority — port these next (file-local-portable, WARNING severity)

| Doctor code | Severity | Source | Description |
|---|---|---|---|
| `SLUG-UNIQUENESS-IMPLICIT` | Warning | `crates/lazuli_cli/src/doctor/domain/slug_uniqueness_implicit.rs` | `@semantic.Slug` without explicit `unique` constraint. |
| `design-token-undefined` | Warning | `crates/lazuli_cli/src/doctor/design/token_undefined.rs` | Token reference doesn't resolve in this file's design block. |
| `design-token-unused` | Warning | `crates/lazuli_cli/src/doctor/design/hygiene.rs` | Declared token never referenced (file-local in `design.lzi`). |
| `design-token-duplicate-value` | Warning | same | Two tokens with same value. |
| `design-token-missing-dark` | Warning | same | Token has light but no dark mode. |
| `design-token-hex-leak` | Warning | `crates/lazuli_cli/src/doctor/design/hex_leak.rs` | Raw `#hex` in `.lzx` or design block; use token. |
| `design-token-px-leak` | Warning | `crates/lazuli_cli/src/doctor/design/px_leak.rs` | Raw `Npx` in `.lzx`. |
| `design-token-shadow-leak` | Warning | `crates/lazuli_cli/src/doctor/design/shadow_leak.rs` | Inline shadow spec. |
| `design-token-fontfamily-leak` | Warning | `crates/lazuli_cli/src/doctor/design/fontfamily_leak.rs` | Raw font-family string. |
| `auth_session_ttl_too_short` | Warning | `crates/lazuli_cli/src/doctor.rs:10130` | Session TTL < 1h. |
| `auth_oauth_no_password_alt` | Warning | `crates/lazuli_cli/src/doctor.rs:10091` | OAuth-only without password fallback. |
| `command_without_audit_hint` | Hint | `crates/lazuli_cli/src/doctor.rs:10698` | Command lacks audit declaration. |
| `resource_without_policy_hint` | Hint | `crates/lazuli_cli/src/doctor.rs:10745` | Resource has no policy attached. |
| `deprecated-no-replacement` | Warning | `crates/lazuli_cli/src/doctor.rs:11709` | `@deprecated` without `replacement:` arg. |
| `deprecated-sunset-past` | Warning | `crates/lazuli_cli/src/doctor.rs:11805` | Sunset date is in the past. |
| `deprecated_sunset_date_invalid` | Warning | `crates/lazuli_cli/src/doctor.rs:11795` | Sunset date unparseable. |
| `webhook-event-payload-empty` | Warning | `crates/lazuli_cli/src/doctor.rs:4202` | Webhook event payload is empty record. |
| `webhook-event-version-decreasing` | Warning | `crates/lazuli_cli/src/doctor.rs:4187` | Webhook event version regressed. |
| `webhook-event-deprecated-no-replacement` | Warning | `crates/lazuli_cli/src/doctor.rs:4216` | Webhook event deprecated, no replacement. |
| `JOB-TIMEOUT-001` | Warning | `crates/lazuli_cli/src/doctor.rs:4250` | Job has no timeout. |
| `JOB-FANOUT-001` / `JOB-FANOUT-002` | Warning | `crates/lazuli_cli/src/doctor.rs:4266`/`4286` | Fanout job shape warnings. |
| `OBSERVABILITY-SOURCE-001` | Warning | `crates/lazuli_cli/src/doctor.rs:8856` | Trace source not in catalog. |
| `OBSERVABILITY-PANIC-001` | Warning | `crates/lazuli_cli/src/doctor.rs:8887` | Missing panic logging hook. |
| `cap_file_visibility_undeclared` | Warning | `crates/lazuli_cli/src/doctor.rs:11409` | `@cap.File` field with no `visibility`. |
| `cap_file_visibility_signed_ttl_mismatch` | Warning | `crates/lazuli_cli/src/doctor.rs:11430` | Signed-URL visibility with bad TTL. |
| `cap_file_mime_family_unknown` | Warning | `crates/lazuli_cli/src/doctor.rs:11459` | MIME family not in catalog. |
| `cap_file_size_unit_invalid` | Warning | `crates/lazuli_cli/src/doctor.rs:11491` | Bad size unit. |
| `cap_file_accept_input_output_mismatch` | Warning | `crates/lazuli_cli/src/doctor.rs:11533` | Accept-in/out mismatch. |

(~28 codes.)

## Multi-file but local-OK candidates

These need cross-file context to be authoritative, but a best-effort local check catches the typo / misshape on the open buffer. The doctor full check should still run in CI; the LSP gives the developer instant feedback when only that buffer is open.

| Doctor code | Why multi-file | Local-OK strategy |
|---|---|---|
| `APP-RUNTIME-001…004` | Needs full feature-set + app manifest. | LSP can lint the `runtime` child block shape against the visible `services` / `bindings` declarations in the same `app.lzi`. (Already partially done with `app-runtime-contract`.) |
| `APP-TARGET-001/002` | Needs feature `surfaces`/`routes`. | LSP can flag `targets: [...]` that doesn't include `web` when the same buffer has no `web` URL declared. |
| `APP-URL-001/002` | Needs feature `routes`/`apis`/`webhooks`. | LSP can flag URL shape and missing protocols. |
| `APP-CAP-001` | Needs feature usage of `@cap.File`. | LSP can flag `capabilities:` block shape and warn when `storage` is absent (best-effort). |
| `PROFILE-001` / `PROFILE-URL-001` / `PROFILE-INT-001/002` / `PROFILE-BIND-001…004` / `PROFILE-APP-001` | Needs `app.lzi` + `registry.lzi`. | LSP can lint profile block shape within the open `profiles.lzi` (already partial via `profile-*-contract` category codes). |
| `WS-001` / `WS-APP-001/002` / `WS-BOUNDARY-001` / `WS-EVENT-001` / `WS-COMM-001` / `WS-CONTRACT-001` / `WS-GW-001…004` | Needs `workspace.lzi` + app graph. | LSP `workspace-*-contract` already covers the shape; surface block hierarchy still has gaps (e.g., gateway op without route binding). |
| `INT-CALL-001…004` | Needs `registry.lzi` integration catalog. | LSP can lint shape of `@integration.X.call(...)` references against a heuristic naming check. |
| `NOTIF-CHANNEL-001` / `NOTIF-DIGEST-001…003` / `NOTIF-THROTTLE-001…003` | Needs feature notifications + registry channel catalog. | File-local: validate `notifications.X` block shape. |
| `WEBHOOK-EVENT-001` / `WEBHOOK-PAYLOAD-001/002` / `WEBHOOK-REPLAY-001/002` / `WEBHOOK-DLQ-001…003` / `WEBHOOK-SCOPE-001` | Need cross-file. | File-local: shape of `webhook X { … }` block within feature. |
| `RBAC-ROLE-UNDECLARED-001` / `RBAC-CATALOG-MISSING-001` / `RBAC-MISSING-POLICY-001` | Need RBAC catalog from registry. | File-local: flag `policy { role: X }` if no `roles { X }` declared in the same file. |
| `tenant-migration-*` | Need cross-feature tenancy axis. | File-local: validate `tenant_migration` block shape. |
| `EVENTGROUP-NESTING-001` / `EVENTGROUP-PREFIX-001` | Already file-local (within feature). | Promote to file-local-portable bucket above. |
| `error-page-contract` / `error-page-duplicate` / `error-page-template-missing` | Need registry + filesystem. | File-local: validate `error_pages` shape; flag duplicate codes in same buffer. |
| `secret-rotation-overlap-contract` / `secret-rotation-binding-unknown` | Need registry. | File-local: rotation block shape (partial — `secret_rotation_contract_diagnostics` already covers). |
| `cache-profile-unknown` / `cache-tag-unknown` / `cache-ttl-contract` / `cache_capability_undeclared` / `cache_invalidates_target_unresolved` / `cache_namespace_collision` / `cache_ttl_unit_invalid` | Need cross-feature cache catalog. | File-local: validate cache block shape within feature; LSP `cache-contract` partial. |
| `MIGRATION-RECIPE-001/002` / `MIGRATION-STRATEGY-CONFLICT-001` | Need migration history files. | Skip — too cross-file. |

## Cross-file inherent — skip these for LSP

| Doctor code | Why cross-file | Note |
|---|---|---|
| `cross_feature_type_unresolved` | Walks ALL features to build the declared-type set. | The classic "needs project indexer". |
| `feature_uses_missing` | Compares feature's actual deps against declared `uses`. | Needs all features parsed. |
| `MANIFEST-REQUIRED-001` | Filesystem check (`lazurite.toml` exists?). | Filesystem-only. |
| `LAZULI-VERSION-001/002` | Reads workspace toml + crate version. | Toolchain consistency. |
| `SUBMODULE-DRIFT-001` | Reads `git submodule` state. | OS-level. |
| `PLUGIN-NOT-DECLARED-001` / `PLUGIN-UNUSED-001` / `PLUGIN-NAMESPACE-MISMATCH-001` | Compares registry plugins against feature usage. | Cross-file. |
| `FRONTEND-AUDIENCE-UNKNOWN-001` / `FRONTEND-OUT-COLLISION-001` / `AUDIENCE-NO-FRONTEND-001` | Manifest + features. | Cross-file. |
| `CODEGEN-WRAP-001` | Scans `runtime/go/lazuli/<bucket>/*.go` for `&lazuli.FieldError{}`. | Filesystem + Go source. |
| `PATTERN-DRAFT-STALE-001` | Walks `crates/lazuli_codegen_go/src/emitter/patterns.rs` + git mtime. | Toolchain-only. |
| `AUTH-SESSION-CALLSITE-001` | Walks Go handler source. | Cross-language. |
| `DEPLOY-STRATEGY-001` / `DEPLOY-CHECKPOINT-001/002` | Need workspace manifest. | Cross-file. |
| `webhook-event-*` cross checks (replacement, deprecation chain) | Need version log across commits. | Cross-time. |
| `tenant-migration-target-unknown` / `tenant-migration-handler-missing` | Need cross-feature tenancy graph. | Cross-file. |
| `cors_origin_undocumented_diagnostics` / `cors_unknown_environment_diagnostics` | Need full env catalog. | Cross-file (but the shape part is already file-local in `cors_contract_diagnostics`). |
| `headers-contract` (doctor) full check | Catalog enforcement that needs production profile context. | Partial overlap; LSP `headers_contract_diagnostics` already covers shape. |
| `auth_identity_field_unknown` family | Needs auth target resource fields cross-file. | Cross-file. |
| `auth_password_algorithm_hash_mismatch` family | Crypto registry catalog. | Cross-file. |
| `auth_oauth_adapter_unbound` family | Registry adapter lookup. | Cross-file. |
| `auth_sessions_resource_unknown` family | Resource lookup across features. | Cross-file. |
| `app_locale_*` family (default unsupported, fallback unknown, fallback cycle) | Need translation catalog files. | Cross-file. |
| `translation_*` (catalog_path_missing, locale_unsupported, locale_missing_for_default, locale_missing_for_supported, key_unused) | Filesystem + JSON catalog files. | Cross-file. |
| `cldr_plural_arm_invalid` | Per-locale CLDR rules. | Catalog. |
| `rule_message_ref_unresolved` | Cross-feature rule → message catalog lookup. | Cross-file. |
| `notification_template_placeholder_unknown` | `templates/*.tmpl` filesystem. | Cross-file. |
| `agent_expose_path_conflict_cross_feature_diagnostics` | Cross-feature path conflict — name says it. | Cross-file. |
| `agent_run_subscriber_payload_drift_diagnostics` | Needs subscriber payload type from another feature. | Cross-file. |
| `agent_expose_audience_unknown_diagnostics` | Needs app manifest audiences. | Cross-file. |
| `agent_lower_failed_diagnostics` / `agent_parse_failed_diagnostics` | Surfaced by the analyzer pipeline itself. | LSP already shows parse error via different path. |
| `tool_registry_effect_required_diagnostics` | Tool registry from `registry.lzi`. | Cross-file. |
| `openapi_text_pattern_api_block` | OpenAPI catalog. | Cross-file/spec. |
| `audit_emit_to_unknown_diagnostics` | Audit sink registry. | Cross-file. |
| `health_probe_path_invalid_diagnostics` | App-runtime + route catalog. | Cross-file (already LSP-partial via `app-operational-contract`). |
| `event_trace_level_invalid_diagnostics` / `event_trace_level_on_domain_event_diagnostics` | Trace level catalog. | File-local? Worth re-checking — these may be cheap to lift. |
| `feature-orphan-component` (`crates/lazuli_cli/src/doctor/folder/feature_orphan.rs`) | Walks `features/` directory. | Filesystem. |
| `cross-feature-direct-import` | Walks Go handler source for cross-feature `import`. | Cross-language scan. |
| `pages-bypass` | Walks frontend pages directory. | Filesystem. |
| `type-duplicate` | Walks all `.lzi` files for duplicate type names. | Cross-file. |
| `previously-*` family | Needs renamed-symbol history across the project. | Cross-file. |

## Per-doctor-module breakdown

| Module | Codes | LSP coverage | Recommendation |
|---|---|---|---|
| `doctor.rs` (root) | ~215 | Partial via category codes (`app-*-contract`, `profile-*-contract`, `workspace-*-contract`). | Mostly cross-file; keep in doctor. Lift the per-line shape parts the LSP doesn't already cover (notification, webhook, cache, RBAC, error-page block shapes). |
| `doctor/correctness/` | 6 | None | **Port all 6 to LSP file-local.** All take `(feature, path)` → vec<Finding>. Highest typo-catching value. |
| `doctor/domain/` | 4 | None | Port all 4 (`AGGREGATE-*`, `INVARIANT-PREDICATE-INVALID`, `SLUG-UNIQUENESS-IMPLICIT`). |
| `doctor/lifecycle/` | 10 | None | Port all 10. Frequent in any resource that uses lifecycle DSL. |
| `doctor/vocab/` | 12 | None | Port all 12. Memory `project_vocab_audit_findings_2026-05-14.md` says these surface in real audits weekly. |
| `doctor/encryption/` | 6 | Partial via `crypto-*` codes (shape only) | Port the file-local ones (`ENC-E2EE-EVENT-001`, `ENC-TENANCY-001`, `ENC-TEMPLATE-AXIS-001`). Skip `ENC-KEY-MISSING-001` which needs registry. |
| `doctor/poller/` | 12 | None | Port all 12 — all single-feature checks. Pleiades poller features will benefit. |
| `doctor/report/` | 11 | None | Port all 11 — all single-feature. |
| `doctor/lzx/` | 13 | Partial via `lzx-*-contract` codes (shape only) | Port all 13. These run on lowered `.lzx` and are the most likely to fire in editor as user types LZX. |
| `doctor/folder/` | 4 | None | Skip — all filesystem-walking (`cross-feature-direct-import`, `feature-orphan-component`, `pages-bypass`, `type-duplicate`). |
| `doctor/auth/` | 4 | None | Skip — all need registry catalogs. |
| `doctor/rbac/` | 0 standalone | `RBAC-*` lives in `doctor.rs`. | Skip the catalog checks; consider best-effort `policy { role: X }` shape check. |
| `doctor/design/` | 8 | None | Port all 8. `design.lzi` is file-local; the leaks live in the open file. |
| `doctor/report/` | (counted above) | | |

## Recommendations (ordered)

1. **Wire the 46 file-local-portable error-severity codes first.** All of them share the same `check(feature, path) -> Vec<Finding>` shape. Pattern: in `diagnostics_for_with_profile` (`crates/lazuli_lsp/src/lib.rs:413`), after `lower_document` succeeds for canonical sources, call each `check` and translate each `Finding` into an LSP `Diagnostic` (use the existing `simple_canonical_diagnostic` helper or a new `simple_doctor_diagnostic` that uses `source: "lazuli-doctor"` and the doctor's CODE constant). Estimated 30-60 minutes per module to wire (12 modules → one focused day) + a fixture-roundtrip test per module.
2. **Then the medium-priority warnings (~28 codes).** Same wiring pattern; broader user benefit (design tokens, deprecation, capability file shapes).
3. **Then the LZX 13 codes specifically.** LZX is where users currently get the least feedback in-editor; the doctor LZX modules already operate on lowered `.lzx` so the wiring is identical.
4. **Multi-file-but-local-OK (~14 codes).** Lower priority but useful when only the relevant file is open. Implement as degraded checks that suppress when cross-file context would change the verdict.
5. **Decision needed on canonical code naming.** The LSP currently uses *category* codes (`app-runtime-contract`); the doctor uses *point* codes (`APP-RUNTIME-001`). When porting, decide whether the LSP should emit the doctor code as-is (round-tripping `lazuli doctor` codes into editor tooltips) or keep the LSP category codes and add the doctor code as a `data` field. The former gives "click code → docs URL" parity with doctor CLI output; the latter preserves current LSP API. Recommendation: emit the doctor code as-is, set `source: "lazuli-doctor"` so the two diagnostic streams remain distinguishable in the Problems panel.
6. **Filesystem / cross-file codes stay in doctor.** Wire `lazuli doctor` to run on save via the VS Code extension's task runner (`extension.js`) and surface its full results as Problems-panel entries through `lazuli doctor --format=lsp` (proposed flag — emit LSP-compatible JSON). That covers the "I see the squiggle on every save, doctor is the source of truth for cross-file" UX.
7. **Add an LSP integration test fixture** per module to prevent the LSP from silently regressing as new doctor rules ship. Pattern: `crates/lazuli_lsp/tests/doctor_parity.rs` with a `.lzi` snippet known to trip the doctor rule; assert the LSP also returns a diagnostic at the expected line with the expected code.

## Appendix — methodology limitations

- Heuristic test-region filtering missed a small number of doctor `code: "X"` calls that live inside `#[cfg(test)]` modules — counts may be off by ±5.
- LSP code extraction only captured the literal string argument; `simple_canonical_diagnostic` calls that pass a `&str` variable would be missed (none observed in spot checks).
- Severity tagging used `DoctorSeverity::Error` as the default when the helper function obscured the per-call severity. The exact severity table is in `doctor.rs:1868` (Error / Warning / Info / Hint) and resolves dynamically through `doctor_rule_severity(security_profile)` (`doctor.rs:1173`) for several rules.
- The audit did not run `cargo test --features doctor_telemetry` to cross-check that every catalog entry actually fires in some fixture; that's the natural next step before porting begins.
