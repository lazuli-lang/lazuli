mod aggregators;
pub mod auth;
pub mod auth_refresh;
mod dispatch;
mod facts;
pub mod folder;
mod helpers;
pub mod lifecycle_gate;
pub mod lzx;
mod package;
pub(crate) mod parsers;
pub mod rbac;
mod refs;
mod release;
mod returns_list_001;
mod returns_list_002;
pub mod route_guard;
mod runtime_options;
mod scanners;
pub mod schema_rich_001;
mod self_audit;

// Re-export canonical / lzx fact collectors so the in-tree
// consumers (`doctor/package.rs`, `doctor/tests.rs`) keep their
// `super::collect_*` / `super::populate_*` call paths after the
// `facts/` extraction.
pub(crate) use facts::canonical::{
    callable_header_key_from_trimmed, collect_callable_bodies_for_eval_order,
    collect_canonical_facts, collect_feature_integration_requirements,
    collect_operational_lzi_facts, job_block_has_schedule, named_block_name,
    populate_command_external_calls_from_ir, populate_commands_from_ir,
    populate_job_external_calls_from_ir,
};
pub(crate) use facts::feature_ir::{
    collect_feature_adapters, collect_feature_uses, collect_file_capability_facts,
    extract_cap_file_field_line, populate_feature_resources_from_ir,
    populate_feature_symbols_from_ir,
};
pub(crate) use facts::lines::{
    collect_construct_lines, collect_event_group_lines, collect_query_lines,
    collect_text_pattern_api_names, find_keyword_line, tenancy_axis_for,
};
pub(crate) use facts::lzx::{
    collect_lzx_experience_facts, collect_lzx_operational_facts, lzx_route_surface_platform,
};

// Re-export the `app_contract` aggregator's public surface used
// outside the aggregator itself — `doctor/dispatch.rs` calls
// `operational_env_names`, `aggregators::app_manifest` consumes the
// rest of the family, and the report-storage aggregator reads
// `collect_object_storage_caps`. Helpers private to `app_contract`
// (e.g. `pack_source_name`) stay scoped there.
pub(crate) use aggregators::app_contract::{
    adapter_provenance_diagnostics, app_binding_contract_diagnostics, app_has_any_capability,
    app_has_target, app_has_url, app_pack_contract_diagnostics, app_runtime_runs,
    app_runtime_serves, app_service_contract_diagnostics, collect_object_storage_caps,
    enabled_pack_provided_features, operational_env_names, profile_contract_diagnostics,
};

// Re-export the `approval` aggregator's public surface so
// `doctor/package.rs`, `doctor/tests.rs`, and `doctor/dispatch.rs`
// keep their `super::*` call paths.
pub(crate) use aggregators::approval::{
    ApprovalBlockPresence, approval_diagnostics, approval_missing_children_diagnostics,
    collect_approval_block_presence,
};

// Re-export the `command_routing` aggregator's public surface so the
// `facts/canonical.rs` + `facts/lzx.rs` collectors, `dispatch.rs`, and
// the in-tree LSP cross-checks keep their `crate::doctor::*` call
// paths after the R6-2 extract.
pub(crate) use aggregators::command_routing::{
    command_reachability_diagnostic, command_route_binding_diagnostics,
    parse_integration_requirement, policy_reachability_diagnostics, resolve_command_target,
    resolve_platform_action_target, route_slot_name,
};

// Re-export the `correctness` aggregator's tier3-fact dispatchers so
// `doctor/dispatch.rs` keeps the `super::*` import block it ships
// today. Each dispatcher walks the lifted `Tier3FeatureFacts` slice
// and emits one diagnostic per finding (deduped where the underlying
// rule allows two findings to collide on the same anchor).
pub(crate) use aggregators::correctness::{
    duplicate_query_name_diagnostics, missing_policy_on_query_diagnostics,
    mutation_without_readback_diagnostics, route_id_effect_consistency_diagnostics,
    updates_missing_updated_at_diagnostics,
};

// Re-export the `report_storage` aggregator's three families
// (`REPORT-*`, `@cap.File(...)`, `query.view <name>` SQL) so the
// in-tree consumers (`doctor/dispatch.rs`, `aggregators::domain`,
// `aggregators::error_vocab`) keep their `crate::doctor::*` call
// paths after the R6-2 extract.
pub(crate) use aggregators::report_storage::{
    cap_file_storage_diagnostics, make_synthetic_feature_for_reports,
    query_view_sql_file_diagnostics, report_diagnostics,
};

// Re-export the `refs` reference-scanner surface so the
// `aggregators::lazurite_manifest` cluster + `facts/canonical.rs`
// collector keep their `crate::doctor::*` call paths after the R6-2
// extract. Items not consumed across the module tree
// (`PluginReferenceFact`, `AtReferenceFact`, `plugin_reference_name_len`,
// `reference_name_len`, `reference_namespace`) stay private to `refs`.
pub(crate) use refs::{
    collect_at_references_in_source, collect_package_plugin_references,
    collect_plugin_references_in_source, go_mod_lazuli_runtime_version,
    is_allowed_reference_namespace_for_doctor, path_references,
};

// Re-export the `lazurite_manifest` aggregator's dispatcher so
// `doctor/dispatch.rs` keeps its `super::lazurite_manifest_diagnostics`
// call path after the extraction.
pub(crate) use aggregators::lazurite_manifest::lazurite_manifest_diagnostics;

// Re-export the `rbac_catalog` aggregator's public surface so
// `doctor/dispatch.rs` keeps its `super::collect_known_roles` /
// `super::rbac_*_diagnostics` call paths.
pub(crate) use aggregators::rbac_catalog::{
    collect_known_roles, collect_package_rbac_catalog, rbac_catalog_diagnostics,
    rbac_catalog_missing_diagnostics, rbac_missing_policy_diagnostics,
    rbac_role_undeclared_diagnostics,
};

// Re-export the `semantic_type` aggregator's public surface so
// `doctor/package.rs`, `doctor/tests.rs`, `doctor/dispatch.rs`, and
// the sibling aggregators (`error_vocab`, `lifecycle_gate`,
// `route_guard`) keep their `super::*` / `crate::doctor::*` call
// paths.
pub(crate) use aggregators::semantic_type::{
    SEMANTIC_TYPE_UNKNOWN_CODE, cross_feature_type_unresolved_diagnostics,
    feature_uses_missing_diagnostics, policy_ref_surface_text,
    semantic_type_unknown_diagnostics_for_feature,
    semantic_type_unknown_diagnostics_for_syntax_feature, span_line,
};

pub(crate) use package::DoctorPackage;
pub use runtime_options::DoctorRuntimeOptions;
// Re-exported so the `returns_list_001` / `returns_list_002` rule
// modules can keep calling `super::leading_spaces` (the helper is
// indent-aware, used to walk command bodies in those rules).
pub(super) use scanners::leading_spaces;

use helpers::{
    doctor_project_root, parse_doctor_format, parse_doctor_severity, parse_fail_on_specs,
    project_has_lazurite_manifest, resolve_test_discipline_severity,
};
// Re-export the shared offset → (line, column) helpers so the
// `facts/*` collectors (`canonical`, `lzx`) keep their existing
// `crate::doctor::line_col_for_offset` call paths after the R6-2
// extract moved both helpers out of `mod.rs` into `helpers.rs`.
pub(crate) use helpers::{line_col_for_offset, line_col_for_offset_in_file};
use parsers::{
    auth_session_ttl_seconds, cache_ttl_as_seconds, catalog_list, environments_summary,
    error_page_catalog_display, format_accept_list, format_agent_policy, format_name_list,
    format_visibility, http_method_word, is_lzi_path, is_lzx_path, is_one_dot_zero_plus,
    is_parseable_cidr, is_parseable_duration, is_parseable_size, is_valid_notification_duration,
    major_minor, mime_matches, mime_sets_intersect, normalise_path, openapi_today_pivot,
    parse_iso_date, parse_notification_duration_seconds, payload_field_list, same_origin,
    tool_kind_word, type_ref_name,
};
use scanners::{
    collect_deprecated_exports, collect_lazuli_paths_recursive, derive_feature_name, is_ident_char,
    is_identifier, is_type_name, lazuli_version_line, matches_word, package_stem,
    parse_export_name, walk_frontend_ts_files, walk_gen_ts_files,
};

// Re-export file-local diagnostic sub-modules extracted to the `lazuli_doctor`
// crate on 2026-05-15 so the LSP can import them. Existing call sites inside
// this module continue to reference them as `correctness::`, `vocab::`, etc.
pub use lazuli_doctor::{
    RuleCategory, correctness, design, domain, encryption, lifecycle, poller, report,
    test_discipline, vocab,
};

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_analyzer::lower_feature_skeleton;
use lazuli_ir::{
    self as ir, Agent, AppContract, AppManifest, AppProfile, AppRegistry, AppWorkspace, LZIR_SCHEMA,
};
use lazuli_lsp::SecurityProfile;
use lazuli_syntax::{LzxDocument, LzxPlatform, LzxPlatformView, parse_feature_skeletons};
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::app_manifest::{
    RegistryParseOutput, RegistryToolDefectReason, parse_app_contracts, parse_app_manifest,
    parse_app_profiles, parse_app_registry_with_defects, parse_app_workspace,
};
use crate::lazurite_manifest::{self, Manifest, MigrationStrategy};

