mod aggregators;
pub mod auth;
mod auth_anchors;
pub mod auth_refresh;
mod diagnostic;
mod dispatch;
mod fact_types;
mod facts;
pub mod folder;
mod helpers;
pub mod lifecycle_gate;
pub mod lzx;
mod package;
pub(crate) mod parsers;
pub mod rbac;
mod package_methods;
mod refs;
mod release;
mod report_build;
mod returns_list_001;
mod returns_list_002;
pub mod route_guard;
mod rule_bridges;
mod runtime_options;
mod scanners;
pub mod schema_rich_001;
mod self_audit;

// Wave R7-3 extract — fact-type data structs (Tier3FeatureFacts,
// DoctorApp* family, Auth/Agent/Symbol facts, OperationalFacts, command
// routing facts) moved into `fact_types.rs`. Re-exported so the existing
// in-module call paths (`super::AuthFacts`, `crate::doctor::CommandKey`,
// …) keep compiling.
pub(crate) use fact_types::{
    AgentFacts, AuthFacts, CommandKey, CommandPolicy, CommandRouteSlot, CommandSymbolFact,
    DoctorAppContract, DoctorAppManifest, DoctorAppProfile, DoctorAppRegistry, DoctorAppWorkspace,
    DoctorFile, ExperienceFacts, ExternalCallFact, FeatureSymbols, FieldPreviousFact,
    FileCapabilityBinding, FileCapabilityFact, IntegrationRequirementFact, OperationalFacts,
    RegistryToolDefect, ResolvedCommandTarget, ResourceFact, ResourceFieldFact,
    ResourcePreviousFact, SourceFact, SymbolFact, Tier3FeatureFacts,
};

// Wave R7-3 extract — canonical diagnostic envelope + severity
// machinery moved into `diagnostic.rs`. Re-exported so existing call
// paths (`super::DoctorDiagnostic`, `super::doctor_severity_for`, …)
// keep compiling. `DoctorConstruct` / `DoctorFix` / `DoctorGroup` /
// `DoctorSeverityOverride` remain `pub` in their defining module and
// can be reached via `crate::doctor::diagnostic::*` when needed.
pub(crate) use diagnostic::{
    DoctorDiagnostic, DoctorSeverity, doctor_rule_severity, doctor_severity_for,
};

// Wave R7-3 extract — per-rule bridge helpers (vocab / money /
// codegen-wrap / pattern-draft-stale / auth-session-callsite) moved
// into `rule_bridges.rs`. Re-exported so the dispatcher + sibling
// aggregators keep their existing `super::*` call paths.
pub(super) use rule_bridges::{
    check_auth_session_callsite_001, check_codegen_wrap_001, check_pattern_draft_stale_001,
    check_pattern_draft_stale_001_at, collect_codegen_wrap_001, collect_issue_session_callsites,
    git_blame_author_time, is_bucket_go_source, is_pattern_draft_line,
    money_arithmetic_001_diagnostics, money_compare_001_diagnostics,
    vocab_audit_001_diagnostics, vocab_audit_002_diagnostics,
    vocab_cap_missing_001_diagnostics, vocab_derived_read_001_diagnostics,
    vocab_event_orphan_001_diagnostics, vocab_event_payload_001_diagnostics,
    vocab_event_producer_001_diagnostics, vocab_grammar_form_diagnostics,
    vocab_handler_heavy_001_diagnostics, vocab_json_typed_001_diagnostics,
    vocab_money_multi_currency_001_diagnostics,
    vocab_resource_wide_cluster_001_diagnostics, vocab_shadow_record_001_diagnostics,
    vocab_tests_missing_001_diagnostics, vocab_union_001_diagnostics,
    vocab_union_002_diagnostics,
};
pub(crate) use rule_bridges::{doctor_rule_path, read_design_ir};

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

/// Handler for `lazuli doctor` — runs the full diagnostic surface
/// with the default runtime options.
///
/// Thin wrapper over [`doctor_command_with_options`]; exists so the
/// clap arm in `main.rs` does not need to construct
/// [`DoctorRuntimeOptions`] for the common case.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_lsp::SecurityProfile;
/// use lazuli_cli::doctor::doctor_command;
///
/// // doctor_command(Path::new("."), SecurityProfile::Strict, false, true)?;
/// ```
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

/// Run `lazuli doctor` with a fully-specified
/// [`DoctorRuntimeOptions`] bag.
///
/// This is the load-bearing entry point — every clap-side flag
/// translates into a field of `opts` (release window, swarm mode, JSON
/// vs text, machine context, etc.). The wrapper above just defaults
/// `opts`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_lsp::SecurityProfile;
/// use lazuli_cli::doctor::{doctor_command_with_options, DoctorRuntimeOptions};
///
/// // doctor_command_with_options(
/// //     Path::new("."),
/// //     SecurityProfile::Strict,
/// //     false,
/// //     true,
/// //     DoctorRuntimeOptions::default(),
/// // )?;
/// ```
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

pub(crate) use auth_anchors::{AuthAnchors, collect_auth_anchors};
pub(crate) use report_build::doctor_diagnostics_json;
use report_build::build_doctor_report;

#[cfg(test)]
mod tests;