pub fn doctor_command(
    input: &Path,
    security_profile: SecurityProfile,
    check_release: bool,
    allow_version_mismatch: bool,
) -> Result<()> {
    doctor_command_with_options(
        input,
        security_profile,
        check_release,
        allow_version_mismatch,
        DoctorRuntimeOptions::default(),
    )
}

pub fn doctor_command_with_options(
    input: &Path,
    security_profile: SecurityProfile,
    check_release: bool,
    allow_version_mismatch: bool,
    opts: DoctorRuntimeOptions,
) -> Result<()> {
    if !allow_version_mismatch {
        let project_root = doctor_project_root(input);
        let manifest = lazurite_manifest::load(&project_root).with_context(|| {
            format!(
                "failed to load {}",
                project_root.join("Lazurite.toml").display()
            )
        })?;
        crate::version::enforce_manifest_pin(manifest.as_ref())?;
    }

    if check_release {
        return release::doctor_release_command(input);
    }

    // W3 — `--self` short-circuits the standard IR pipeline and walks
    // the framework's Rust source instead. Pairs with workspace
    // `Lazurite.toml [doctor.internal_hygiene]` (preset / overrides).
    if opts.self_audit {
        return self_audit::doctor_self_command(input, &opts);
    }

    let package = DoctorPackage::load(input, security_profile)?;
    let diagnostics = package.diagnostics();

    // Wave 2 — JSON output surface. When `--format json` (or ndjson) is
    // requested, emit the canonical `DoctorReport` schema instead of the
    // text rendering and short-circuit. Auto mode falls back to text for
    // TTY stdout and JSON for non-TTY pipes (per agent-first parity).
    let format = parse_doctor_format(opts.format.as_deref());
    let want_json = matches!(
        format,
        crate::doctor_report::DoctorFormat::Json | crate::doctor_report::DoctorFormat::Ndjson
    );

    if want_json {
        let report = build_doctor_report(&diagnostics, opts.coverage, &package);
        let payload = serde_json::to_string_pretty(&report)
            .context("failed to serialize DoctorReport JSON")?;
        println!("{payload}");
        // Wave 2.2 — fail-on gate
        let specs =
            parse_fail_on_specs(&opts.fail_on).map_err(|e| anyhow::anyhow!("--fail-on: {e}"))?;
        if crate::doctor_report::report_fails_gate(&report, &specs)
            || diagnostics
                .iter()
                .any(|d| d.severity == DoctorSeverity::Error)
        {
            bail!("{} failed Lazuli doctor checks", input.display());
        }
        return Ok(());
    }

    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DoctorSeverity::Error);

    for diagnostic in &diagnostics {
        diagnostic.print();
    }

    if opts.coverage {
        // Wave 6 — emit per-layer coverage summary at end of text output.
        let report = build_doctor_report(&diagnostics, true, &package);
        if let Some(cov) = &report.coverage {
            println!("\nCoverage report (schema_version {}):", cov.schema_version);
            for (layer, info) in &cov.layers {
                println!(
                    "  {:<24} {:>6.1}%  {:<8} ({}/{})",
                    layer, info.pct, info.verdict, info.covered, info.total
                );
            }
            println!(
                "  gate: {} (below_block={:?}, below_warn={:?})",
                cov.gate_result.verdict, cov.gate_result.below_block, cov.gate_result.below_warn
            );
        }
    }

    // Wave 2.2 — text-mode fail-on still gates
    let specs =
        parse_fail_on_specs(&opts.fail_on).map_err(|e| anyhow::anyhow!("--fail-on: {e}"))?;
    if !specs.is_empty() {
        let report = build_doctor_report(&diagnostics, opts.coverage, &package);
        if crate::doctor_report::report_fails_gate(&report, &specs) {
            bail!(
                "{} failed Lazuli doctor checks (--fail-on gate)",
                input.display()
            );
        }
    }

    if has_error {
        bail!("{} failed Lazuli doctor checks", input.display());
    }

    println!("{} passed Lazuli doctor checks", input.display());
    Ok(())
}

/// Build a canonical `DoctorReport` from `DoctorDiagnostic` list +
/// optional coverage. Wave 2 (JSON schema) + Wave 6 (coverage).
pub(super) fn build_doctor_report(
    diagnostics: &[DoctorDiagnostic],
    want_coverage: bool,
    package: &DoctorPackage,
) -> crate::doctor_report::DoctorReport {
    use crate::doctor_report::{
        DoctorReport, DoctorSummary, FindingBuilder, FindingJson, Severity as JsonSeverity,
        SpanJson, classify_result,
    };
    use std::collections::BTreeMap;
    let mut findings = Vec::with_capacity(diagnostics.len());
    let mut summary = DoctorSummary {
        errors: 0,
        warnings: 0,
        infos: 0,
        by_category: BTreeMap::new(),
        by_feature: BTreeMap::new(),
        by_rule: BTreeMap::new(),
    };
    for d in diagnostics {
        let sev = match d.severity {
            DoctorSeverity::Error => {
                summary.errors += 1;
                JsonSeverity::Error
            }
            DoctorSeverity::Warning => {
                summary.warnings += 1;
                JsonSeverity::Warning
            }
            DoctorSeverity::Info => {
                summary.infos += 1;
                JsonSeverity::Info
            }
            DoctorSeverity::Hint => {
                summary.infos += 1;
                JsonSeverity::Hint
            }
        };
        let category = d.category.unwrap_or_else(|| {
            lazuli_doctor::rule_category::RuleCategory::from_code_prefix(&d.code)
        });
        *summary
            .by_category
            .entry(category.as_str().to_owned())
            .or_insert(0) += 1;
        if let Some(fname) = &d.feature_name {
            *summary.by_feature.entry(fname.clone()).or_insert(0) += 1;
        }
        *summary.by_rule.entry(d.code.clone()).or_insert(0) += 1;
        let construct_json = d
            .construct
            .as_ref()
            .map(|c| crate::doctor_report::ConstructJson {
                kind: c.kind.clone(),
                name: c.name.clone(),
                feature: d.feature_name.clone(),
                policy: None,
            });
        let fix_json = d.fix.as_ref().map(|f| crate::doctor_report::FixJson {
            action: f.action.clone(),
            preview: f.preview.clone(),
            auto_applicable: f.auto_applicable,
            cli: f.cli.clone(),
        });
        let finding = FindingBuilder {
            rule: d.code.clone(),
            category,
            severity: sev,
            path: d.path.clone(),
            line: d.line,
            column: d.column,
            message: d.message.clone(),
            construct: construct_json,
            fix: fix_json,
            feature_name: d.feature_name.clone(),
        };
        findings.push(finding.build());
    }
    let result = classify_result(&summary);
    let coverage = if want_coverage {
        Some(package.coverage_report())
    } else {
        None
    };
    DoctorReport {
        schema_version: 1,
        result: result.as_str().to_string(),
        summary,
        findings,
        coverage,
    }
}

/// MCP read surface — runs the same `DoctorPackage` pipeline as
/// `doctor_command` but returns a structured JSON array of
/// diagnostics instead of printing to stdout + bailing.
///
/// Wired by `crate::cmd_mcp` for the `tools/call doctor` MCP method.
/// Wire-thin: returns `serde_json::Value` to keep
/// `DoctorPackage` / `DoctorDiagnostic` / `DoctorSeverity` private to
/// this module (the MCP catalog is closed at the JSON surface, not
/// the type surface).
///
/// Companion: `docs/proposals/lazuli-mcp-subcommand-2026-05-17.md` §6.
pub(crate) fn doctor_diagnostics_json(
    input: &Path,
    security_profile: SecurityProfile,
) -> Result<serde_json::Value> {
    let package = DoctorPackage::load(input, security_profile)?;
    let diagnostics = package.diagnostics();
    let payload: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            serde_json::json!({
                "path": d.path.display().to_string(),
                "line": d.line,
                "column": d.column,
                "severity": match d.severity {
                    DoctorSeverity::Error => "error",
                    DoctorSeverity::Warning => "warning",
                    DoctorSeverity::Info => "info",
                    DoctorSeverity::Hint => "hint",
                },
                "code": d.code,
                "message": d.message,
            })
        })
        .collect();
    Ok(serde_json::Value::Array(payload))
}

/// Phase L Tier 3 — lifted job/webhook/notification/event_group bundle
/// for one feature, with source line anchors for diagnostic placement.
///
/// Field-level visibility (Wave 4.3 R2): every field is `pub(crate)` so
/// the per-category aggregators under `doctor/aggregators/*` can read
/// the lifted Tier 3 IR without re-deriving it from source. Construction
/// stays inside `DoctorPackage::load` — these structs are read-only
/// once the package is loaded.
#[derive(Debug, Clone)]
pub(crate) struct Tier3FeatureFacts {
    pub(crate) feature: String,
    pub(crate) path: PathBuf,
    pub(crate) feature_line: usize,
    /// Resolved tenancy axis (`org`, `team`, custom, none) inferred
    /// from the feature's `defaults` block. `None` if the source did
    /// not declare a default. Doctor's tenant_from / fanout checks
    /// use this to cross-check axis references.
    pub(crate) tenancy_axis: Option<String>,
    /// Feature-level default policy from `defaults.policy`. Queries
    /// with no per-query policy inherit this; absent defaults imply the
    /// runtime's public fallback.
    pub(crate) defaults_policy: Option<lazuli_ir::PolicyRef>,
    /// Feature-level `defaults.timestamps`. Most correctness dispatchers
    /// only need commands/resources, but audit timestamp checks must know
    /// whether `updated_at` is framework-managed.
    pub(crate) defaults_timestamps: bool,
    pub(crate) jobs: Vec<lazuli_ir::Job>,
    pub(crate) webhooks: Vec<lazuli_ir::Webhook>,
    pub(crate) notifications: Vec<lazuli_ir::Notification>,
    pub(crate) event_groups: Vec<lazuli_ir::EventGroup>,
    /// Migrations bucket cycle Route C — lifted `TenantMigration`
    /// declarations for this feature, paired with `tenant_migration_lines`
    /// for `TM-*` diagnostic line anchoring.
    pub(crate) tenant_migrations: Vec<lazuli_ir::TenantMigration>,
    /// Migrations bucket cycle Route C — `Resource.previous_names`
    /// captures plus current resource names per feature, for
    /// `PREVIOUSLY-*` cross-checks.
    pub(crate) resource_previous_names: Vec<ResourcePreviousFact>,
    /// Migrations bucket cycle Route C — `Field.previous_names`
    /// captures (resource + field + previous names + line).
    pub(crate) field_previous_names: Vec<FieldPreviousFact>,
    /// Migrations bucket cycle Route C — every current resource name
    /// in this feature (including resources without any `previously`
    /// declaration) so `PREVIOUSLY-FWD-001` can detect stale rename
    /// targets pointing at live symbols.
    pub(crate) all_resource_names_in_feature: BTreeSet<String>,
    /// Migrations bucket cycle Route C — `resource_name -> {field_names}`
    /// per feature for `PREVIOUSLY-FWD-001` on field-level rename hints.
    pub(crate) all_field_names_in_feature: BTreeMap<String, BTreeSet<String>>,
    /// `job_name -> source line` lookup.
    pub(crate) job_lines: BTreeMap<String, usize>,
    pub(crate) webhook_lines: BTreeMap<String, usize>,
    pub(crate) notification_lines: BTreeMap<String, usize>,
    pub(crate) tenant_migration_lines: BTreeMap<String, usize>,
    /// `event_group_pattern -> source line` lookup.
    pub(crate) event_group_lines: BTreeMap<String, usize>,
    /// OpenAPI/Cache bucket cycles — lifted `command` IR per feature.
    /// Doctor reads `Command.deprecated` and `Command.invalidates` from
    /// here for the openapi/cache cross-checks.
    pub(crate) commands: Vec<lazuli_ir::Command>,
    /// `command_name -> source line` lookup. Anchors `deprecated_*` and
    /// `cache_invalidates_*` diagnostics at the command header.
    pub(crate) command_lines: BTreeMap<String, usize>,
    /// Cache bucket cycle — lifted `query` IR per feature. Doctor reads
    /// `Query.cache` (when populated) for the cache cross-checks.
    pub(crate) queries: Vec<lazuli_ir::Query>,
    /// `query_name -> source line` lookup. Anchors `cache_*` diagnostics
    /// at the query header.
    pub(crate) query_lines: BTreeMap<String, usize>,
    /// Cache bucket cycle (CL.C.3) — feature-level `cache <name>`
    /// profile declarations lifted from the canonical-indent slice.
    /// Doctor uses this to (1) resolve query `cache <profile>`
    /// references for `cache-profile-unknown`, (2) build the package-
    /// wide tag index for `cache-tag-unknown`, and (3) cross-check TTL
    /// shape invariants for `cache-ttl-contract`.
    pub(crate) caches: Vec<lazuli_ir::CacheProfile>,
    /// `cache_profile_name -> source line` lookup. Anchors CL.C.3
    /// diagnostics at the profile header.
    pub(crate) cache_lines: BTreeMap<String, usize>,
    /// OpenAPI bucket cycle — every `api <name>` declaration in this
    /// feature (text-pattern era, before Tier 4 lift). Doctor uses this
    /// to surface `openapi_text_pattern_api_block`.
    pub(crate) api_names_text_pattern: Vec<String>,
    /// i18n bucket cycle — lifted typed `api` blocks (post Tier 4).
    /// Doctor reads `Api.locale_negotiate` from here for per-endpoint
    /// override validation.
    pub(crate) apis: Vec<lazuli_ir::Api>,
    /// Phase L Tier 4b — `api_name -> source line` lookup for the lifted
    /// `apis` slot. Anchors `agent_expose_*` cross-checks at each api
    /// header.
    pub(crate) api_lines: BTreeMap<String, usize>,
    /// Cut A.7 — lifted agents for report auto-mount route conflict checks.
    pub(crate) agents: Vec<lazuli_ir::Agent>,
    /// i18n bucket cycle — lifted `translation` block (when authored).
    pub(crate) translation: Option<lazuli_ir::Translation>,
    pub(crate) translation_line: usize,
    /// Phase L Tier 4 follow-up — lifted `record <Name>` declarations
    /// per feature. Replaces the text-scanned `FeatureSymbols.records`
    /// for the agent discriminator cross-checks.
    pub(crate) records: Vec<lazuli_ir::Record>,
    /// Phase L Tier 4 follow-up — lifted `enum <Name>` declarations per
    /// feature. Closes out the canonical-indent slice for `domain`:
    /// `agent_discriminator_target_invalid` and
    /// `check_record_discriminator` both read from here. The retired
    /// `FeatureSymbols.enums` text walker is gone.
    pub(crate) enums: Vec<lazuli_ir::EnumDecl>,
    /// Notifications expanded bucket cycle — lifted `event` /
    /// `event.trace` declarations for this feature. `NOTIF-DIGEST-001`
    /// resolves `notification.digest.group_by` against the trigger
    /// event's payload schema; cross-feature lookup walks `facts`
    /// keyed by `<feature>.<event>`. Tracking the full payload at the
    /// fact level keeps the diagnostic shape-aware without adding a
    /// new fact family.
    pub(crate) events: Vec<lazuli_ir::Event>,
    /// Whether the feature authored a top-level `policies` block.
    /// `Feature.policies` has a default value, so doctor reads the
    /// lowered `span_ref` to distinguish "absent" from "declared".
    pub(crate) policies_declared: bool,
    /// Phase L Tier 4 follow-up — full lifted `policies` block. Used
    /// by `SCOPE-OWNER-COLUMN-001` and other lints that need to walk
    /// a command's policy atom list (resolving `PolicyRef::Local`
    /// through `categories`) without re-deriving the lookup from
    /// text. Empty `Policies::default()` when the feature did not
    /// author a block.
    pub(crate) policies: lazuli_ir::Policies,
    /// Wave 10 — feature-level `extensions` block lifted. Used by
    /// `resource_validates_path_unknown` to resolve
    /// `validates field <f> @validator.<name>` references against
    /// declared `extensions.validator`. Empty when the feature has
    /// no extensions block.
    pub(crate) extensions: Vec<lazuli_ir::Extension>,
    /// Report vocab — lifted `report` declarations per feature. See
    /// `docs/proposals/report-vocab.md`.
    pub(crate) reports: Vec<lazuli_ir::Report>,
    /// `report_name -> source line` lookup. Anchors `REPORT-*`
    /// diagnostics at the report header.
    pub(crate) report_lines: BTreeMap<String, usize>,
    /// Resources captured (full `Resource`) per feature — used by
    /// `REPORT-COLUMN-MISMATCH-001` to resolve `row.<field>` against
    /// the source query's projection.
    pub(crate) resources: Vec<lazuli_ir::Resource>,
    /// Raw `ReportDecl` AST per feature — used by rules that need the
    /// original (pre-lowering) form (e.g. `REPORT-FORMAT-UNKNOWN-001`
    /// scans the AST formats list since lowering drops unknown tokens).
    pub(crate) report_decls: Vec<lazuli_syntax::ReportDecl>,
    /// CL.C.4 — lifted `aggregate <Name>` declarations per feature.
    /// Powers the four domain-model diagnostics:
    /// `AGGREGATE-ROOT-UNKNOWN`, `AGGREGATE-CONTAINS-UNKNOWN`,
    /// `INVARIANT-PREDICATE-INVALID`, `SLUG-UNIQUENESS-IMPLICIT`.
    /// Empty vec when the feature authored no aggregate blocks.
    pub(crate) aggregates: Vec<lazuli_ir::Aggregate>,
    /// CL.C.4 — `aggregate_name -> source line` lookup. Anchors the
    /// `AGGREGATE-*` and aggregate-scoped `INVARIANT-*` diagnostics
    /// at the aggregate header.
    pub(crate) aggregate_lines: BTreeMap<String, usize>,
    /// IR Error-Vocab (Cell ANALYZE-1) — lifted `errors` block. `None`
    /// when the feature did not author one. Used by the 7 `ERR-VOCAB-*`
    /// checks for `default hide/expose`, `expose client 4xx/5xx`, and
    /// per-code `<code> message @translation.<key>` rows.
    pub(crate) errors: Option<lazuli_ir::FeatureErrors>,
    /// IR Error-Vocab (Cell ANALYZE-1) — cloned `Feature.uses` so the
    /// `ERR-VOCAB-002` cross-feature key resolver can walk imported
    /// features without re-deriving the import set from `feature_uses`.
    pub(crate) uses: Vec<String>,
    /// Realtime bucket cycle — lifted `channel <name>` declarations per
    /// feature. Used by `CHANNEL-PAYLOAD-001` to resolve each channel's
    /// `payload <Type>` against the feature's `records` / `resources`.
    /// Empty when the feature declares no realtime channels.
    pub(crate) channels: Vec<lazuli_ir::Channel>,
}

/// Migrations bucket cycle Route C — `Resource` rename fact captured
/// from `previously migrated <old>` at the resource header.
#[derive(Debug, Clone)]
pub(super) struct ResourcePreviousFact {
    /// Current resource name.
    pub(super) current_name: String,
    /// Previously-known name(s).
    pub(super) previous_names: Vec<String>,
    pub(super) line: usize,
}

/// Migrations bucket cycle Route C — `Field` rename fact captured
/// from `previously migrated <old>` on a resource field.
#[derive(Debug, Clone)]
pub(super) struct FieldPreviousFact {
    pub(super) resource_name: String,
    pub(super) current_name: String,
    pub(super) previous_names: Vec<String>,
    pub(super) line: usize,
}

impl DoctorPackage {
    /// Iron-hand meta-bundle — dispatch the three `VOCAB-CONTEXT-*`
    /// rules across every `.lzi` feature in the package and resolve
    /// each finding's severity through the layered precedence:
    ///
    ///   1. Manifest user override
    ///      (`[doctor.test_discipline.severity_override."<CODE>"]`)
    ///      wins absolutely. Authors can downgrade an iron-hand error
    ///      back to a warning with a documented `reason`.
    ///   2. Active coverage preset escalation
    ///      (`preset_severity_overrides`): under `tdd-iron-hand` the
    ///      three rules become `error`.
    ///   3. Category default (`doctor_severity_for` →
    ///      `RuleCategory::Vocabulary` → warning at strict, error at
    ///      production).
    ///
    /// The `off` preset suppresses the rules entirely (consistent with
    /// the coverage layers it zeroes out).
    pub(super) fn context_vocab_diagnostics(&self) -> Vec<DoctorDiagnostic> {
        use lazuli_doctor::coverage::{CoveragePreset, preset_severity_overrides};
        use lazuli_doctor::vocab::{
            vocab_context_ctxmd_001, vocab_context_nongoals_001, vocab_context_purpose_001,
        };

        let preset = self.coverage_preset();
        // `off` preset opts out entirely — mirrors how the coverage
        // layers all zero out under `off`.
        if matches!(preset, Some(CoveragePreset::Off)) {
            return Vec::new();
        }

        let manifest_overrides = self
            .lazurite_manifest
            .as_ref()
            .and_then(|m| m.doctor.as_ref())
            .and_then(|d| d.test_discipline.as_ref())
            .map(|td| &td.severity_override);

        let preset_overrides = preset.map(preset_severity_overrides).unwrap_or_default();

        // Resolver: manifest > preset > category default.
        let resolve = |code: &str| -> DoctorSeverity {
            if let Some(map) = manifest_overrides
                && let Some(ov) = map.get(code)
                && let Some(parsed) = parse_doctor_severity(&ov.severity)
            {
                return parsed;
            }
            if let Some(severity_str) = preset_overrides.get(code)
                && let Some(parsed) = parse_doctor_severity(severity_str)
            {
                return parsed;
            }
            doctor_severity_for(
                code,
                RuleCategory::Vocabulary,
                self.security_profile,
                &std::collections::BTreeMap::new(),
            )
        };

        let mut out: Vec<DoctorDiagnostic> = Vec::new();
        for file in &self.files {
            if !is_lzi_path(&file.path) {
                continue;
            }
            let Ok(skeletons) = parse_feature_skeletons(&file.source) else {
                continue;
            };
            for skeleton in &skeletons {
                let Ok(feature) = lower_feature_skeleton(skeleton) else {
                    continue;
                };

                // VOCAB-CONTEXT-PURPOSE-001
                let sev = resolve(vocab_context_purpose_001::Finding::CODE);
                for finding in vocab_context_purpose_001::check(&feature, &file.path) {
                    let message = finding.message();
                    out.push(DoctorDiagnostic {
                        path: finding.path,
                        line: 1,
                        column: 1,
                        severity: sev,
                        code: vocab_context_purpose_001::Finding::CODE.to_owned(),
                        message,
                        category: Some(RuleCategory::Vocabulary),
                        feature_name: Some(finding.feature),
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }

                // VOCAB-CONTEXT-NONGOALS-001
                let sev = resolve(vocab_context_nongoals_001::Finding::CODE);
                for finding in vocab_context_nongoals_001::check(&feature, &file.path) {
                    let message = finding.message();
                    out.push(DoctorDiagnostic {
                        path: finding.path,
                        line: 1,
                        column: 1,
                        severity: sev,
                        code: vocab_context_nongoals_001::Finding::CODE.to_owned(),
                        message,
                        category: Some(RuleCategory::Vocabulary),
                        feature_name: Some(finding.feature),
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }

                // VOCAB-CONTEXT-CTXMD-001 — passes project_root so
                // sidecar paths resolve relative to the feature `.lzi`
                // first, then to the project root as a fallback.
                let sev = resolve(vocab_context_ctxmd_001::Finding::CODE);
                for finding in
                    vocab_context_ctxmd_001::check(&feature, &file.path, Some(&self.project_root))
                {
                    let message = finding.message();
                    out.push(DoctorDiagnostic {
                        path: finding.path,
                        line: 1,
                        column: 1,
                        severity: sev,
                        code: vocab_context_ctxmd_001::Finding::CODE.to_owned(),
                        message,
                        category: Some(RuleCategory::Vocabulary),
                        feature_name: Some(finding.feature),
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
        }
        out
    }

    /// Wave 6 — `lazuli doctor --coverage` data path. Builds the per-layer
    /// coverage report using the active `SecurityProfile`, the optional
    /// `[doctor.coverage] preset = "<name>"` opt-in (Frente 1), and any
    /// per-layer `[doctor.coverage.<layer>]` overrides authored in
    /// `Lazurite.toml`.
    ///
    /// Resolution precedence (highest wins):
    ///   1. per-layer `[doctor.coverage.<layer>]` block
    ///   2. `[doctor.coverage] preset = "<name>"` (Frente 1)
    ///   3. profile-default thresholds (`profile_default_thresholds`)
    ///
    /// Unknown preset names are silently ignored at this layer (a doctor
    /// diagnostic flags them via `check_coverage_preset_unknown`).
    pub fn coverage_report(&self) -> lazuli_doctor::coverage::CoverageReport {
        use lazuli_doctor::coverage::{
            CoveragePreset, CoverageProfile, LayerThreshold, build_coverage_report_with_e2e_root,
            resolve_coverage_thresholds,
        };
        use std::collections::BTreeMap;
        use std::path::PathBuf;

        let (features, lzx_views) = self.coverage_inputs();
        let profile = match self.security_profile {
            SecurityProfile::Prototype => CoverageProfile::Prototype,
            SecurityProfile::Strict => CoverageProfile::Strict,
            SecurityProfile::Production => CoverageProfile::Production,
        };

        // Lift `[doctor.coverage]` from the manifest into the resolver
        // inputs. Absent manifest / absent section → empty maps, which
        // makes resolution fall back to the profile defaults verbatim
        // (backwards compatible).
        let (preset, per_layer_overrides, aggregate_method) = self
            .lazurite_manifest
            .as_ref()
            .and_then(|m| m.doctor.as_ref())
            .and_then(|d| d.coverage.as_ref())
            .map(|cov| {
                let preset = cov.preset.as_deref().and_then(CoveragePreset::parse);
                let per_layer: BTreeMap<String, LayerThreshold> = cov
                    .per_layer
                    .iter()
                    .map(|(name, cfg)| {
                        (
                            name.clone(),
                            LayerThreshold {
                                block_under: cfg.block_under,
                                warn_under: cfg.warn_under,
                            },
                        )
                    })
                    .collect();
                (preset, per_layer, cov.aggregate_method.clone())
            })
            .unwrap_or_default();

        let thresholds =
            resolve_coverage_thresholds(profile, preset, per_layer_overrides, aggregate_method);

        let e2e_discovery_root: Option<PathBuf> = self
            .lazurite_manifest
            .as_ref()
            .and_then(|m| m.testing.as_ref())
            .and_then(|t| t.playwright.as_ref())
            .and_then(|pw| pw.discovery_root.as_deref())
            .map(PathBuf::from);

        build_coverage_report_with_e2e_root(
            &features,
            &lzx_views,
            profile,
            &thresholds,
            Some(&self.project_root),
            e2e_discovery_root.as_deref(),
        )
    }
}

/// Wave 0.5 — category-aware severity resolver.
///
/// Replaces the pre-Wave-0.5 global `doctor_rule_severity()` which
/// applied a single mapping (production → error, everything else →
/// warning) to every rule. With this function in place the framework
/// can express category-specific posture, e.g. "test-discipline rules
/// are info at prototype, warning at strict, error at production"
/// independently of vocab/correctness defaults.
///
/// Resolution order:
/// 1. Per-rule TOML override (`[doctor.<category>].severity_override.<CODE>`)
///    wins absolutely.
/// 2. Category default per profile (see match below).
/// 3. Fallback to the legacy global mapping for backward compat.
///
/// `overrides` is the parsed `Lazurite.toml [doctor]` block; pass an
/// empty map when no manifest is present.
pub(super) fn doctor_severity_for(
    code: &str,
    category: RuleCategory,
    security_profile: SecurityProfile,
    overrides: &std::collections::BTreeMap<String, DoctorSeverityOverride>,
) -> DoctorSeverity {
    // Per-rule override wins absolutely (TOML).
    if let Some(ov) = overrides.get(code) {
        if let Some(parsed) = parse_doctor_severity(&ov.severity) {
            return parsed;
        }
    }
    match (category, security_profile) {
        // Test-discipline rules carry their own per-profile posture so
        // the framework can promote test-completeness without leaking
        // the same posture to vocab/correctness.
        (RuleCategory::TestDiscipline, SecurityProfile::Production) => DoctorSeverity::Error,
        (RuleCategory::TestDiscipline, SecurityProfile::Strict) => DoctorSeverity::Warning,
        (RuleCategory::TestDiscipline, SecurityProfile::Prototype) => DoctorSeverity::Info,
        // Everything else: keep the legacy global mapping so Wave 0.5
        // is purely additive — no behavior change for existing rules.
        (_, SecurityProfile::Production) => DoctorSeverity::Error,
        (_, SecurityProfile::Prototype | SecurityProfile::Strict) => DoctorSeverity::Warning,
    }
}

/// Legacy alias — kept as a thin shim so existing call sites compile
/// unchanged. New rules SHOULD call `doctor_severity_for` directly with
/// their declared `RuleCategory`. Existing sites get the same behavior
/// they had before Wave 0.5 because `from_code_prefix` recovers the
/// category and the non-TestDiscipline branches reproduce the original
/// mapping verbatim.
pub(crate) fn doctor_rule_severity(security_profile: SecurityProfile) -> DoctorSeverity {
    doctor_severity_for(
        "",
        RuleCategory::Vocabulary,
        security_profile,
        &std::collections::BTreeMap::new(),
    )
}

/// Per-rule severity override as authored in `Lazurite.toml`.
///
/// `[doctor.test_discipline.severity_override]` table entries lift into
/// this shape. `DOCTOR-OVERRIDE-NEEDS-REASON-001` enforces the
/// `reason` field is present and non-blank.
#[derive(Debug, Clone)]
pub struct DoctorSeverityOverride {
    /// Author-supplied severity (`warning`, `error`, `info`, `hint`).
    pub severity: String,
    /// Optional human justification. Required (non-blank) per
    /// `DOCTOR-OVERRIDE-NEEDS-REASON-001`.
    pub reason: Option<String>,
}

pub(super) fn vocab_grammar_form_diagnostics(
    files: &[DoctorFile],
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }

    let severity = doctor_rule_severity(security_profile);
    files
        .iter()
        .filter(|file| is_lzi_path(&file.path))
        .flat_map(|file| {
            vocab::vocab_grammar_form_001::check(&file.source, &file.path)
                .into_iter()
                .map(move |finding| {
                    let message = finding.message();
                    DoctorDiagnostic {
                        path: finding.path,
                        line: finding.line,
                        column: finding.column,
                        severity,
                        code: vocab::vocab_grammar_form_001::Finding::CODE.to_owned(),
                        message,
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    }
                })
        })
        .collect()
}

/// MONEY-1 §3.2 — bridge `lazuli_doctor::vocab::money_compare_001` into the
/// CLI's `DoctorDiagnostic` shape. Fixed `Error` severity per the proposal:
/// mixed-currency comparisons silently lose money, which is a bug
/// regardless of `prototype`/`strict`/`production` posture.
pub(super) fn money_compare_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::vocab::money_compare_001;
    money_compare_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: money_compare_001::Finding::CODE.to_owned(),
                message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// Wave 0 — bridge `lazuli_doctor::vocab::vocab_tests_missing_001`
/// into the CLI's `DoctorDiagnostic` shape. The detector has shipped
/// since 2026-05-15 but was never dispatched (see Issue Zero of
/// `docs/proposals/tdd-bdd-first-2026-05-23.md`); this helper closes
/// that gap.
///
/// Severity follows the legacy global mapping (warning at strict,
/// error at production). The rule's `RuleCategory` is `Vocabulary`
/// (matches the module path); Wave 1 will land separate `TEST-*`
/// rules under `RuleCategory::TestDiscipline`. Prototype profile
/// suppresses the warning so quick spikes are not blocked by
/// test-vocabulary discipline.
pub(super) fn vocab_tests_missing_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
    feature_header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        vocab::vocab_tests_missing_001::Finding::CODE,
        RuleCategory::Vocabulary,
        security_profile,
        &std::collections::BTreeMap::new(),
    );
    vocab::vocab_tests_missing_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: feature_header_line.max(1),
                column: 1,
                severity,
                code: vocab::vocab_tests_missing_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::Vocabulary),
                feature_name: Some(finding.feature),
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// MONEY-1 §3.2 — bridge `lazuli_doctor::vocab::money_arithmetic_001` into
/// the CLI's `DoctorDiagnostic` shape. Same fixed-`Error` policy as the
/// comparison check: cross-currency or Money-times-Money arithmetic is a
/// structural bug.
pub(super) fn money_arithmetic_001_diagnostics(
    path: &Path,
    feature: &lazuli_ir::Feature,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::vocab::money_arithmetic_001;
    money_arithmetic_001::check(feature, path)
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            DoctorDiagnostic {
                path: finding.path,
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: money_arithmetic_001::Finding::CODE.to_owned(),
                message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// Parse `design.lzi` at the project root into the lowered IR. Returns
/// `None` when the file is missing OR parse/lower fails — doctor's
/// `design-custom-*` rules then suppress (no false positives when the
/// file isn't authored yet). Mirrors the parse-then-lower pipeline used
/// by `lazuli build`.
pub(crate) fn read_design_ir(project_root: &Path) -> Option<lazuli_ir::Design> {
    let path = project_root.join("design.lzi");
    let source = std::fs::read_to_string(&path).ok()?;
    let ast = lazuli_syntax::parse_design_document(&source).ok()?;
    lazuli_analyzer::lower_design(&ast).ok()
}

pub(crate) fn doctor_rule_path(project_root: &Path, path: PathBuf) -> PathBuf {
    path.strip_prefix(project_root)
        .unwrap_or(&path)
        .to_path_buf()
}

/// CODEGEN-WRAP-001 - typed-error constructors forbidden in bucket source.
///
/// The one-wrap boundary (docs/proposals/bucket-ai-debug-loop-cycle.md §7.2)
/// requires that *lazuli.FieldError, *lazuli.PolicyError, etc. struct values
/// are constructed ONLY in codegen-emitted handlers (the .gen.go boundary),
/// never in hand-written bucket source under runtime/go/lazuli/<bucket>/.
pub(super) fn check_codegen_wrap_001(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let bucket_root = project_root.join("runtime/go/lazuli");
    if !bucket_root.exists() {
        return diagnostics;
    }

    collect_codegen_wrap_001(&bucket_root, &bucket_root, &mut diagnostics);
    diagnostics
}

/// PATTERN-DRAFT-STALE-001 - a `//lazuli:pattern <id> draft`
/// annotation has been on main for more than 7 days.
///
/// The check bounds the draft escape hatch for in-progress codegen
/// patterns. It focuses on the Rust-side pattern catalog because
/// generated `dist/` output is regen-only and often absent in source
/// checkouts. If git/blame data is unavailable, the check is a no-op.
pub(super) fn check_pattern_draft_stale_001(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    check_pattern_draft_stale_001_at(project_root, now)
}

pub(super) fn check_pattern_draft_stale_001_at(
    project_root: &Path,
    now: u64,
) -> Vec<DoctorDiagnostic> {
    let pattern_file = project_root.join("crates/lazuli_codegen_go/src/emitter/patterns.rs");
    let Ok(source) = fs::read_to_string(&pattern_file) else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let seven_days_seconds: u64 = 7 * 24 * 60 * 60;
    for (lineno, line) in source.lines().enumerate() {
        if !is_pattern_draft_line(line) {
            continue;
        }

        let Some(author_time) = git_blame_author_time(project_root, &pattern_file, lineno + 1)
        else {
            continue;
        };
        let age = now.saturating_sub(author_time);
        if age <= seven_days_seconds {
            continue;
        }

        diagnostics.push(DoctorDiagnostic {
            path: pattern_file.clone(),
            line: lineno + 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "PATTERN-DRAFT-STALE-001".to_owned(),
            message: format!(
                "pattern annotation marked `draft` on main for {} days (> 7). Promote to a numbered version (v1, v2, ...) or remove. See docs/proposals/bucket-ai-debug-loop-cycle.md §6.3.",
                age / (24 * 60 * 60)
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    diagnostics
}

/// AUTH-SESSION-CALLSITE-001 — direct `auth.IssueSession` call inside a
/// user-authored handler for a feature whose session resource has extra columns.
///
/// The v1 shim emits a per-resource `Issue<Resource>` wrapper that threads
/// the tenant-pin columns automatically; callers must use that wrapper, not
/// the base `auth.IssueSession` function, so the extra columns are always
/// supplied.
pub(super) fn check_auth_session_callsite_001(
    auth_facts: &[AuthFacts],
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let features_root = project_root.join("features");
    if !features_root.exists() {
        return diagnostics;
    }
    for fact in auth_facts {
        let sessions = match fact.auth.sessions.as_ref() {
            Some(s) if !s.extra_columns.is_empty() => s,
            _ => continue,
        };
        let feature_dir = features_root.join(&fact.feature);
        if !feature_dir.exists() {
            continue;
        }
        let resource_name = sessions.resource.name.as_str();
        collect_issue_session_callsites(&feature_dir, resource_name, &mut diagnostics);
    }
    diagnostics
}

pub(super) fn collect_issue_session_callsites(
    dir: &Path,
    resource_name: &str,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_issue_session_callsites(&path, resource_name, diagnostics);
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !file_name.ends_with(".go")
            || file_name.ends_with(".gen.go")
            || file_name.ends_with("_test.go")
        {
            continue;
        }
        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        for (idx, line) in source.lines().enumerate() {
            if line.contains("auth.IssueSession(") {
                diagnostics.push(DoctorDiagnostic {
                    path: path.clone(),
                    line: idx + 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "AUTH-SESSION-CALLSITE-001".to_owned(),
                    message: format!(
                        "direct call to `auth.IssueSession` in a feature with extra session columns; use `auth.Issue{resource_name}` instead so the tenant-pin column is always supplied.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }
    }
}

pub(super) fn is_pattern_draft_line(line: &str) -> bool {
    if !line.contains("draft") {
        return false;
    }
    (line.contains("PATTERN_") && line.contains("\"draft\"")) || line.contains("//lazuli:pattern")
}

pub(super) fn git_blame_author_time(project_root: &Path, path: &Path, line: usize) -> Option<u64> {
    let blame_path = path.strip_prefix(project_root).unwrap_or(path);
    let output = std::process::Command::new("git")
        .args([
            "blame",
            "-L",
            &format!("{line},{line}"),
            "--porcelain",
            "--",
        ])
        .arg(blame_path)
        .current_dir(project_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let blame_text = String::from_utf8_lossy(&output.stdout);
    blame_text.lines().find_map(|line| {
        line.strip_prefix("author-time ")
            .and_then(|rest| rest.trim().parse::<u64>().ok())
    })
}

pub(super) fn collect_codegen_wrap_001(
    bucket_root: &Path,
    current: &Path,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_codegen_wrap_001(bucket_root, &path, diagnostics);
            continue;
        }
        if !is_bucket_go_source(bucket_root, &path) {
            continue;
        }

        let Ok(source) = fs::read_to_string(&path) else {
            continue;
        };
        for (lineno, line) in source.lines().enumerate() {
            for typed in [
                "FieldError",
                "PolicyError",
                "TenantError",
                "AdapterError",
                "LibBugError",
            ] {
                let needle = format!("lazuli.{typed}{{");
                if line.contains(&needle) {
                    diagnostics.push(DoctorDiagnostic {
                        path: path.clone(),
                        line: lineno + 1,
                        column: line.find(&needle).map(|col| col + 1).unwrap_or(1),
                        severity: DoctorSeverity::Error,
                        code: "CODEGEN-WRAP-001".to_owned(),
                        message: format!(
                            "typed error `{typed}` constructed in bucket source. The one-wrap boundary requires typed errors only at the codegen-emitted handler boundary. Return a bare sentinel from this bucket; codegen will wrap it. See bucket-ai-debug-loop-cycle.md §7.2."
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
        }
    }
}

pub(super) fn is_bucket_go_source(bucket_root: &Path, path: &Path) -> bool {
    if path.parent() == Some(bucket_root) {
        return false;
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("go") {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if name.ends_with("_test.go") {
        return false;
    }
    let rendered = path.to_string_lossy();
    !rendered.contains(".gen.")
}

#[derive(Debug)]
pub(super) struct DoctorAppWorkspace {
    pub(super) path: PathBuf,
    pub(super) manifest: AppWorkspace,
}

#[derive(Debug)]
pub(super) struct DoctorAppContract {
    pub(super) path: PathBuf,
    pub(super) manifest: AppContract,
}

#[derive(Debug)]
pub(crate) struct DoctorAppManifest {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) manifest: AppManifest,
}

#[derive(Debug)]
pub(crate) struct DoctorAppRegistry {
    pub(crate) path: PathBuf,
    pub(crate) manifest: AppRegistry,
}

#[derive(Debug)]
pub(crate) struct DoctorAppProfile {
    pub(crate) path: PathBuf,
    pub(crate) profile: AppProfile,
}

#[derive(Debug)]
pub(crate) struct DoctorFile {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) local_diagnostics: Vec<DoctorDiagnostic>,
    pub(crate) lzx: Option<LzxDocument>,
}

// Cut A — typed agent + symbol facts gathered for cross-feature checks.

#[derive(Debug, Clone)]
pub(super) struct AgentFacts {
    pub(super) feature: String,
    pub(super) agent: Agent,
    pub(super) path: PathBuf,
    /// 1-based source line where the `agent <name>` header lives.
    pub(super) line: usize,
}

/// Phase L — typed `auth` block facts harvested per feature for auth
/// contract diagnostics:
///   - `auth_password_algorithm_hash_mismatch`
///   - `auth_password_no_session`
///   - `auth_sessions_resource_unknown`
///   - `auth_identity_field_unknown`
///   - `auth_oauth_adapter_unbound`
///   - `auth_oauth_no_password_alt`
///   - `auth_session_ttl_too_short`
///
/// The IR carries the lowered shape; the auxiliary `*_line` fields map
/// each subblock back to the source so diagnostics point at the exact
/// authored token rather than the `auth` header.
#[derive(Debug, Clone)]
pub(super) struct AuthFacts {
    pub(super) feature: String,
    pub(super) auth: ir::Auth,
    pub(super) path: PathBuf,
    /// 1-based line for the `auth` header.
    pub(super) line: usize,
    pub(super) identity_line: usize,
    pub(super) password_line: Option<usize>,
    pub(super) password_algorithm_line: Option<usize>,
    pub(super) sessions_line: Option<usize>,
    pub(super) sessions_resource_line: Option<usize>,
    pub(super) mfa_line: Option<usize>,
    /// Per-provider `oauth <provider>` header line.
    pub(super) oauth_lines: BTreeMap<String, usize>,
}

/// Phase L Tier 4 follow-up — both `records` and `enums` slots retired
/// (lifted into `Tier3FeatureFacts.records` / `Tier3FeatureFacts.enums`
/// from the typed IR). The struct now carries only the command policy
/// hint that `agent_tool_diagnostics` still text-walks while the legacy
/// pipeline owns surface commands.
#[derive(Debug, Clone, Default)]
pub(super) struct FeatureSymbols {
    /// Maps short command name (e.g. `archive`) to its registered policy
    /// + safety hint. Commands are inherently write-effect for Cut A.
    pub(super) commands: BTreeMap<String, CommandSymbolFact>,
}

#[derive(Debug, Clone)]
pub(super) struct SymbolFact {
    pub(super) path: PathBuf,
    pub(super) line: usize,
}

/// Phase L Tier 4 follow-up — typed shape of a `resource <Name>`
/// declaration for the `auth_*` cross-checks. Now populated from the
/// IR `Feature.resources` lift instead of a text walker; the
/// `type_ref` slot carries `TypeRef::Capability(CapabilityRef::Hashed(...))`
/// directly so `cap_hashed_algorithm` is a typed match.
#[derive(Debug, Clone, Default)]
pub(super) struct ResourceFact {
    pub(super) path: PathBuf,
    pub(super) line: usize,
    pub(super) fields: BTreeMap<String, ResourceFieldFact>,
}

#[derive(Debug, Clone)]
pub(super) struct ResourceFieldFact {
    /// Typed `TypeRef` lifted from `Field.type_ref`. `cap_hashed_algorithm`
    /// matches `TypeRef::Capability(CapabilityRef::Hashed(...))`;
    /// `is_identity_shaped` matches `Builtin::SemanticEmail/SemanticPhone`
    /// + `Builtin::Id` + the typed `unique` axis.
    pub(super) type_ref: lazuli_ir::TypeRef,
    /// `Field.unique`. Used by `is_identity_shaped` for unique-shaped
    /// identity detection.
    pub(super) unique: bool,
    /// 1-based line where the field is declared. Currently unused by
    /// diagnostics; reserved for future field-anchored messages.
    #[allow(dead_code)]
    pub(super) line: usize,
}

#[derive(Debug, Clone)]
pub(super) struct CommandSymbolFact {
    pub(super) base: SymbolFact,
    pub(super) policy: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct RegistryToolDefect {
    pub(super) path: PathBuf,
    pub(super) line: usize,
    pub(super) name: String,
    pub(super) reason: RegistryToolDefectReason,
}

impl Default for SymbolFact {
    fn default() -> Self {
        Self {
            path: PathBuf::new(),
            line: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CommandKey {
    pub(crate) feature: String,
    pub(crate) command: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandPolicy {
    pub(crate) reference: String,
    pub(crate) atoms: Vec<String>,
    pub(crate) routes: BTreeMap<String, CommandRouteSlot>,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandRouteSlot {
    pub(crate) bound_from_context: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedCommandTarget {
    pub(crate) key: CommandKey,
    pub(crate) args: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ExperienceFacts {
    pub(crate) view_actions: BTreeMap<String, BTreeMap<String, String>>,
    pub(crate) view_routes: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Debug, Clone)]
struct SourceFact {
    path: PathBuf,
    line: usize,
    column: usize,
    name: String,
}

#[derive(Debug, Default)]
pub(super) struct OperationalFacts {
    pub(super) features: BTreeMap<String, SourceFact>,
    pub(super) integration_requirements: Vec<IntegrationRequirementFact>,
    pub(super) external_calls: Vec<ExternalCallFact>,
    pub(super) env_references: Vec<SourceFact>,
    pub(super) file_capabilities: Vec<SourceFact>,
    /// Row 30 — typed `@cap.File(...)` sites carrying the lowered
    /// `FileCapability` + origin + binding context (`ResourceField` or
    /// `ApiOutput`). Populated alongside `file_capabilities` so the
    /// storage diagnostics can run against typed IR shape, while the
    /// existing text-pattern fact powers the `APP-CAP-001` check.
    pub(super) file_capability_facts: Vec<FileCapabilityFact>,
    pub(super) jobs: Vec<SourceFact>,
    pub(super) schedules: Vec<SourceFact>,
    pub(super) webhooks: Vec<SourceFact>,
    pub(super) apis: Vec<SourceFact>,
    pub(super) web_surfaces: Vec<SourceFact>,
    pub(super) mobile_surfaces: Vec<SourceFact>,
    pub(super) web_routes: Vec<SourceFact>,
    pub(super) mobile_routes: Vec<SourceFact>,
}

#[derive(Debug, Clone)]
struct IntegrationRequirementFact {
    path: PathBuf,
    line: usize,
    column: usize,
    feature: String,
    slot: String,
    contract: String,
}

#[derive(Debug, Clone)]
pub(super) struct ExternalCallFact {
    pub(super) path: PathBuf,
    pub(super) line: usize,
    pub(super) column: usize,
    pub(super) feature: String,
    pub(super) subject_kind: String,
    pub(super) subject: String,
    pub(super) slot: String,
    pub(super) operation: String,
    pub(super) has_timeout: bool,
    pub(super) has_retry: bool,
    pub(super) has_idempotency: bool,
}

/// Row 30 — one typed `@cap.File(...)` site harvested from a `.lzi`
/// source file. `feature` is the enclosing `feature <name>` block;
/// `binding` discriminates between a resource field and an api output
/// so the storage diagnostics can apply context-sensitive rules (e.g.
/// `visibility` is required on api outputs but defaults to `private`
/// on resource fields).
#[derive(Debug, Clone)]
pub(crate) struct FileCapabilityFact {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) feature: String,
    pub(crate) binding: FileCapabilityBinding,
    pub(crate) capability: lazuli_ir::FileCapability,
}

#[derive(Debug, Clone)]
pub(crate) enum FileCapabilityBinding {
    /// `<field>: @cap.File(...)` inside a `resource <Name>` block.
    ResourceField { resource: String, field: String },
    /// `output @cap.File(...)` inside an `api <Name>` block.
    ApiOutput { api: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DoctorSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Rails-style canonical envelope for every doctor finding.
///
/// One row in `lazuli doctor`'s output — the **and only the** shape a
/// diagnostic ever takes once it crosses out of an aggregator's per-rule
/// crate (`lazuli_doctor::correctness`, `lazuli_doctor::vocab`, …) and
/// into the CLI's dispatch pipeline. Every doctor rule, no matter where
/// it lives (this crate, `lazuli_doctor`, `lazuli_lsp`'s LSP bridge,
/// per-rule sibling files under `doctor/auth/`, `doctor/folder/`, …)
/// converts its native finding into this struct so the rest of the
/// system — JSON rendering (`build_doctor_report`), text printing
/// (`print()`), coverage rollup, `--fail-on` gate, dedup — has exactly
/// one type to walk.
///
/// Field-level visibility (Wave 4.3 R2): every field is `pub(crate)`
/// so sibling submodules under `doctor/` (`aggregators/*`,
/// `auth/*`, `route_guard/*`, `lzx/*`, …) can construct the envelope
/// directly. The Wave 0.5 agent-first fields (`category`, `feature_name`,
/// `construct`, `fix`, `group`) stay `Option<>` so every existing call
/// site keeps compiling. New rules populate them when they have the
/// information.
///
/// See `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5 for the
/// agent-first envelope contract.
#[derive(Debug, Clone)]
pub(crate) struct DoctorDiagnostic {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) severity: DoctorSeverity,
    pub(crate) code: String,
    pub(crate) message: String,
    // Wave 0.5 — agent-first JSON parity fields. Additive `Option<>` so
    // existing construction sites stay compiling; new rules and Wave 1+
    // migrations populate them explicitly. See
    // `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5.
    /// Rule taxonomy bucket; resolved via `RuleCategory::from_code_prefix`
    /// when not set explicitly. `None` means "fall back to prefix
    /// derivation at consumption time".
    #[allow(dead_code)]
    pub(crate) category: Option<RuleCategory>,
    /// Feature this diagnostic anchors to, when known (`None` for
    /// project-level / cross-feature / manifest checks).
    #[allow(dead_code)]
    pub(crate) feature_name: Option<String>,
    /// Construct the diagnostic anchors to (`resource Foo`, `command
    /// bar`, `policy baz`, …). Wave 1+ populates from authoring sites.
    #[allow(dead_code)]
    pub(crate) construct: Option<DoctorConstruct>,
    /// Suggested fix, when one exists.
    #[allow(dead_code)]
    pub(crate) fix: Option<DoctorFix>,
    /// JSON rollup grouping (e.g. per-feature, per-rule). Wave 2 populates.
    #[allow(dead_code)]
    pub(crate) group: Option<DoctorGroup>,
}

/// Wave 0.5 skeleton — populated per-rule in Wave 1+.
///
/// Identifies the authoring construct a diagnostic anchors to so the
/// agent-facing JSON can surface "this fires on `resource Post.title`"
/// rather than just a line/column.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DoctorConstruct {
    /// `resource`, `command`, `policy`, `field`, etc.
    pub kind: String,
    /// Construct name (resource name, command name, …).
    pub name: String,
}

/// Wave 0.5 skeleton — populated per-rule in Wave 1+.
///
/// Carries a CLI-applicable fix preview alongside the diagnostic so
/// agents can act on output without a second tool roundtrip. See the
/// "Founding principle: Agent-first CLI parity" section of the
/// tdd-bdd-first proposal.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DoctorFix {
    /// Short fix label (`add tests block`, `rename field`, …).
    pub action: String,
    /// Human-readable preview of the change.
    pub preview: Option<String>,
    /// `true` if the fix can be applied without human review.
    pub auto_applicable: bool,
    /// CLI invocation that would apply the fix.
    pub cli: Option<String>,
}

/// Wave 0.5 skeleton — populated per-rule in Wave 1+.
///
/// JSON rollup grouping key so the agent-facing output can build
/// `summary.by_feature`, `summary.by_rule`, `summary.by_category`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DoctorGroup {
    /// Group key (feature name, rule code, category, …).
    pub key: String,
    /// What kind of group this is (`feature`, `rule`, `category`).
    pub kind: String,
}

impl DoctorDiagnostic {
    fn from_lsp(path: PathBuf, diagnostic: &tower_lsp::lsp_types::Diagnostic) -> Self {
        let severity = match diagnostic.severity {
            Some(DiagnosticSeverity::ERROR) => DoctorSeverity::Error,
            Some(DiagnosticSeverity::WARNING) => DoctorSeverity::Warning,
            Some(DiagnosticSeverity::INFORMATION) => DoctorSeverity::Info,
            Some(DiagnosticSeverity::HINT) => DoctorSeverity::Hint,
            _ => DoctorSeverity::Warning,
        };
        let code = diagnostic
            .code
            .as_ref()
            .map(|code| match code {
                tower_lsp::lsp_types::NumberOrString::String(value) => value.clone(),
                tower_lsp::lsp_types::NumberOrString::Number(value) => value.to_string(),
            })
            .unwrap_or_else(|| "diagnostic".to_owned());

        Self {
            path,
            line: diagnostic.range.start.line as usize + 1,
            column: diagnostic.range.start.character as usize + 1,
            severity,
            code,
            message: diagnostic.message.clone(),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        }
    }

    fn print(&self) {
        let severity = match self.severity {
            DoctorSeverity::Error => "error",
            DoctorSeverity::Warning => "warning",
            DoctorSeverity::Info => "info",
            DoctorSeverity::Hint => "hint",
        };
        println!(
            "{}:{}:{}: {severity} [{}]: {}",
            self.path.display(),
            self.line,
            self.column,
            self.code,
            self.message
        );
    }
}

pub(super) fn collect_package_paths(input: &Path) -> Result<Vec<PathBuf>> {
    if input.is_dir() {
        let mut paths = Vec::new();
        collect_lazuli_paths_recursive(input, &mut paths)?;
        paths.sort();
        return Ok(paths);
    }

    if !input.exists() {
        bail!("{} does not exist", input.display());
    }

    let Some(parent) = input.parent() else {
        return Ok(vec![input.to_path_buf()]);
    };
    let Some(input_package_stem) = package_stem(input) else {
        return Ok(vec![input.to_path_buf()]);
    };

    let mut paths = Vec::new();
    for entry in
        fs::read_dir(parent).with_context(|| format!("failed to list {}", parent.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read {}", parent.display()))?;
        let path = entry.path();
        if path.is_file()
            && (is_lzi_path(&path) || is_lzx_path(&path))
            && package_stem(&path).as_deref() == Some(input_package_stem.as_str())
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

// =============================================================================
// Cut A — cross-feature diagnostics (8 ids per plan §5.2).
//
// Lives downstream of the typed `AgentFacts` collected during package
// load and the per-feature symbol tables populated by
// `collect_feature_symbols`. File-local checks (predicate shape, block
// well-formedness) stay in `crates/lazuli_lsp/src/lib.rs`; this module
// owns the workspace-wide work that the LSP cannot perform.
// =============================================================================

// -----------------------------------------------------------------------------
// Diagnostic id: cross_feature_type_unresolved
// -----------------------------------------------------------------------------

// -----------------------------------------------------------------------------
// Diagnostic id: feature_uses_missing
// -----------------------------------------------------------------------------

// =============================================================================
// RB.B — RBAC catalog diagnostics.
//
// Aggregate the package-level catalog by re-parsing each `.lzi` file
// via `parse_package_skeleton` and folding the permission/role decls.
// Pass through `analyze_rbac_catalog` for closure + cycle issues,
// then layer two package-level checks on top:
//   - RBAC-CATALOG-MISSING-001 (info): `@role.*` references exist but
//     no catalog declared.
//   - RBAC-ROLE-UNDECLARED-001 (error): `@role.X` references a role
//     not in the catalog (when a catalog IS declared).
//   - RBAC-MISSING-POLICY-001 (warning): a feature with ≥2 policied
//     commands has a sibling command/query without explicit policy.
// =============================================================================

// =============================================================================
// Phase L — auth block cross-feature diagnostics.
//
// Auth ids per `docs/proposals/bucket-auth-cycle.md` §Doctor/LSP:
//   - `auth_password_algorithm_hash_mismatch`
//   - `auth_password_no_session`
//   - `auth_sessions_resource_unknown`
//   - `auth_identity_field_unknown`
//   - `auth_oauth_adapter_unbound`
//   - `auth_oauth_no_password_alt`
//   - `auth_session_ttl_too_short`
//
// The lowered `ir::Auth` block arrives via `lower_feature_skeleton`
// (Phase L Tier 1). This module owns the text-pattern collection of
// neighbouring resources + extensions adapter slots and the
// cross-feature lookup that the LSP cannot perform.
// =============================================================================

#[derive(Debug, Default)]
struct AuthAnchors {
    identity_line: usize,
    password_line: Option<usize>,
    password_algorithm_line: Option<usize>,
    sessions_line: Option<usize>,
    sessions_resource_line: Option<usize>,
    mfa_line: Option<usize>,
    oauth_lines: BTreeMap<String, usize>,
}

/// Walk the source under the `auth` block (starting at `auth_line`) and
/// map each subblock onto its 1-based source line. Used to anchor
/// diagnostics at the offending keyword rather than the `auth` header.
pub(super) fn collect_auth_anchors(source: &str, auth_line: usize) -> AuthAnchors {
    let mut anchors = AuthAnchors {
        identity_line: auth_line,
        ..Default::default()
    };
    let lines: Vec<&str> = source.lines().collect();
    if auth_line == 0 || auth_line > lines.len() {
        return anchors;
    }
    // `auth_line` is 1-based; index = auth_line - 1 points at the
    // `auth` keyword. Body starts the next line.
    let header_index = auth_line - 1;
    let auth_indent = leading_spaces(lines[header_index]);
    let child_indent = auth_indent + 2;
    let grand_indent = auth_indent + 4;

    let mut i = header_index + 1;
    let mut current_password = false;
    let mut current_sessions = false;
    let mut current_mfa = false;
    let mut current_oauth: Option<String> = None;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= auth_indent {
            break;
        }
        if indent == child_indent {
            current_password = false;
            current_sessions = false;
            current_mfa = false;
            current_oauth = None;
            if let Some(rest) = trimmed.strip_prefix("identity ") {
                let _ = rest;
                anchors.identity_line = i + 1;
            } else if trimmed == "password" {
                anchors.password_line = Some(i + 1);
                current_password = true;
            } else if trimmed == "sessions" {
                anchors.sessions_line = Some(i + 1);
                current_sessions = true;
            } else if let Some(rest) = trimmed.strip_prefix("mfa ") {
                let _ = rest;
                anchors.mfa_line = Some(i + 1);
                current_mfa = true;
            } else if let Some(rest) = trimmed.strip_prefix("oauth ") {
                let provider = rest.split_whitespace().next().unwrap_or("").to_owned();
                if !provider.is_empty() {
                    anchors.oauth_lines.insert(provider.clone(), i + 1);
                    current_oauth = Some(provider);
                }
            }
        } else if indent == grand_indent {
            if current_password {
                if trimmed.starts_with("algorithm ") {
                    anchors.password_algorithm_line = Some(i + 1);
                }
            } else if current_sessions && trimmed.starts_with("resource ") {
                anchors.sessions_resource_line = Some(i + 1);
            } else if current_mfa || current_oauth.is_some() {
                // body lines for mfa/oauth carry adapter/enroll/verify
                // refs but we don't need per-line anchors today.
            }
        }
        i += 1;
    }
    anchors
}

#[cfg(test)]
mod tests;
