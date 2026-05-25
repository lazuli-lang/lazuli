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

// Re-export the `lazurite_manifest` aggregator's dispatcher so
// `doctor/dispatch.rs` keeps its `super::lazurite_manifest_diagnostics`
// call path after the extraction.
pub(crate) use aggregators::lazurite_manifest::lazurite_manifest_diagnostics;

// Re-export the `semantic_type` aggregator's public surface so
// `doctor/package.rs`, `doctor/tests.rs`, `doctor/dispatch.rs`, and
// the sibling aggregators (`error_vocab`, `lifecycle_gate`,
// `route_guard`) keep their `super::*` / `crate::doctor::*` call
// paths.
pub(crate) use aggregators::semantic_type::{
    cross_feature_type_unresolved_diagnostics, feature_uses_missing_diagnostics,
    policy_ref_surface_text, semantic_type_unknown_diagnostics_for_feature,
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
struct CommandKey {
    feature: String,
    command: String,
}

#[derive(Debug, Clone)]
struct CommandPolicy {
    reference: String,
    atoms: Vec<String>,
    routes: BTreeMap<String, CommandRouteSlot>,
}

#[derive(Debug, Clone)]
struct CommandRouteSlot {
    bound_from_context: bool,
}

#[derive(Debug, Clone)]
struct ResolvedCommandTarget {
    key: CommandKey,
    args: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
struct ExperienceFacts {
    view_actions: BTreeMap<String, BTreeMap<String, String>>,
    view_routes: BTreeMap<String, BTreeSet<String>>,
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
struct FileCapabilityFact {
    path: PathBuf,
    line: usize,
    column: usize,
    feature: String,
    binding: FileCapabilityBinding,
    capability: lazuli_ir::FileCapability,
}

#[derive(Debug, Clone)]
enum FileCapabilityBinding {
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

pub(super) fn project_uses_plugin_refs(project_root: &Path) -> bool {
    let mut paths = Vec::new();
    if collect_lazuli_paths_recursive(project_root, &mut paths).is_err() {
        return false;
    }

    paths
        .into_iter()
        .filter(|path| is_lzi_path(path))
        .any(|path| {
            fs::read_to_string(&path)
                .map(|source| !collect_plugin_references_in_source(&path, &source).is_empty())
                .unwrap_or(false)
        })
}

/// LSP emits both `app-env-contract` and `env-schema-contract` on the
/// same line of a `registry.env` block when the env declaration shape is
/// invalid — the `app` and `registry` indent-6 branches both call
/// `validate_app_env_line`, then the dedicated `env-schema-contract`
/// validator runs over the registry pass. Audit ref: R1.C real-world
/// sweep produced 9 duplicates (gamma 7×, delta 2×).
///
/// `env-schema-contract` is the more specific registry-scoped rule and
/// owns the registry env shape; drop the broader `app-env-contract`
/// diagnostic when the same `(path, line)` already carries it.
pub(super) fn dedupe_env_contract_diagnostics(
    diagnostics: &[DoctorDiagnostic],
) -> Vec<DoctorDiagnostic> {
    let env_schema_lines: BTreeSet<(PathBuf, usize)> = diagnostics
        .iter()
        .filter(|d| d.code == "env-schema-contract")
        .map(|d| (d.path.clone(), d.line))
        .collect();

    diagnostics
        .iter()
        .filter(|d| {
            !(d.code == "app-env-contract" && env_schema_lines.contains(&(d.path.clone(), d.line)))
        })
        .cloned()
        .collect()
}

/// LSP emits `env-schema-reference` per file because the per-file rule can't
/// see the registry. Doctor has cross-package visibility (it loads
/// `registry.lzi` and `app.lzi`), so it can suppress those warnings for envs
/// that ARE declared. Closes the false-positive surfaced by the hostpoint
/// pilot port (2026-05-16): `env.MERCADOPAGO_WEBHOOK_SECRET` was correctly
/// declared in `registry.env` but the LSP warning was inherited verbatim.
///
/// Message shape: ``"environment reference `env.<NAME>` should be declared..."``
pub(super) fn suppress_env_schema_when_declared(
    diagnostics: &[DoctorDiagnostic],
    declared_env_names: &BTreeSet<&str>,
) -> Vec<DoctorDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| {
            if d.code != "env-schema-reference" {
                return true;
            }
            // Extract env name from `env.X` in the message.
            let Some(start) = d.message.find("env.") else {
                return true;
            };
            let rest = &d.message[start + "env.".len()..];
            let end = rest.find('`').unwrap_or(rest.len());
            let env_name = &rest[..end];
            !declared_env_names.contains(env_name)
        })
        .cloned()
        .collect()
}

pub(super) fn manifest_required_diagnostics(
    project_root: &Path,
    single_file_input: bool,
) -> Vec<DoctorDiagnostic> {
    // Single-file invocation (`lazuli doctor path/to/file.lzi`) has no
    // project context — the rule walked the parent directory, picked up
    // unrelated sibling fixtures' `@lazuli/plugin-*` refs, and pointed the user
    // at a phantom `Lazurite.toml`. Audit ref: R1.C real-world sweep
    // (12 false positives across standalone fixtures).
    if single_file_input {
        return Vec::new();
    }

    if !project_uses_plugin_refs(project_root) || project_has_lazurite_manifest(project_root) {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: project_root.join("Lazurite.toml"),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "MANIFEST-REQUIRED-001".to_owned(),
        message: "project uses @lazuli/plugin-* references but is missing Lazurite.toml."
            .to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

/// CAP-FILE-POLICY-IMPLICIT (warning) — every `@cap.File` field on
/// a per-user resource should declare an explicit
/// `auto_photo_policy: @policy.<name>`. The analyzer's heuristic
/// fallback (resource-singular + `_only`) produces silent surprises
/// when a feature has multiple matching policies — e.g. both
/// `host_only` and `host_and_operator` — and the wrong one wins.
///
/// Wave §6 (2026-05-23). Severity is Warning today so existing
/// pilots can migrate field-by-field; escalating to Error is
/// gated on every pilot's `@cap.File` sites having explicit
/// policy declarations.
pub(super) fn cap_file_policy_implicit_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for feature in facts {
        for resource in &feature.resources {
            for field in &resource.fields {
                let cap = match &field.type_ref {
                    lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(spec)) => spec,
                    _ => continue,
                };
                if cap.auto_photo_policy.is_some() {
                    continue;
                }
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CAP-FILE-POLICY-IMPLICIT".to_owned(),
                    message: format!(
                        "resource `{}.{}` field `{}` is a `@cap.File(...)` site without an explicit `auto_photo_policy: @policy.<name>`. The analyzer falls back to the resource-singular heuristic (e.g. `host_only`), which silently picks the wrong policy when the feature has multiple matching candidates. Add `auto_photo_policy: @policy.<your_policy>` to the `@cap.File(...)` arglist.",
                        feature.feature, resource.name, field.name,
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
    diagnostics
}

/// MANUAL-PARAM-COERCION (warning) — flags hand-rolled coercion
/// of route params (`Number(params.id)`, `as unknown as number`,
/// `String(params.X)`) in the frontend source tree. Wave §3
/// codegen-emitted typed param parsers are the contract — every
/// hit here is a site that should use the generated parser
/// instead.
///
/// Today the codegen only emits typed params for per-feature
/// view routes; app-level routes (e.g. `app/<frontend>.lzx`)
/// still expose `params: Record<string, string>`. That's the §2
/// router-contract gap. This lint surfaces every consumer that
/// would benefit from closing it.
///
/// Scope: walks `app/clients/<frontend>/src/**/*.{ts,tsx}` only.
/// Skips `*.gen.ts`, `*.test.*`, `node_modules`, `dist`. Severity
/// is Warning today so the migration sweep can land per-file
/// without blocking the doctor gate.
pub(super) fn manual_param_coercion_diagnostics(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let clients_root = project_root.join("app").join("clients");
    if !clients_root.exists() {
        return diagnostics;
    }

    // Wave §2 sweep tightening (2026-05-24): only flag coercion of
    // variables literally named `params` / `rawParams` (the canonical
    // useParams() return value names). Iteration vars (`p`, `item`,
    // `entry`) that happen to access `.id` are NOT route params and
    // were producing false positives.
    const ID_PARAMS: &[&str] = &[
        "params.id",
        "params.propertyId",
        "params.serviceId",
        "params.threadId",
        "params.chatId",
        "params.userId",
        "params.hostId",
        "params.travelerId",
        "rawParams.id",
        "rawParams.propertyId",
        "rawParams.serviceId",
        "rawParams.threadId",
        "rawParams.chatId",
        "rawParams.userId",
        "rawParams.hostId",
        "rawParams.travelerId",
    ];

    walk_frontend_ts_files(&clients_root, &mut |path, contents| {
        for (lineno, line) in contents.lines().enumerate() {
            // Cheap pre-filter: skip lines that can't contain any pattern.
            let has_number = line.contains("Number(");
            let has_cast = line.contains("as unknown as number");
            let has_string = line.contains("String(");
            if !has_number && !has_cast && !has_string {
                continue;
            }
            // Skip the lint's own comment lines and existing
            // codegen workaround banners — they MENTION the pattern
            // but aren't violations.
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") || trimmed.starts_with("/*") {
                continue;
            }
            let matches_param = ID_PARAMS.iter().any(|p| line.contains(p));
            let kind = if matches_param && has_cast {
                Some("as unknown as number on params.X")
            } else if has_number && matches_param {
                Some("Number(params.X)")
            } else if has_string && matches_param {
                Some("String(params.X)")
            } else {
                None
            };
            let Some(kind) = kind else { continue };
            diagnostics.push(DoctorDiagnostic {
                path: path.to_path_buf(),
                line: lineno + 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "MANUAL-PARAM-COERCION".to_owned(),
                message: format!(
                    "manual route-param coercion ({kind}) — wave §2 typed param parsers should land here instead. Use the generated `parse<Route>Params(rawParams)` factory from `dist/ts-<surface>/<audience>/routes.gen.tsx`."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    });

    diagnostics
}

/// IMPORT-DEPRECATED-ALIAS (warning) — flags consumer imports of
/// SDK exports marked `@deprecated` in the generated TS code.
/// Codegen emits backward-compat aliases for every rename
/// (`listMinePropertiesCatalogs` → `listMineProperties`,
/// `listMinePropertiesUploadedAssets` → `listMineProperties`); the
/// alias lives for one cycle to give consumers time to migrate,
/// then gets removed. This lint catches the consumer half so the
/// removal lands without dangling references.
///
/// Wave §3 (2026-05-23). Severity is Warning — informational; the
/// alias still resolves at runtime. Escalation to Error happens
/// when each removal is planned (consumer fixes its import +
/// runtime drops the alias in the same release).
pub(super) fn import_deprecated_alias_diagnostics(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let dist_root = project_root.join("dist");
    let clients_root = project_root.join("app").join("clients");
    if !dist_root.exists() || !clients_root.exists() {
        return diagnostics;
    }

    let mut deprecated_exports: BTreeMap<String, PathBuf> = BTreeMap::new();
    collect_deprecated_exports(&dist_root, &mut deprecated_exports);
    if deprecated_exports.is_empty() {
        return diagnostics;
    }

    walk_frontend_ts_files(&clients_root, &mut |path, contents| {
        // Cheap pre-filter: only inspect lines inside an import statement.
        let mut in_import = false;
        for (lineno, line) in contents.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("import ") || trimmed.starts_with("import{") {
                in_import = true;
            }
            if !in_import {
                continue;
            }
            for name in deprecated_exports.keys() {
                // Word-boundary match — guard against substring false positives
                // (e.g. `listMinePropertiesV2` would otherwise fire on the
                // shorter `listMineProperties`).
                if !matches_word(line, name) {
                    continue;
                }
                diagnostics.push(DoctorDiagnostic {
                    path: path.to_path_buf(),
                    line: lineno + 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "IMPORT-DEPRECATED-ALIAS".to_owned(),
                    message: format!(
                        "import of deprecated SDK alias `{name}`. The generated `.gen.ts` declares it `@deprecated`; switch to the canonical export before the alias is removed in the next codegen cycle."
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            if line.contains("from ") {
                in_import = false;
            }
        }
    });

    diagnostics
}

/// SCHEMA-RICH-GAP (hint) — flags resource fields declared as
/// opaque `JSON` whose name strongly suggests a richer typed
/// shape (`*_photos`, `*_files`, `*_attachments`, `*_images`,
/// `*_documents`, `*_assets`). The codegen emits these as
/// `unknown` in TS + `any` shape in Zod schemas, which forces
/// every consumer to cast or re-validate by hand.
///
/// Wave §5a (2026-05-23) — the gap count is a metric of compiler
/// expressiveness. Closing each hit means either (a) authoring
/// the field as `@cap.File[]` / `@cap.AttachmentRef[]` to lift
/// the shape, or (b) accepting it as inevitable JSON and
/// silencing this lint via a future `@opaque` annotation.
///
/// Hint severity (informational) — does not fail the doctor
/// gate. The check is deliberately conservative (name-based
/// heuristic) so false positives are easy to triage.
pub(super) fn schema_rich_gap_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    const AVOIDABLE_SUFFIXES: &[&str] = &[
        "_photos",
        "_files",
        "_attachments",
        "_images",
        "_documents",
        "_assets",
    ];
    let mut diagnostics = Vec::new();
    for feature in facts {
        for resource in &feature.resources {
            for field in &resource.fields {
                let is_json = matches!(
                    field.type_ref,
                    lazuli_ir::TypeRef::Builtin(lazuli_ir::BuiltinType::Json)
                );
                if !is_json {
                    continue;
                }
                let suggests_files = AVOIDABLE_SUFFIXES
                    .iter()
                    .any(|suffix| field.name.ends_with(suffix));
                if !suggests_files {
                    continue;
                }
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Hint,
                    code: "SCHEMA-RICH-GAP".to_owned(),
                    message: format!(
                        "resource `{}.{}` field `{}` is declared as opaque `JSON` but its name suggests a typed array of files/attachments. Consider lifting to `@cap.File[]` (or `@cap.AttachmentRef[]`) so the codegen emits a specific TS type + Zod schema instead of `unknown`/`z.any()`. If the field is genuinely opaque JSON, this hint can be ignored (the future `@opaque` annotation will silence it).",
                        feature.feature, resource.name, field.name,
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
    diagnostics
}

pub(super) fn lazuli_version_001_diagnostics(
    app: Option<&DoctorAppManifest>,
    schema: &str,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else { return Vec::new() };
    let current_major_minor = major_minor(schema);

    match app.manifest.lazuli_version.as_deref() {
        None => {
            let severity = if is_one_dot_zero_plus(schema) {
                DoctorSeverity::Error
            } else {
                DoctorSeverity::Warning
            };
            vec![DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity,
                code: "LAZULI-VERSION-001".to_owned(),
                message: format!(
                    "lazuli_version pin missing. Expected: lazuli_version \"{}\". Add this to app.lzi to lock the runtime/IR ABI version.",
                    current_major_minor
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }]
        }
        Some(pinned) => {
            let pinned_major_minor = major_minor(pinned);
            if pinned_major_minor == current_major_minor {
                Vec::new()
            } else {
                vec![DoctorDiagnostic {
                    path: app.path.clone(),
                    line: lazuli_version_line(&app.source).unwrap_or(1),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "LAZULI-VERSION-001".to_owned(),
                    message: format!(
                        "lazuli_version pin \"{}\" does not match current LZIR_SCHEMA \"{}\". Run: lazuli upgrade --from {} --to {} <project>. See migrations/recipes/{}-to-{}/.",
                        pinned,
                        schema,
                        pinned_major_minor,
                        current_major_minor,
                        pinned_major_minor,
                        current_major_minor
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }]
            }
        }
    }
}

pub(super) fn lazuli_version_002_diagnostics(
    app: Option<&DoctorAppManifest>,
    schema: &str,
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else { return Vec::new() };
    let Some(pinned) = app.manifest.lazuli_version.as_deref() else {
        return Vec::new();
    };
    let pinned_major_minor = major_minor(pinned);
    let current_major_minor = major_minor(schema);
    if pinned_major_minor == current_major_minor {
        return Vec::new();
    }

    let recipe_dir = project_root
        .join("migrations/recipes")
        .join(format!("{}-to-{}", pinned_major_minor, current_major_minor));
    if recipe_dir.exists() {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: app.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "LAZULI-VERSION-002".to_owned(),
        message: format!(
            "lazuli_version pin \"{}\" has no migration recipe to current \"{}\". No recipe directory at {}. This may indicate a stale pin or a release that shipped without a recipe - file an issue.",
            pinned,
            schema,
            recipe_dir.display()
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

pub(super) fn policy_reachability_diagnostics(
    files: &[DoctorFile],
    experiences: &BTreeMap<String, ExperienceFacts>,
    commands: &BTreeMap<CommandKey, CommandPolicy>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for file in files {
        let Some(document) = file.lzx.as_ref() else {
            continue;
        };

        for surface in &document.surfaces {
            let experience_name = surface
                .uses_experience
                .as_deref()
                .unwrap_or(surface.experience.as_str());
            let experience = experiences.get(experience_name);

            for audience in &surface.audiences {
                for view in &audience.views {
                    if let Some(submit) = view.submit.as_deref() {
                        if let Some(target) = resolve_command_target(submit, &surface.experience) {
                            diagnostics.extend(command_reachability_diagnostic(
                                file,
                                view,
                                &audience.name,
                                &audience.qualifiers,
                                "submit",
                                &target.key,
                                commands,
                            ));
                            diagnostics.extend(command_route_binding_diagnostics(
                                file,
                                view,
                                experience.and_then(|facts| facts.view_routes.get(&view.name)),
                                "submit",
                                &target,
                                commands,
                            ));
                        }
                    }

                    for action in &view.actions {
                        let target = resolve_platform_action_target(
                            action,
                            &surface.experience,
                            experience.and_then(|facts| facts.view_actions.get(&view.name)),
                        );
                        if let Some(target) = target {
                            diagnostics.extend(command_reachability_diagnostic(
                                file,
                                view,
                                &audience.name,
                                &audience.qualifiers,
                                "action",
                                &target.key,
                                commands,
                            ));
                            diagnostics.extend(command_route_binding_diagnostics(
                                file,
                                view,
                                experience.and_then(|facts| facts.view_routes.get(&view.name)),
                                "action",
                                &target,
                                commands,
                            ));
                        }
                    }
                }
            }
        }
    }

    diagnostics
}

pub(super) fn missing_policy_on_query_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    for fact in facts {
        for finding in correctness::missing_policy_on_query_001::check_queries(
            &fact.feature,
            fact.defaults_policy.as_ref(),
            &fact.queries,
            &fact.path,
        ) {
            let line = fact
                .query_lines
                .get(&finding.query_name)
                .copied()
                .unwrap_or(fact.feature_line);
            if !seen.insert((
                finding.path.clone(),
                finding.feature.clone(),
                finding.query_kind,
                finding.query_name.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: correctness::missing_policy_on_query_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

pub(super) fn duplicate_query_name_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for fact in facts {
        for finding in correctness::duplicate_query_name::check_queries(
            &fact.feature,
            &fact.queries,
            &fact.path,
        ) {
            let line = fact
                .query_lines
                .get(&finding.query_name)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::duplicate_query_name::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

/// ROUTE-ID-UNUSED-IN-EFFECT-001 — pair to the LAZ-route-id-codegen-go
/// guard. Walks each feature's lifted commands and fires when a
/// `route <name>: <Type>` slot on an `updates` / `deletes` effect has
/// no matching input slot to back the codegen's `FromInput(...)`
/// binding. Anchored at the command header via `command_lines`.
pub(super) fn route_id_effect_consistency_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    for fact in facts {
        for finding in correctness::route_id_effect_consistency::check_commands(
            &fact.feature,
            &fact.commands,
            &fact.path,
        ) {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or(fact.feature_line);
            if !seen.insert((
                finding.path.clone(),
                finding.feature.clone(),
                finding.command.clone(),
                finding.param_name.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: correctness::route_id_effect_consistency::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

/// MUTATION-WITHOUT-READBACK-001 dispatch — codegen-correctness cycle
/// 2026-05-21 cell A6. Pairs each mutating command (`creates` / `updates`
/// / `deletes`) with the set of `query.lookup` / `query.list` shapes
/// across ALL features (cross-feature read queries count, per cycle
/// decision). Anchored at the command header line; falls back to the
/// feature header when the command line is unknown.
pub(super) fn mutation_without_readback_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    // Pre-build the cross-feature read-query index once. Each fact's
    // own queries get re-included via `check_from_facts`, which dedupes
    // against the current feature name to avoid double-counting.
    let neighbor_queries: Vec<(String, &[lazuli_ir::Query])> = facts
        .iter()
        .map(|fact| (fact.feature.clone(), fact.queries.as_slice()))
        .collect();

    for fact in facts {
        for finding in correctness::mutation_without_readback::check_from_facts(
            &fact.feature,
            &fact.commands,
            &fact.queries,
            &neighbor_queries,
            &fact.path,
        ) {
            let line = fact
                .command_lines
                .get(&finding.command)
                .copied()
                .unwrap_or(fact.feature_line);
            if !seen.insert((
                finding.path.clone(),
                finding.feature.clone(),
                finding.command.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: correctness::mutation_without_readback::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

/// UPDATES-MISSING-UPDATED-AT-001 dispatch — any local resource touched by
/// an `updates` effect must either declare `updated_at: DateTime` or have
/// effective timestamps enabled. Anchored at the feature header because the
/// finding is resource-scoped and the current fact row does not carry
/// resource-header line anchors.
pub(super) fn updates_missing_updated_at_diagnostics(
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();

    for fact in facts {
        let mut feature = aggregators::correctness::make_synthetic_feature_for_correctness(fact);
        feature.defaults.timestamps = fact.defaults_timestamps;

        for finding in correctness::updates_missing_updated_at::check(&feature, &fact.path) {
            if !seen.insert((
                finding.path.clone(),
                finding.feature.clone(),
                finding.resource.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: correctness::updates_missing_updated_at::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

pub(super) fn app_binding_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut requirement_index = BTreeMap::new();

    for requirement in &operational.integration_requirements {
        requirement_index.insert(
            (requirement.feature.as_str(), requirement.slot.as_str()),
            requirement.contract.as_str(),
        );

        let matching_binding = app.manifest.bindings.iter().find(|binding| {
            binding.target_feature == requirement.feature && binding.target_slot == requirement.slot
        });

        if matching_binding.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: requirement.path.clone(),
                line: requirement.line,
                column: requirement.column,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-001".to_owned(),
                message: format!(
                    "feature `{}` requires integration slot `{}`: `{}`, but app manifest does not bind `{}.{}`.",
                    requirement.feature,
                    requirement.slot,
                    requirement.contract,
                    requirement.feature,
                    requirement.slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for (feature, slot, contract) in enabled_pack_integration_requirements(&app.manifest, registry)
    {
        requirement_index.insert((feature, slot), contract);

        let matching_binding = app
            .manifest
            .bindings
            .iter()
            .find(|binding| binding.target_feature == feature && binding.target_slot == slot);

        if matching_binding.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-001".to_owned(),
                message: format!(
                    "enabled pack `{feature}` requires integration slot `{slot}`: `{contract}`, but app manifest does not bind `{feature}.{slot}`.",
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    let integrations = operational_integrations(&app.manifest, registry);

    for binding in &app.manifest.bindings {
        let target = (
            binding.target_feature.as_str(),
            binding.target_slot.as_str(),
        );
        let Some(expected_contract) = requirement_index.get(&target).copied() else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-BIND-005".to_owned(),
                message: format!(
                    "app binding `{}.{}` has no matching feature requirement.",
                    binding.target_feature, binding.target_slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        let Some(integration_name) = integration_source_name(&binding.source) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-002".to_owned(),
                message: format!(
                    "app binding `{}.{}` points to `{}`, but bindings must use `integrations.<name>` or `registry.integrations.<name>`.",
                    binding.target_feature, binding.target_slot, binding.source
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        let Some(actual_contract) = integrations.get(integration_name) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-003".to_owned(),
                message: format!(
                    "app binding `{}.{}` references integration `{integration_name}`, but no app/registry integration with that name exists.",
                    binding.target_feature, binding.target_slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        if *actual_contract != expected_contract {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-BIND-004".to_owned(),
                message: format!(
                    "app binding `{}.{}` expects `{expected_contract}`, but integration `{integration_name}` is `{actual_contract}`.",
                    binding.target_feature, binding.target_slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

// -----------------------------------------------------------------------------
// Phase L Tier 3 — jobs bucket cycle (row 33) — six IR-driven diagnostics.
// -----------------------------------------------------------------------------

/// Phase L Tier 3 — EVENTGROUP-NESTING-001 + the pattern-prefix
/// promotion (row 34). Two rules over `event_group`:
///
/// - **NESTING-001**: an event group must not contain another event
///   group. Non-canonical authoring; the LSP catches it file-local
///   today but the IR lift now surfaces it cross-feature.
/// - **pattern-prefix**: every concrete event authored under a group
///   must share the pattern prefix (`customer_*` → events must start
///   with `customer_`). Promoted from the LSP `event_group_can_own_
///   short_event_declarations` rule.

pub(super) fn profile_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    profiles: &[DoctorAppProfile],
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let app_environments: BTreeSet<_> = app
        .manifest
        .environments
        .iter()
        .map(String::as_str)
        .collect();
    let integrations = operational_integrations(&app.manifest, registry);
    let mut requirement_index = BTreeMap::new();
    for requirement in &operational.integration_requirements {
        requirement_index.insert(
            (requirement.feature.as_str(), requirement.slot.as_str()),
            requirement.contract.as_str(),
        );
    }
    for (feature, slot, contract) in enabled_pack_integration_requirements(&app.manifest, registry)
    {
        requirement_index.insert((feature, slot), contract);
    }

    for profile in profiles {
        if !app_environments.contains(profile.profile.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: profile.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "PROFILE-001".to_owned(),
                message: format!(
                    "profile `{}` is not declared in app `environments`.",
                    profile.profile.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        for url in &profile.profile.urls {
            if !profile_url_target_valid(&app.manifest, &url.target) {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-URL-001".to_owned(),
                    message: format!(
                        "profile `{}` declares URL target `{}`, but app targets do not expose that target.",
                        profile.profile.name, url.target
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        for integration in &profile.profile.integrations {
            let Some(kind) = integrations.get(integration.name.as_str()).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-INT-001".to_owned(),
                    message: format!(
                        "profile `{}` overrides integration `{}`, but no app/registry integration with that name exists.",
                        profile.profile.name, integration.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };

            if let Some(environment) = &integration.environment
                && !integration_environment_allowed(
                    &app.manifest,
                    registry,
                    &integration.name,
                    environment,
                )
            {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-INT-002".to_owned(),
                    message: format!(
                        "profile `{}` selects `{}` environment `{environment}`, but `{}` does not list that environment.",
                        profile.profile.name, kind, integration.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        for binding in &profile.profile.bindings {
            let target = (
                binding.target_feature.as_str(),
                binding.target_slot.as_str(),
            );
            let Some(expected_contract) = requirement_index.get(&target).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PROFILE-BIND-001".to_owned(),
                    message: format!(
                        "profile `{}` overrides binding `{}.{}`, but that feature slot has no requirement.",
                        profile.profile.name, binding.target_feature, binding.target_slot
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };

            let Some(integration_name) = integration_source_name(&binding.source) else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-002".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` points to `{}`, but profile bindings must use `integrations.<name>` or `registry.integrations.<name>`.",
                        profile.profile.name,
                        binding.target_feature,
                        binding.target_slot,
                        binding.source
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };

            let Some(actual_contract) = integrations.get(integration_name).copied() else {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-003".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` references integration `{integration_name}`, but no app/registry integration with that name exists.",
                        profile.profile.name, binding.target_feature, binding.target_slot
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };

            if actual_contract != expected_contract {
                diagnostics.push(DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-BIND-004".to_owned(),
                    message: format!(
                        "profile `{}` binding `{}.{}` expects `{expected_contract}`, but integration `{integration_name}` is `{actual_contract}`.",
                        profile.profile.name, binding.target_feature, binding.target_slot
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

    diagnostics
}

pub(super) fn operational_integrations<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeMap<&'a str, &'a str> {
    let mut integrations = BTreeMap::new();
    for integration in &app.integrations {
        integrations.insert(integration.name.as_str(), integration.kind.as_str());
    }
    if let Some(registry) = registry {
        for integration in &registry.manifest.integrations {
            integrations.insert(integration.name.as_str(), integration.kind.as_str());
        }
    }
    integrations
}

pub(super) fn app_pack_contract_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let integrations = operational_integrations(&app.manifest, registry);

    for pack_use in &app.manifest.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-PACK-001".to_owned(),
                message: format!(
                    "app pack `{}` points to `{}`, but packs must use `packs.<name>` or `registry.packs.<name>`.",
                    pack_use.name, pack_use.source
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        let Some(pack) = registry.and_then(|registry| {
            registry
                .manifest
                .packs
                .iter()
                .find(|pack| pack.name == pack_name)
        }) else {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-PACK-002".to_owned(),
                message: format!(
                    "app pack `{}` references registry pack `{pack_name}`, but no such pack exists in `registry.lzi`.",
                    pack_use.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
            continue;
        };

        for requirement in &pack.requirements {
            if requirement.kind == "integration"
                && !integrations
                    .values()
                    .any(|contract| *contract == requirement.contract)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: app.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "APP-PACK-003".to_owned(),
                    message: format!(
                        "enabled pack `{}` requires integration `{}`: `{}`, but app/registry declares no integration with that contract.",
                        pack_use.name, requirement.name, requirement.contract
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

    diagnostics
}

pub(super) fn adapter_provenance_diagnostics(
    app: &DoctorAppManifest,
    registry: Option<&DoctorAppRegistry>,
    profiles: &[DoctorAppProfile],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for integration in &app.manifest.integrations {
        if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
            diagnostics.push(adapter_source_diagnostic(
                app.path.clone(),
                "APP-ADAPTER-001",
                &integration.name,
                integration.adapter.as_deref().unwrap_or_default(),
            ));
        }
    }

    if let Some(registry) = registry {
        for integration in &registry.manifest.integrations {
            if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
                diagnostics.push(adapter_source_diagnostic(
                    registry.path.clone(),
                    "REG-ADAPTER-001",
                    &integration.name,
                    integration.adapter.as_deref().unwrap_or_default(),
                ));
            }
        }
    }

    for profile in profiles {
        for integration in &profile.profile.integrations {
            if integration.adapter.is_some() && integration.adapter_provenance.is_none() {
                diagnostics.push(adapter_source_diagnostic(
                    profile.path.clone(),
                    "PROFILE-ADAPTER-001",
                    &integration.name,
                    integration.adapter.as_deref().unwrap_or_default(),
                ));
            }
        }
    }

    diagnostics
}

pub(super) fn adapter_source_diagnostic(
    path: PathBuf,
    code: &str,
    integration_name: &str,
    adapter: &str,
) -> DoctorDiagnostic {
    DoctorDiagnostic {
        path,
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: code.to_owned(),
        message: format!(
            "integration `{integration_name}` uses adapter `{adapter}`, but adapter sources must declare provenance with `@runtime/...`, `@lazuli/plugin-<name>` (or `@lazuli/plugin-<publisher>/<name>`), `@adapter.<local>`, or a local path."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }
}

pub(super) fn enabled_pack_provided_features<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeSet<&'a str> {
    let mut features = BTreeSet::new();
    let Some(registry) = registry else {
        return features;
    };

    for pack_use in &app.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            continue;
        };
        let Some(pack) = registry
            .manifest
            .packs
            .iter()
            .find(|pack| pack.name == pack_name)
        else {
            continue;
        };

        for provide in &pack.provides {
            if provide.kind == "feature" {
                features.insert(provide.name.as_str());
            }
        }
    }

    features
}

pub(super) fn enabled_pack_integration_requirements<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> Vec<(&'a str, &'a str, &'a str)> {
    let mut requirements = Vec::new();
    let Some(registry) = registry else {
        return requirements;
    };

    for pack_use in &app.packs {
        let Some(pack_name) = pack_source_name(&pack_use.source) else {
            continue;
        };
        let Some(pack) = registry
            .manifest
            .packs
            .iter()
            .find(|pack| pack.name == pack_name)
        else {
            continue;
        };

        for requirement in &pack.requirements {
            if requirement.kind == "integration" {
                requirements.push((
                    pack_use.name.as_str(),
                    requirement.name.as_str(),
                    requirement.contract.as_str(),
                ));
            }
        }
    }

    requirements
}

pub(super) fn integration_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))
}

pub(super) fn pack_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("packs.")
        .or_else(|| source.strip_prefix("registry.packs."))
}

pub(super) fn integration_environment_allowed(
    app: &AppManifest,
    registry: Option<&DoctorAppRegistry>,
    name: &str,
    environment: &str,
) -> bool {
    app.integrations
        .iter()
        .chain(
            registry
                .into_iter()
                .flat_map(|registry| registry.manifest.integrations.iter()),
        )
        .find(|integration| integration.name == name)
        .is_some_and(|integration| {
            integration.environments.is_empty()
                || integration
                    .environments
                    .iter()
                    .any(|allowed| allowed == environment)
        })
}

pub(super) fn app_service_contract_diagnostics(
    app: &DoctorAppManifest,
    operational: &OperationalFacts,
    pack_features: &BTreeSet<&str>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

    for service in &app.manifest.services {
        for owned in &service.owns {
            owners
                .entry(owned.as_str())
                .or_default()
                .push(service.name.as_str());
        }

        for exposure in &service.exposes {
            if let Some(feature_name) = exposure.target.split('.').next()
                && !service.owns.iter().any(|owned| owned == feature_name)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: app.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "APP-SVC-003".to_owned(),
                    message: format!(
                        "service `{}` exposes `{}` from feature `{feature_name}`, but does not own that feature.",
                        service.name, exposure.target
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

    for feature in operational.features.values() {
        match owners.get(feature.name.as_str()) {
            Some(service_names) if service_names.len() == 1 => {}
            Some(service_names) => diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "APP-SVC-001".to_owned(),
                message: format!(
                    "feature `{}` is owned by multiple app services: {}.",
                    feature.name,
                    service_names.join(", ")
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
            None => diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-SVC-002".to_owned(),
                message: format!(
                    "feature `{}` is not assigned to any app service boundary.",
                    feature.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
        }
    }

    for owned in owners.keys() {
        if !operational.features.contains_key(*owned) && !pack_features.contains(*owned) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-SVC-004".to_owned(),
                message: format!(
                    "app service owns `{owned}`, but no local feature with that name was found in this package."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

pub(super) fn app_has_target(app: &AppManifest, target: &str) -> bool {
    app.targets
        .iter()
        .any(|entry| entry.split_whitespace().next() == Some(target))
}

pub(super) fn profile_url_target_valid(app: &AppManifest, target: &str) -> bool {
    target == "api" && app_has_target(app, "backend") || app_has_target(app, target)
}

pub(super) fn app_has_url(app: &AppManifest, profiles: &[DoctorAppProfile], target: &str) -> bool {
    app.urls.iter().any(|url| url.target == target)
        || profiles
            .iter()
            .flat_map(|profile| profile.profile.urls.iter())
            .any(|url| url.target == target)
}

pub(super) fn operational_env_names<'a>(
    app: &'a AppManifest,
    registry: Option<&'a DoctorAppRegistry>,
) -> BTreeSet<&'a str> {
    let mut names: BTreeSet<_> = app.env.iter().map(|env| env.name.as_str()).collect();
    if let Some(registry) = registry {
        names.extend(registry.manifest.env.iter().map(|env| env.name.as_str()));
    }
    names
}

/// Collect every `object_storage` capability concrete-name declared by
/// the app manifest or registry. Capability lines parse as
/// `<kind> <name>` where the parser stores kind in `AppCapability.name`
/// and the concrete name in `AppCapability.value`. This helper returns
/// the list of concrete names (e.g. `files`) for every entry whose kind
/// is `object_storage` or `storage`. Used by report-vocab doctor rules
/// (`REPORT-SIGNED-NO-STORAGE-001` / `REPORT-STORAGE-AMBIGUOUS-001`)
/// to resolve implicit `storage` bindings and reject signed reports
/// in packages without any object-storage capability.
pub(super) fn collect_object_storage_caps(
    app: Option<&AppManifest>,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    if let Some(app) = app {
        for cap in &app.capabilities {
            if cap.name == "object_storage" || cap.name == "storage" {
                names.push(cap.value.clone());
            }
        }
    }
    if let Some(registry) = registry {
        for cap in &registry.manifest.capabilities {
            if cap.name == "object_storage" || cap.name == "storage" {
                names.push(cap.value.clone());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

pub(super) fn app_has_any_capability(
    app: &AppManifest,
    registry: Option<&DoctorAppRegistry>,
    names: &[&str],
) -> bool {
    app.capabilities
        .iter()
        .any(|capability| names.contains(&capability.name.as_str()))
        || registry.is_some_and(|registry| {
            registry
                .manifest
                .capabilities
                .iter()
                .any(|capability| names.contains(&capability.name.as_str()))
        })
}

pub(super) fn app_runtime_serves(app: &AppManifest, service: &str) -> bool {
    app.runtime
        .iter()
        .flat_map(|unit| unit.serves.iter())
        .any(|item| runtime_item_matches(item, service))
}

pub(super) fn app_runtime_runs(app: &AppManifest, service: &str) -> bool {
    app.runtime
        .iter()
        .flat_map(|unit| unit.runs.iter())
        .any(|item| runtime_item_matches(item, service))
}

pub(super) fn runtime_item_matches(item: &str, service: &str) -> bool {
    item == "*"
        || item == service
        || item
            .split_whitespace()
            .next()
            .is_some_and(|first| first == service)
}

pub(super) fn command_reachability_diagnostic(
    file: &DoctorFile,
    view: &LzxPlatformView,
    audience: &str,
    qualifiers: &[String],
    source_kind: &str,
    target: &CommandKey,
    commands: &BTreeMap<CommandKey, CommandPolicy>,
) -> Vec<DoctorDiagnostic> {
    let (line, column) = line_col_for_offset(&file.source, view.span.start);
    let Some(policy) = commands.get(target) else {
        return vec![DoctorDiagnostic {
            path: file.path.clone(),
            line,
            column,
            severity: DoctorSeverity::Warning,
            code: "LZX-POL-002".to_owned(),
            message: format!(
                "{source_kind} targets unresolved command `{}.command.{}`; doctor could not prove policy reachability.",
                target.feature, target.command
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        }];
    };

    if audience_can_reach_policy(audience, qualifiers, &policy.atoms) {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: file.path.clone(),
        line,
        column,
        severity: DoctorSeverity::Error,
        code: "LZX-POL-001".to_owned(),
        message: format!(
            "audience `{audience}` {source_kind} reaches `{}.command.{}`, but its policy `{}` resolves to {}; change the surface target or expose a command policy reachable by this audience.",
            target.feature,
            target.command,
            policy.reference,
            if policy.atoms.is_empty() {
                "no known atoms".to_owned()
            } else {
                policy.atoms.join(", ")
            }
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

pub(super) fn command_route_binding_diagnostics(
    file: &DoctorFile,
    view: &LzxPlatformView,
    view_routes: Option<&BTreeSet<String>>,
    source_kind: &str,
    target: &ResolvedCommandTarget,
    commands: &BTreeMap<CommandKey, CommandPolicy>,
) -> Vec<DoctorDiagnostic> {
    let Some(command) = commands.get(&target.key) else {
        return Vec::new();
    };
    let missing: Vec<_> = command
        .routes
        .iter()
        .filter(|(name, slot)| {
            !slot.bound_from_context
                && !target.args.contains(*name)
                && !view_routes.is_some_and(|routes| routes.contains(*name))
        })
        .map(|(name, _)| name.clone())
        .collect();

    if missing.is_empty() {
        return Vec::new();
    }

    let (line, column) = line_col_for_offset(&file.source, view.span.start);
    vec![DoctorDiagnostic {
        path: file.path.clone(),
        line,
        column,
        severity: DoctorSeverity::Error,
        code: "LZX-ROUTE-001".to_owned(),
        message: format!(
            "{source_kind} reaches `{}.command.{}` but does not bind required command route slot(s) {}; pass them in the target call or bind the command route from context.",
            target.key.feature,
            target.key.command,
            missing.join(", ")
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

pub(super) fn resolve_platform_action_target(
    action: &str,
    default_feature: &str,
    abstract_actions: Option<&BTreeMap<String, String>>,
) -> Option<ResolvedCommandTarget> {
    if let Some((_, target)) = action.split_once("->") {
        return resolve_command_target(target.trim(), default_feature);
    }
    if let Some(target) = resolve_command_target(action, default_feature) {
        return Some(target);
    }
    let target = abstract_actions?.get(action)?;
    resolve_command_target(target, default_feature)
}

pub(super) fn resolve_command_target(
    target: &str,
    default_feature: &str,
) -> Option<ResolvedCommandTarget> {
    let target = target.trim();
    let (callee, args) = split_target_call(target);

    if let Some(command) = callee.strip_prefix("command.") {
        return Some(ResolvedCommandTarget {
            key: CommandKey {
                feature: default_feature.to_owned(),
                command: command.to_owned(),
            },
            args,
        });
    }

    let parts: Vec<_> = callee.split('.').collect();
    match parts.as_slice() {
        [feature, "command", command] => Some(ResolvedCommandTarget {
            key: CommandKey {
                feature: (*feature).to_owned(),
                command: (*command).to_owned(),
            },
            args,
        }),
        _ => None,
    }
}

pub(super) fn split_target_call(target: &str) -> (&str, BTreeSet<String>) {
    let Some((callee, rest)) = target.split_once('(') else {
        return (target, BTreeSet::new());
    };
    let args = rest
        .trim_end_matches(')')
        .split(',')
        .filter_map(|arg| {
            arg.split_once(':')
                .or_else(|| arg.split_once('='))
                .map(|(name, _)| name.trim())
        })
        .filter(|name| is_identifier(name))
        .map(str::to_owned)
        .collect();
    (callee.trim(), args)
}

pub(super) fn parse_integration_requirement(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.trim().strip_prefix("integration ")?;
    let (slot, contract) = rest.split_once(':')?;
    let slot = slot.trim();
    let contract = contract.trim();

    if is_identifier(slot) && is_type_name(contract) {
        Some((slot, contract))
    } else {
        None
    }
}

pub(super) fn route_slot_name(route: &str) -> Option<&str> {
    route
        .split_once(':')
        .map(|(name, _)| name.trim())
        .or_else(|| route.split_whitespace().next())
        .filter(|name| is_identifier(name))
}

pub(super) fn audience_can_reach_policy(
    audience: &str,
    qualifiers: &[String],
    atoms: &[String],
) -> bool {
    if atoms.iter().any(|atom| atom == "@scope.public") {
        return true;
    }

    if audience == "public" {
        return false;
    }

    let allowed_roles = audience_roles(audience, qualifiers);
    if atoms.iter().any(|atom| allowed_roles.contains(atom)) {
        return true;
    }

    audience == "account"
        && atoms
            .iter()
            .any(|atom| atom == "@scope.same_org" || atom == "@scope.current_customer")
}

pub(super) fn audience_roles(audience: &str, qualifiers: &[String]) -> BTreeSet<String> {
    let mut roles = BTreeSet::new();
    roles.insert(format!("@role.{audience}"));

    for qualifier in qualifiers {
        if qualifier == "role" || qualifier == "roles" {
            continue;
        }
        if let Some(role) = qualifier.strip_prefix("@role.") {
            roles.insert(format!("@role.{role}"));
        } else {
            roles.insert(format!("@role.{qualifier}"));
        }
    }

    roles
}

pub(super) fn line_col_for_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;

    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

pub(super) fn path_references<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut references = Vec::new();
    let mut rest = source;

    while let Some(start) = rest.find(prefix) {
        let after_prefix = &rest[start + prefix.len()..];
        // Walk the reference char-by-char so embedded `{axis}` segments
        // (e.g. `env.CRYPT_KEY_TENANT_{tenant_id}` from the encryption
        // bucket cycle) are captured as part of the canonical reference
        // name. Outside braces, only `_` / alphanumerics belong to a
        // reference; inside braces, every character up to `}` is
        // captured verbatim.
        let bytes = after_prefix.as_bytes();
        let mut end = 0;
        let mut in_brace = false;
        while end < bytes.len() {
            let ch = bytes[end] as char;
            if in_brace {
                if ch == '}' {
                    in_brace = false;
                    end += 1;
                    continue;
                }
                end += 1;
                continue;
            }
            if ch == '{' {
                in_brace = true;
                end += 1;
                continue;
            }
            if ch == '_' || ch.is_ascii_alphanumeric() {
                end += 1;
                continue;
            }
            break;
        }
        if end > 0 {
            references.push(&after_prefix[..end]);
        }
        rest = &after_prefix[end..];
    }

    references
}

#[derive(Debug, Clone)]
struct PluginReferenceFact {
    path: PathBuf,
    line: usize,
    column: usize,
    reference: String,
}

#[derive(Debug, Clone)]
struct AtReferenceFact {
    path: PathBuf,
    line: usize,
    column: usize,
    reference: String,
    namespace: String,
    name: String,
}

pub(super) fn collect_package_plugin_references(
    package: &DoctorPackage,
) -> Vec<PluginReferenceFact> {
    package
        .files
        .iter()
        .filter(|file| is_lzi_path(&file.path))
        .flat_map(|file| collect_plugin_references_in_source(&file.path, &file.source))
        .collect()
}

pub(super) fn collect_plugin_references_in_source(
    path: &Path,
    source: &str,
) -> Vec<PluginReferenceFact> {
    let mut references = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = source[offset..].find("@lazuli/plugin-") {
        let start = offset + relative_start;
        let after_prefix = &source[start + "@lazuli/plugin-".len()..];
        let name_len = plugin_reference_name_len(after_prefix);
        if name_len > 0 {
            let (line, column) = line_col_for_offset(source, start);
            references.push(PluginReferenceFact {
                path: path.to_path_buf(),
                line,
                column,
                reference: source[start..start + "@lazuli/plugin-".len() + name_len].to_owned(),
            });
        }
        offset = start + "@lazuli/plugin-".len() + name_len.max(1);
    }
    references
}

pub(super) fn collect_at_references_in_source(path: &Path, source: &str) -> Vec<AtReferenceFact> {
    let mut references = Vec::new();
    let bytes = source.as_bytes();
    let mut offset = 0;

    while let Some(relative_start) = source[offset..].find('@') {
        let start = offset + relative_start;
        if start > 0 {
            let previous = bytes[start - 1];
            if previous.is_ascii_alphanumeric() || previous == b'_' {
                offset = start + 1;
                continue;
            }
        }

        let after_at = &source[start + 1..];
        let namespace_len = after_at
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        if namespace_len == 0 {
            offset = start + 1;
            continue;
        }

        let namespace = &after_at[..namespace_len];
        let separator = after_at.as_bytes().get(namespace_len).copied();
        if separator != Some(b'.') && separator != Some(b'/') {
            offset = start + 1 + namespace_len;
            continue;
        }

        let name_start = start + 1 + namespace_len + 1;
        let name_len = reference_name_len(&source[name_start..]);
        if name_len == 0 {
            offset = name_start;
            continue;
        }

        let (line, column) = line_col_for_offset(source, start);
        references.push(AtReferenceFact {
            path: path.to_path_buf(),
            line,
            column,
            reference: source[start..name_start + name_len].to_owned(),
            namespace: namespace.to_owned(),
            name: source[name_start..name_start + name_len].to_owned(),
        });
        offset = name_start + name_len;
    }

    references
}

pub(super) fn plugin_reference_name_len(source: &str) -> usize {
    source
        .bytes()
        .take_while(|byte| {
            byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'.' | b'/')
        })
        .count()
}

pub(super) fn reference_name_len(source: &str) -> usize {
    source
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'/'))
        .count()
}

pub(super) fn reference_namespace(reference: &str) -> Option<&str> {
    let after_at = reference.strip_prefix('@')?;
    let end = after_at.find(['.', '/']).unwrap_or(after_at.len());
    (end > 0).then_some(&after_at[..end])
}

pub(super) fn is_allowed_reference_namespace_for_doctor(namespace: &str) -> bool {
    matches!(
        namespace,
        "role"
            | "scope"
            | "actor"
            | "policy"
            | "semantic"
            | "cap"
            | "pii"
            | "key"
            | "fn"
            | "hook"
            | "validator"
            | "adapter"
            | "client"
            | "query_modifier"
            | "anchor"
            | "llm"
            | "tool"
            | "trace"
    )
}

pub(super) fn go_mod_lazuli_runtime_version(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || !trimmed.contains("lazuli.dev/runtime") {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        while let Some(part) = parts.next() {
            if part == "lazuli.dev/runtime" {
                return parts
                    .next()
                    .map(|version| version.trim_matches('"').to_owned());
            }
        }
    }
    None
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

/// Normalise a URL path so `:foo` and `:bar` become a single
/// placeholder. Two paths with the same shape but different slot
/// names collide at the gateway, so the check should treat them as
/// equal.
/// Collect every audience that's a first-class declaration in the
/// workspace. Today, surfaces in `.lzx` files are the canonical source
/// (`surface customer web` ... `audience admin`). A future cut may
/// elevate audiences to top-level `app.lzi` declarations; until then,
/// `.lzi` text scans are intentionally avoided — counting `audience X`
/// references inside e.g. `expose http audience X` would defeat the
/// `agent_expose_audience_unknown_diagnostics` check by self-resolving.
pub(super) fn collect_known_audiences(files: &[DoctorFile]) -> BTreeSet<String> {
    let mut audiences = BTreeSet::new();
    for file in files {
        if let Some(document) = file.lzx.as_ref() {
            for surface in &document.surfaces {
                for audience in &surface.audiences {
                    audiences.insert(audience.name.clone());
                }
            }
        }
    }
    audiences
}

pub(super) fn app_urls_missing_diagnostics(
    app: Option<&DoctorAppManifest>,
) -> Vec<DoctorDiagnostic> {
    let Some(app_manifest) = app else {
        return Vec::new();
    };
    if !app_manifest.manifest.urls.is_empty() {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: app_manifest.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "app_urls_missing".to_owned(),
        message: APP_URLS_MISSING_MESSAGE.to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

const APP_URLS_MISSING_MESSAGE: &str = "app declares no `urls` block — auth callbacks, CORS allowlist, and frontend redirect targets cannot be configured. Add a `urls` block to app.lzi with at least one environment URL (e.g., `urls\n  dev: \"http://localhost:3000\"`).";

// -----------------------------------------------------------------------------
// Cut A.9 — `approval` primitive on commands
// -----------------------------------------------------------------------------

/// Phase L Tier 4b — minimal text-walker output that captures the
/// existence of an `approval` block plus its missing children. Other
/// approval cross-checks (`timeout` shape, `then` catalog, `by` role
/// resolution) read from IR `Command.approval` via `Tier3FeatureFacts`.
///
/// The walker is retained because the parser canonical-indent slice
/// rejects malformed `approval` blocks with a parse error — which
/// short-circuits the feature lift, so `Command.approval` never reaches
/// the IR for those sources. The LSP file-local pass
/// (`approval_contract_diagnostics` in `crates/lazuli_lsp/src/lib.rs`)
/// emits the same diagnostic when invoked via `lazuli doctor`; this
/// walker covers the in-process unit test path
/// (`package_from_sources`), which does not feed sources through
/// `lazuli_lsp::diagnostics_for_source`.
#[derive(Debug, Clone)]
struct ApprovalBlockPresence {
    feature: String,
    command: String,
    path: PathBuf,
    line: usize,
    missing_children: Vec<&'static str>,
}

pub(super) fn collect_approval_block_presence(
    file: &DoctorFile,
    out: &mut Vec<ApprovalBlockPresence>,
) {
    let lines: Vec<&str> = file.source.lines().collect();
    let mut feature: Option<String> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            feature = trimmed
                .strip_prefix("feature ")
                .map(|n| n.trim().to_owned());
            i += 1;
            continue;
        }
        if leading_spaces(line) == 2 && trimmed.starts_with("command ") {
            let name = trimmed
                .strip_prefix("command ")
                .map(|n| n.split_whitespace().next().unwrap_or("").to_owned())
                .unwrap_or_default();
            let feature_name = feature.clone().unwrap_or_default();

            // Find the `approval` child (indent 4) inside this command body.
            let mut j = i + 1;
            let mut approval_at: Option<usize> = None;
            while j < lines.len() {
                let inner = lines[j];
                let inner_trim = inner.trim_start();
                if inner_trim.is_empty() || inner_trim.starts_with('#') {
                    j += 1;
                    continue;
                }
                if leading_spaces(inner) <= 2 {
                    break;
                }
                if leading_spaces(inner) == 4 && inner_trim == "approval" {
                    approval_at = Some(j);
                    break;
                }
                j += 1;
            }

            if let Some(approval_at) = approval_at {
                let mut has_by = false;
                let mut has_timeout = false;
                let mut has_then = false;
                let mut k = approval_at + 1;
                while k < lines.len() {
                    let body = lines[k];
                    let body_trim = body.trim_start();
                    if body_trim.is_empty() || body_trim.starts_with('#') {
                        k += 1;
                        continue;
                    }
                    if leading_spaces(body) <= 4 {
                        break;
                    }
                    if leading_spaces(body) == 6 {
                        if body_trim.starts_with("by ") {
                            has_by = true;
                        } else if body_trim.starts_with("timeout ") {
                            has_timeout = true;
                        } else if body_trim.starts_with("then ") {
                            has_then = true;
                        }
                    }
                    k += 1;
                }
                let mut missing: Vec<&'static str> = Vec::new();
                if !has_by {
                    missing.push("by");
                }
                if !has_timeout {
                    missing.push("timeout");
                }
                if !has_then {
                    missing.push("then");
                }
                if !missing.is_empty() {
                    out.push(ApprovalBlockPresence {
                        feature: feature_name,
                        command: name,
                        path: file.path.clone(),
                        line: approval_at + 1,
                        missing_children: missing,
                    });
                }
                i = k;
                continue;
            }
        }
        i += 1;
    }
}

/// `approval_contract_diagnostics` (missing-children variant) — emitted
/// from the text-pattern walker above because parse-error approval
/// blocks never reach the IR.
pub(super) fn approval_missing_children_diagnostics(
    presences: &[ApprovalBlockPresence],
) -> Vec<DoctorDiagnostic> {
    presences
        .iter()
        .map(|p| DoctorDiagnostic {
            path: p.path.clone(),
            line: p.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "approval_contract_diagnostics".to_owned(),
            message: format!(
                "command `{}.{}` declares `approval` but is missing required children: {}.",
                p.feature,
                p.command,
                p.missing_children.join(", "),
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

/// Doctor-side diagnostics for the `approval` primitive. Three
/// dedicated ids plus the write-tool guard extension; the latter
/// reaches inside `agent_tool_write_unguarded_diagnostics` so write
/// tools whose target command carries `approval` no longer require
/// the agent's `safety` validator.
///
/// Phase L Tier 4b — reads `Command.approval` from `Tier3FeatureFacts`
/// (populated by `lower_feature_skeleton`). The text-walking
/// `CommandApprovalFact` collector retired; a minimal
/// `ApprovalBlockPresence` walker survives for the
/// missing-required-children variant. Two drift facts the retirement
/// repaired:
///
/// 1. The text-walker treated `by` as a `,`-split list. The parser
///    only accepts a single `by` declaration; the lifted IR carries
///    `by: String`. The doctor now validates the single role atom.
/// 2. The text-walker reported a closed catalog of `deny | proceed`
///    for `then`. The parser already enforces `deny | allow | escalate`
///    at parse time, lowering to `ApprovalThen` (enum-fechado). The
///    redundant doctor-side catalog check retired.
pub(super) fn approval_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
    known_roles: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for feature in tier3_facts {
        for command in &feature.commands {
            let Some(approval) = &command.approval else {
                continue;
            };
            let line = feature
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or(feature.feature_line);

            // Timeout shape (when authored).
            if let Some(timeout) = approval.timeout.as_deref() {
                if !approval_timeout_well_formed(timeout) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "approval_timeout_invalid_diagnostics".to_owned(),
                        message: format!(
                            "command `{}.{}` declares `approval timeout {:?}` which is not a recognised duration shape (e.g. `\"24h\"`, `\"30 minutes\"`, `\"7d\"`).",
                            feature.feature, command.name, timeout,
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }

            // Role resolution. `by` is a single `@role.<name>` atom.
            let role_ref = approval.by.as_str();
            let Some(suffix) = role_ref.strip_prefix("@role.") else {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "approval_role_unresolved_diagnostics".to_owned(),
                    message: format!(
                        "command `{}.{}` approval `by {role_ref}` is not a `@role.<name>` reference; approvers are roles, not scopes.",
                        feature.feature, command.name,
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            };
            if !known_roles.contains(suffix) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "approval_role_unresolved_diagnostics".to_owned(),
                    message: format!(
                        "command `{}.{}` approval `by @role.{suffix}` references a role that no `policies` block or `app.lzi` `policy_for` declares.",
                        feature.feature, command.name,
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

    diagnostics
}

/// `SCOPE-OWNER-COLUMN-001` — warn when a command's policy includes
/// `@scope.owner` or `@scope.same_org` but the targeted resource has
/// no matching ownership / tenant column. Codegen silently skips the
/// WHERE binding in that case (per `crates/lazuli_codegen_go/src/emitter/command.rs`
/// `resolve_scope_bindings`); this diagnostic surfaces the silent-skip
/// at design time so authors don't ship a policy that's only enforced
/// at the role-check gate.
///
/// Closed-catalog column priorities mirror codegen exactly:
/// - `@scope.owner` searches `user_id` > `user` > `owner_id` > `owner`.
/// - `@scope.same_org` searches `org_id` > `org` > `tenant_id` > `tenant`.
pub(super) fn scope_owner_column_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use lazuli_ir::{CommandEffect, PolicyRef};

    const OWNER_COLUMNS: &[&str] = &["user_id", "user", "owner_id", "owner"];
    const SAME_ORG_COLUMNS: &[&str] = &["org_id", "org", "tenant_id", "tenant"];

    let mut diagnostics = Vec::new();

    for feature in tier3_facts {
        let local_policies: BTreeMap<&str, &Vec<String>> = feature
            .policies
            .categories
            .iter()
            .map(|c| (c.name.as_str(), &c.atoms))
            .collect();

        for command in &feature.commands {
            // Only `Updates` and `Deletes` benefit from row-scoping;
            // `Creates`/`Returns`/`None` have no target row.
            let resource_qname = match &command.effect {
                CommandEffect::Updates(u) => &u.resource,
                CommandEffect::Deletes(d) => &d.resource,
                _ => continue,
            };

            // Only resolve when the resource lives in this feature.
            // Cross-feature scope lowering is a follow-up (would need
            // the module-level resource index).
            if let Some(feature_part) = &resource_qname.feature {
                if feature_part != &feature.feature {
                    continue;
                }
            }
            let Some(resource) = feature
                .resources
                .iter()
                .find(|r| r.name == resource_qname.name)
            else {
                continue;
            };

            // Resolve the command's policy atom list. Mirrors the
            // codegen-side `command_policy_atoms` helper plus the
            // `populate_commands_from_ir` atom-resolution logic so
            // the diagnostic fires on exactly the cases codegen would
            // silently skip — and on the canonical `@policy.<name>`
            // form (parsed as `PolicyRef::Atom("policy.<name>")` with
            // a local-policies lookup) as well as the rare bare
            // `PolicyRef::Local`.
            let atoms: Vec<String> = match &command.policy {
                PolicyRef::Local(name) => local_policies
                    .get(name.as_str())
                    .map(|atoms| (*atoms).clone())
                    .unwrap_or_default(),
                PolicyRef::Atom(atom) => {
                    if let Some(local) = atom.strip_prefix("policy.") {
                        local_policies
                            .get(local)
                            .map(|atoms| (*atoms).clone())
                            .unwrap_or_else(|| vec![format!("@{atom}")])
                    } else {
                        vec![format!("@{atom}")]
                    }
                }
                _ => continue,
            };

            for atom in &atoms {
                let (priority, axis_label) = match atom.as_str() {
                    "@scope.owner" => (OWNER_COLUMNS, "owner"),
                    "@scope.same_org" => (SAME_ORG_COLUMNS, "org"),
                    _ => continue,
                };
                let has_match = priority
                    .iter()
                    .any(|c| resource.fields.iter().any(|f| f.name == *c));
                if has_match {
                    continue;
                }
                let line = feature
                    .command_lines
                    .get(&command.name)
                    .copied()
                    .unwrap_or(feature.feature_line);
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "SCOPE-OWNER-COLUMN-001".to_owned(),
                    message: format!(
                        "command `{}.{}` policy includes `{atom}` but resource `{}` has none of `{}`; codegen will skip the auto-injected WHERE binding and the row-scope will only be enforced by the role check (not the DB).",
                        feature.feature,
                        command.name,
                        resource.name,
                        priority.join("`, `"),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                // Don't flag the same command twice for the same atom.
                let _ = axis_label;
                break;
            }
        }
    }

    diagnostics
}

/// `field_derived_from_unresolved` — warn when a resource field's
/// `derived from <expr>` references identifiers that don't resolve
/// to siblings on the same resource. Closes the first of three
/// net-new Tier 4c doctor lints catalogued in the
/// naming-reconciliation proposal (`docs/proposals/naming-reconciliation-2026-05-17.md`).
///
/// The lint tokenises the expression text, drops keywords / operators
/// / numeric literals / string literals / dotted-path identifiers
/// (`other.field` — relation traversal is out of scope for v1), and
/// reports any remaining bare identifier that is not a sibling field
/// or a built-in (`ctx`, `now`, `true`, `false`, `nil`). Severity is
/// Warning — the runtime panics on resolution failure, but a Warning
/// at design time surfaces the typo before deploy.
pub(super) fn field_derived_from_unresolved_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for feature in tier3_facts {
        for resource in &feature.resources {
            let sibling_names: BTreeSet<&str> =
                resource.fields.iter().map(|f| f.name.as_str()).collect();
            for field in &resource.fields {
                let Some(expr) = field.derived_from.as_deref() else {
                    continue;
                };
                let unresolved = collect_unresolved_field_refs(expr, &sibling_names);
                if unresolved.is_empty() {
                    continue;
                }
                let line = field
                    .span_ref
                    .as_ref()
                    .map(|s| line_col_for_offset_in_file(&feature.path, s.start).0)
                    .unwrap_or(feature.feature_line);
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "field_derived_from_unresolved".to_owned(),
                    message: format!(
                        "field `{}.{}` derived from `{}` references identifier(s) `{}` that don't resolve to a sibling field on resource `{}`.",
                        resource.name,
                        field.name,
                        expr,
                        unresolved.join("`, `"),
                        resource.name,
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
    diagnostics
}

/// Best-effort source-offset → line resolver for derived-from spans.
/// Falls back to line 1 when the path can't be read (test fixtures
/// often produce in-memory facts without disk-backing).
pub(super) fn line_col_for_offset_in_file(path: &Path, offset: usize) -> (usize, usize) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return (1, 1);
    };
    line_col_for_offset(&source, offset)
}

/// Tokenise a `derived from` expression, drop operators / numerics /
/// string literals / dotted paths / keywords, and return identifiers
/// that don't resolve to any name in `siblings`. The check is
/// intentionally conservative — over-rejecting an identifier the
/// runtime would have accepted is a Warning, not an Error, so a
/// false positive nudges the author to rename / annotate rather than
/// blocking the commit.
pub(super) fn collect_unresolved_field_refs(expr: &str, siblings: &BTreeSet<&str>) -> Vec<String> {
    // Strip string literals (single + double quoted) first so their
    // contents don't masquerade as identifiers.
    let mut buf = String::with_capacity(expr.len());
    let mut chars = expr.chars().peekable();
    let mut in_string: Option<char> = None;
    while let Some(c) = chars.next() {
        match in_string {
            Some(quote) => {
                if c == quote {
                    in_string = None;
                    buf.push(' ');
                } else if c == '\\' {
                    chars.next();
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_string = Some(c);
                } else {
                    buf.push(c);
                }
            }
        }
    }
    // Replace non-identifier-char with whitespace so split tokenises
    // cleanly.
    let normalised: String = buf
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
                c
            } else {
                ' '
            }
        })
        .collect();

    let keywords: &[&str] = &[
        "and", "or", "not", "true", "false", "nil", "null", "ctx", "now", "self", "target",
    ];

    let mut out: Vec<String> = Vec::new();
    for raw in normalised.split_whitespace() {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        // Drop dotted paths (relation traversal — v1 limit).
        if token.contains('.') {
            continue;
        }
        // Drop numeric literals.
        if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        // Drop keywords.
        if keywords.contains(&token.to_ascii_lowercase().as_str()) {
            continue;
        }
        // Identifiers must start with letter / underscore.
        if !token
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            continue;
        }
        if siblings.contains(token) {
            continue;
        }
        if !out.iter().any(|s| s == token) {
            out.push(token.to_owned());
        }
    }
    out
}

/// `resource_unique_qualifier_unknown` — Tier 4c lint per the
/// naming-reconciliation proposal §4 row 1 (NEW producer). Warns when
/// a `unique <field> per <qualifier>` constraint names a `<qualifier>`
/// that is not a field on the same resource. The runtime ignores
/// unknown qualifiers silently; the lint surfaces the gap at design
/// time so SQL composes the intended composite unique index.
pub(super) fn resource_unique_qualifier_unknown_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use lazuli_ir::Constraint;
    let mut diagnostics = Vec::new();
    for feature in tier3_facts {
        for resource in &feature.resources {
            let field_names: BTreeSet<&str> =
                resource.fields.iter().map(|f| f.name.as_str()).collect();
            for constraint in &resource.constraints {
                let Constraint::Unique(unique) = constraint else {
                    continue;
                };
                let Some(qualifier) = unique.per.as_deref() else {
                    continue;
                };
                // The qualifier itself may be a known tenant axis
                // (`org` / `team` — see `tenancy_axis_for`). Skip those
                // even when the resource doesn't declare a literal
                // `org` field; the runtime resolves them through the
                // feature's `defaults.tenancy`.
                if matches!(qualifier, "org" | "team" | "tenant") {
                    continue;
                }
                if field_names.contains(qualifier) {
                    continue;
                }
                let line = resource
                    .span_ref
                    .as_ref()
                    .map(|s| line_col_for_offset_in_file(&feature.path, s.start).0)
                    .unwrap_or(feature.feature_line);
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "resource_unique_qualifier_unknown".to_owned(),
                    message: format!(
                        "resource `{}` declares `unique {} per {}` but `{}` is not a sibling field. The runtime will silently ignore the qualifier and emit a non-tenant-scoped UNIQUE index.",
                        resource.name,
                        unique.fields.join(", "),
                        qualifier,
                        qualifier,
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
    diagnostics
}

/// `resource_validates_path_unknown` — Tier 4c lint per the
/// naming-reconciliation proposal §4 row 2 (NEW producer). Two checks
/// fire:
///
/// 1. `validates field <field> ...` — `<field>` must be a sibling on
///    the same resource.
/// 2. `validates field <field> @validator.<name>` — `<name>` must be
///    declared under `extensions` with the `Validator` contract.
///
/// The LSP proxy `validation-syntax` (`lazuli_lsp/src/lib.rs:5987`)
/// only catches malformed syntax; this lint is the cross-reference.
pub(super) fn resource_validates_path_unknown_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    use lazuli_ir::ExtensionContract;
    let mut diagnostics = Vec::new();
    for feature in tier3_facts {
        let validator_names: BTreeSet<&str> = feature
            .extensions
            .iter()
            .filter(|e| matches!(e.contract, ExtensionContract::Validator { .. }))
            .map(|e| e.name.as_str())
            .collect();

        for resource in &feature.resources {
            let field_names: BTreeSet<&str> =
                resource.fields.iter().map(|f| f.name.as_str()).collect();

            for v in &resource.validates {
                let line = resource
                    .span_ref
                    .as_ref()
                    .map(|s| line_col_for_offset_in_file(&feature.path, s.start).0)
                    .unwrap_or(feature.feature_line);

                // Check 1: field exists.
                if !field_names.contains(v.field.as_str()) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Warning,
                        code: "resource_validates_path_unknown".to_owned(),
                        message: format!(
                            "resource `{}` declares `validates field {}` but `{}` is not a field on this resource. Available fields: {}.",
                            resource.name,
                            v.field,
                            v.field,
                            field_names
                                .iter()
                                .copied()
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                    continue;
                }

                // Check 2: @validator.<name> resolves through extensions.
                if let Some(rest) = v.path.path.strip_prefix("@validator.") {
                    let name = rest
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()
                        .unwrap_or(rest);
                    if !name.is_empty() && !validator_names.contains(name) {
                        let known: Vec<&str> = validator_names.iter().copied().collect();
                        diagnostics.push(DoctorDiagnostic {
                            path: feature.path.clone(),
                            line,
                            column: 1,
                            severity: DoctorSeverity::Warning,
                            code: "resource_validates_path_unknown".to_owned(),
                            message: format!(
                                "resource `{}.{}` validates against `@validator.{}` but no `validator {}` is declared under the feature's `extensions` block. Declared validators: {}.",
                                resource.name,
                                v.field,
                                name,
                                name,
                                if known.is_empty() { "(none)".to_owned() } else { known.join(", ") },
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
    diagnostics
}

/// Shape-check a duration string. Accepts `<digits> <unit>` or
/// `<digits><unit>` where unit ∈ s, m, h, d, w, second(s),
/// minute(s), hour(s), day(s), week(s). The runtime adapter parses
/// the canonical form; this layer rejects obviously malformed input.
pub(super) fn approval_timeout_well_formed(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut chars = trimmed.chars();
    let mut saw_digit = false;
    let mut unit_start = 0;
    for (i, c) in chars.by_ref().enumerate() {
        if c.is_ascii_digit() {
            saw_digit = true;
            continue;
        }
        unit_start = i;
        break;
    }
    if !saw_digit {
        return false;
    }
    let unit = trimmed[unit_start..].trim();
    matches!(
        unit,
        "s" | "m"
            | "h"
            | "d"
            | "w"
            | "second"
            | "seconds"
            | "minute"
            | "minutes"
            | "hour"
            | "hours"
            | "day"
            | "days"
            | "week"
            | "weeks"
    )
}

/// Collect every role name declared by a feature's `policies` block
/// (children at indent 4 referencing `@role.<name>`) or by an
/// `app.lzi` `policy_for ...: @role.<name>` default. Used by
/// `approval_role_unresolved_diagnostics`.
///
/// Intentionally scoped: scanning every `@role.X` reference in the
/// file would self-resolve the very `by @role.X` line we're trying
/// to validate. Only first-class declarations count.
pub(super) fn collect_known_roles(files: &[DoctorFile]) -> BTreeSet<String> {
    let mut roles = BTreeSet::new();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            // Feature-level `policies` block at indent 2.
            if leading_spaces(line) == 2 && trimmed == "policies" {
                let mut j = i + 1;
                while j < lines.len() {
                    let inner = lines[j];
                    let inner_trim = inner.trim_start();
                    if inner_trim.is_empty() || inner_trim.starts_with('#') {
                        j += 1;
                        continue;
                    }
                    if leading_spaces(inner) <= 2 {
                        break;
                    }
                    // `<name>: @role.x, @scope.y, ...` — harvest only
                    // the @role.<name> entries.
                    if let Some((_, refs)) = inner_trim.split_once(':') {
                        extract_role_atoms(refs, &mut roles);
                    }
                    j += 1;
                }
                i = j;
                continue;
            }
            // Top-level `app.lzi` `policy_for <kinds>: @role.x, ...`
            // (or feature-level `policy_for` inside `defaults`).
            if let Some(rest) = trimmed.strip_prefix("policy_for ") {
                if let Some((_, refs)) = rest.split_once(':') {
                    extract_role_atoms(refs, &mut roles);
                }
            }
            i += 1;
        }
    }
    roles
}

pub(super) fn extract_role_atoms(refs: &str, roles: &mut BTreeSet<String>) {
    for token in refs.split(',') {
        let token = token.trim();
        if let Some(name) = token.strip_prefix("@role.") {
            let end = name
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(name.len());
            if end > 0 {
                roles.insert(name[..end].to_owned());
            }
        }
    }
}

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

/// Aggregate the package-level RBAC catalog by re-parsing each `.lzi`
/// file and concatenating `permission` / `role` decls. Cross-file
/// duplicates are caught by the analyzer's per-package pass.
pub(super) fn collect_package_rbac_catalog(
    files: &[DoctorFile],
) -> (
    Option<lazuli_ir::RbacCatalog>,
    Vec<(PathBuf, lazuli_analyzer::rbac::RbacIssue)>,
) {
    use lazuli_syntax::{PackageSkeleton, PermissionDeclAst, RoleDeclAst, parse_package_skeleton};

    let mut all_permissions: Vec<PermissionDeclAst> = Vec::new();
    let mut all_roles: Vec<RoleDeclAst> = Vec::new();
    let mut file_of_decl: BTreeMap<usize, PathBuf> = BTreeMap::new();

    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let Ok(pkg) = parse_package_skeleton(&file.source) else {
            continue;
        };
        for p in pkg.permissions {
            file_of_decl.insert(all_permissions.len(), file.path.clone());
            all_permissions.push(p);
        }
        for r in pkg.roles {
            // Use a disjoint key space (roles indexed by 1_000_000 + i)
            // to avoid collision with permission indices.
            file_of_decl.insert(1_000_000 + all_roles.len(), file.path.clone());
            all_roles.push(r);
        }
    }

    if all_permissions.is_empty() && all_roles.is_empty() {
        return (None, Vec::new());
    }

    let pkg = PackageSkeleton {
        features: Vec::new(),
        permissions: all_permissions,
        roles: all_roles,
    };
    let (catalog, issues) = lazuli_analyzer::rbac::analyze_rbac_catalog(&pkg);
    // For now, attach the first .lzi file with rbac decls to each issue.
    let representative = files
        .iter()
        .find(|f| {
            is_lzi_path(&f.path) && f.source.contains("\nrole ") || f.source.starts_with("role ")
        })
        .or_else(|| files.iter().find(|f| is_lzi_path(&f.path)))
        .map(|f| f.path.clone())
        .unwrap_or_default();
    let issues_with_path: Vec<(PathBuf, _)> = issues
        .into_iter()
        .map(|i| (representative.clone(), i))
        .collect();
    (catalog, issues_with_path)
}

/// Convert analyzer-emitted RBAC issues into doctor diagnostics.
pub(super) fn rbac_catalog_diagnostics(
    files: &[DoctorFile],
) -> (Vec<DoctorDiagnostic>, Option<lazuli_ir::RbacCatalog>) {
    let (catalog, issues) = collect_package_rbac_catalog(files);
    let mut out: Vec<DoctorDiagnostic> = Vec::new();
    for (path, issue) in issues {
        let line = if let Some((start, _)) = issue.span {
            line_col_for_offset_from_files(files, &path, start).0
        } else {
            1
        };
        let severity = match issue.code {
            "RBAC-PERM-UNUSED-001" => DoctorSeverity::Warning,
            _ => DoctorSeverity::Error,
        };
        out.push(DoctorDiagnostic {
            path,
            line,
            column: 1,
            severity,
            code: issue.code.to_owned(),
            message: issue.message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    (out, catalog)
}

/// Resolve a byte offset within a given file path to (line, column).
pub(super) fn line_col_for_offset_from_files(
    files: &[DoctorFile],
    path: &Path,
    offset: usize,
) -> (usize, usize) {
    for f in files {
        if f.path == path {
            return line_col_for_offset(&f.source, offset);
        }
    }
    (1, 1)
}

/// RBAC-ROLE-UNDECLARED-001 — when a catalog IS declared, every
/// `@role.X` mention in `policies` / `policy_for` must resolve to a
/// catalog role. Returns one diagnostic per orphan reference (deduped).
pub(super) fn rbac_role_undeclared_diagnostics(
    files: &[DoctorFile],
    catalog: &lazuli_ir::RbacCatalog,
) -> Vec<DoctorDiagnostic> {
    let mut out = Vec::new();
    let catalog_roles: BTreeSet<String> = catalog.roles.iter().map(|r| r.name.clone()).collect();
    let mentioned = collect_known_roles(files);
    for role in mentioned.difference(&catalog_roles) {
        // Find the first file that mentions this role.
        for file in files {
            if !is_lzi_path(&file.path) {
                continue;
            }
            let needle = format!("@role.{}", role);
            if let Some(idx) = file.source.find(&needle) {
                let (line, _) = line_col_for_offset(&file.source, idx);
                out.push(DoctorDiagnostic {
                    path: file.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "RBAC-ROLE-UNDECLARED-001".to_owned(),
                    message: format!(
                        "`@role.{}` references a role not declared in the RBAC catalog (declare `role {}` at top level or remove the reference).",
                        role, role
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                break;
            }
        }
    }
    out
}

/// RBAC-CATALOG-MISSING-001 (info advisory) — fires when the legacy
/// implicit-role-set has entries but no `role` / `permission` blocks
/// were authored at top level. Migration hint per
/// `docs/proposals/rbac-catalog-vocab.md` §Backwards compatibility.
pub(super) fn rbac_catalog_missing_diagnostics(
    files: &[DoctorFile],
    catalog_present: bool,
) -> Vec<DoctorDiagnostic> {
    if catalog_present {
        return Vec::new();
    }
    let implicit = collect_known_roles(files);
    if implicit.is_empty() {
        return Vec::new();
    }
    // Surface a single hint on the first `.lzi` file that mentions a
    // role. Severity is `Info` mapped to LSP Hint.
    let role_names: Vec<String> = implicit.into_iter().collect();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let needle = format!("@role.{}", role_names[0]);
        if let Some(idx) = file.source.find(&needle) {
            let (line, _) = line_col_for_offset(&file.source, idx);
            return vec![DoctorDiagnostic {
                path: file.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Info,
                code: "RBAC-CATALOG-MISSING-001".to_owned(),
                message: format!(
                    "package uses `@role.*` references ({}) but declares no `role` / `permission` catalog. Consider migrating to a top-level RBAC catalog (see docs/proposals/rbac-catalog-vocab.md).",
                    role_names.join(", ")
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }];
        }
    }
    Vec::new()
}

/// RBAC-MISSING-POLICY-001 — feature mixes policied + unpoliced
/// commands/queries. Suspicious gap; explicit `policy @scope.public`
/// opts out. Warning level. Per-feature; scans for indent-2 `command`/
/// `query.*` blocks and checks if their indent-4 children include a
/// `policy ` line.
pub(super) fn rbac_missing_policy_diagnostics(files: &[DoctorFile]) -> Vec<DoctorDiagnostic> {
    let mut out = Vec::new();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut feature: Option<String> = None;
        let mut feature_line: usize = 0;
        // For each feature, count callables with/without `policy`.
        let mut policied: Vec<String> = Vec::new();
        let mut unpoliced: Vec<(String, usize)> = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }
            if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
                // Flush prior feature.
                if let Some(_fname) = feature.take() {
                    flush_missing_policy(&mut out, &file.path, &policied, &unpoliced);
                }
                feature = trimmed
                    .strip_prefix("feature ")
                    .map(|n| n.trim().to_owned());
                feature_line = i + 1;
                policied.clear();
                unpoliced.clear();
                i += 1;
                continue;
            }
            let _ = feature_line;
            if leading_spaces(line) == 2
                && (trimmed.starts_with("command ")
                    || trimmed.starts_with("query.list ")
                    || trimmed.starts_with("query.lookup ")
                    || trimmed.starts_with("query.sql ")
                    || trimmed.starts_with("query.view ")
                    || trimmed.starts_with("api "))
            {
                let name = trimmed.split_whitespace().nth(1).unwrap_or("").to_owned();
                // Scan body at indent 4 for a `policy ` line.
                let mut has_policy = false;
                let mut j = i + 1;
                while j < lines.len() {
                    let inner = lines[j];
                    let inner_trim = inner.trim_start();
                    if inner_trim.is_empty() || inner_trim.starts_with('#') {
                        j += 1;
                        continue;
                    }
                    if leading_spaces(inner) <= 2 {
                        break;
                    }
                    if leading_spaces(inner) == 4 && inner_trim.starts_with("policy ") {
                        has_policy = true;
                        break;
                    }
                    j += 1;
                }
                if has_policy {
                    policied.push(name);
                } else {
                    unpoliced.push((name, i + 1));
                }
            }
            i += 1;
        }
        if let Some(_fname) = feature.take() {
            flush_missing_policy(&mut out, &file.path, &policied, &unpoliced);
        }
    }
    out
}

pub(super) fn flush_missing_policy(
    out: &mut Vec<DoctorDiagnostic>,
    path: &Path,
    policied: &[String],
    unpoliced: &[(String, usize)],
) {
    if policied.len() < 2 || unpoliced.is_empty() {
        return;
    }
    for (name, line) in unpoliced {
        out.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line: *line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "RBAC-MISSING-POLICY-001".to_owned(),
            message: format!(
                "`{}` declares no explicit `policy` while sibling callables do; add `policy <atoms>` (or `policy @scope.public` to opt out) for visibility.",
                name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

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

// ============================================================================
// Row 30 — Storage bucket cycle diagnostics
//
// Five typed `@cap.File(...)` checks run against `OperationalFacts.
// file_capability_facts`, populated by `collect_file_capability_facts`.
// Codes:
//   - `cap_file_visibility_undeclared`              error
//   - `cap_file_accept_input_output_mismatch`       error
//   - `cap_file_visibility_signed_ttl_mismatch`     error
//   - `cap_file_size_unit_invalid`                  error
//   - `cap_file_mime_family_unknown`                warning
//
// See `docs/proposals/bucket-storage-cycle.md` §Doctor/LSP.
// ============================================================================

/// IANA top-level MIME families recognised by Lazuli's `@cap.File(accept:)`
/// closed catalog. Subtype `*` and family `*` are also accepted at the
/// shape level, but emitted under the wildcard match.
const KNOWN_MIME_FAMILIES: &[&str] = &[
    "text",
    "image",
    "application",
    "audio",
    "video",
    "font",
    "*",
];

/// Run the 10 `REPORT-*` doctor rules per
/// `docs/proposals/report-vocab.md` v0.2 §Doctor / LSP, aggregating
/// findings into typed `DoctorDiagnostic` rows.
pub(super) fn report_diagnostics(
    facts: &[Tier3FeatureFacts],
    app: Option<&AppManifest>,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let storage_caps = collect_object_storage_caps(app, registry);

    for fact in facts {
        if fact.reports.is_empty() {
            continue;
        }
        let mut feature_for_rules = make_synthetic_feature_for_reports(fact);

        // Local-only rules consume the synthesized Feature view.
        for finding in report::report_columns_empty_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_columns_empty_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_signed_ttl_missing_001::check(&feature_for_rules, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_signed_ttl_missing_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in
            report::report_signed_ttl_forbidden_001::check(&feature_for_rules, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_signed_ttl_forbidden_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in
            report::report_filename_token_unknown_001::check(&feature_for_rules, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_filename_token_unknown_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_source_kind_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_source_kind_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in
            report::report_policy_public_no_rate_limit_001::check(&feature_for_rules, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_policy_public_no_rate_limit_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_column_mismatch_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_column_mismatch_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_path_collision_001::check(&feature_for_rules, &fact.path) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_path_collision_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_signed_no_storage_001::check(
            &feature_for_rules,
            &storage_caps,
            &fact.path,
        ) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_signed_no_storage_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        for finding in report::report_storage_ambiguous_001::check(
            &feature_for_rules,
            &storage_caps,
            &fact.path,
        ) {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_storage_ambiguous_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // AST-based rule (REPORT-FORMAT-UNKNOWN-001) reads the raw
        // ReportDecl text because lowering drops unknown format tokens.
        for finding in
            report::report_format_unknown_001::check(&fact.feature, &fact.report_decls, &fact.path)
        {
            let line = fact
                .report_lines
                .get(&finding.report)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: report::report_format_unknown_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // Drop the unused borrow to satisfy the borrow checker on the
        // synthetic feature value (none of the rule branches return
        // borrowed data beyond their iteration).
        let _ = &mut feature_for_rules;
    }

    diagnostics
}

/// Build a minimal `ir::Feature` view from a `Tier3FeatureFacts` row so
/// the report rule modules (which take `&Feature`) can be invoked
/// without re-lowering. Only the slots the rule modules read are
/// populated; everything else stays at default.
pub(crate) fn make_synthetic_feature_for_reports(fact: &Tier3FeatureFacts) -> lazuli_ir::Feature {
    lazuli_ir::Feature {
        name: fact.feature.clone(),
        purpose: None,
        non_goals: Vec::new(),
        context_path: None,
        defaults: lazuli_ir::Defaults::default(),
        uses: Vec::new(),
        uses_spans: Vec::new(),
        uses_versions: Vec::new(),
        requirements: Vec::new(),
        enums: Vec::new(),
        resources: fact.resources.clone(),
        events: Vec::new(),
        rules: Vec::new(),
        policies: lazuli_ir::Policies::default(),
        errors: None,
        commands: Vec::new(),
        apis: fact.apis.clone(),
        records: fact.records.clone(),
        queries: fact.queries.clone(),
        resume_routers: Vec::new(),
        workflows: Vec::new(),
        jobs: Vec::new(),
        webhooks: Vec::new(),
        notifications: Vec::new(),
        event_groups: Vec::new(),
        tenant_migrations: Vec::new(),
        translation: None,
        auth: None,
        surfaces: Vec::new(),
        extensions: Vec::new(),
        escape_routes: Vec::new(),
        agents: fact.agents.clone(),
        reports: fact.reports.clone(),
        pollers: vec![],
        channels: Vec::new(),
        caches: Vec::new(),
        aggregates: fact.aggregates.clone(),
        mcp_servers: Vec::new(),
        previous_names: Vec::new(),
        synth_origins: std::collections::BTreeMap::new(),
        span_ref: None,
    }
}

pub(super) fn cap_file_storage_diagnostics(
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // (1) cap_file_visibility_undeclared — api output without `visibility:`.
    // (3) cap_file_visibility_signed_ttl_mismatch — visibility/signed_ttl
    //     coherence (api outputs and resource fields both).
    // (5) cap_file_mime_family_unknown — MIME family outside the IANA
    //     closed catalog.
    for fact in &operational.file_capability_facts {
        if matches!(fact.binding, FileCapabilityBinding::ApiOutput { .. })
            && fact.capability.visibility.is_none()
        {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: fact.column,
                severity: DoctorSeverity::Error,
                code: "cap_file_visibility_undeclared".to_owned(),
                message: format!(
                    "api `{}` output declares `@cap.File(...)` without `visibility:`; ambiguous visibility on a file URL is a security contract gap. Declare `visibility:` as `public`, `private`, or `signed`.",
                    match &fact.binding {
                        FileCapabilityBinding::ApiOutput { api } => api.as_str(),
                        _ => "<unknown>",
                    }
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        match (
            fact.capability.visibility,
            fact.capability.signed_ttl.as_deref(),
        ) {
            (Some(lazuli_ir::FileVisibility::Signed), None) => {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Error,
                    code: "cap_file_visibility_signed_ttl_mismatch".to_owned(),
                    message:
                        "`@cap.File(visibility:signed)` requires `signed_ttl:<duration>` (e.g. `1h`); signed URLs without a TTL contract leak forever."
                            .to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            (Some(other), Some(_)) if !matches!(other, lazuli_ir::FileVisibility::Signed) => {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Error,
                    code: "cap_file_visibility_signed_ttl_mismatch".to_owned(),
                    message: format!(
                        "`@cap.File(visibility:{})` forbids `signed_ttl`; `signed_ttl` only applies when `visibility:signed`.",
                        format_visibility(other),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            _ => {}
        }

        for mime in &fact.capability.accept {
            if !KNOWN_MIME_FAMILIES.contains(&mime.family.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.line,
                    column: fact.column,
                    severity: DoctorSeverity::Warning,
                    code: "cap_file_mime_family_unknown".to_owned(),
                    message: format!(
                        "`@cap.File(accept:{}/{}` uses unknown MIME family `{}`; known families: {}.",
                        mime.family,
                        mime.subtype,
                        mime.family,
                        KNOWN_MIME_FAMILIES.join(", "),
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

    // (4) cap_file_size_unit_invalid — typed promotion. The IR rejects
    //     unknown units at parse time (the analyzer falls through to
    //     `UserDefined`), so any line that matched `@cap.File(...)`
    //     literally but did NOT produce a typed `FileCapability` fact
    //     is the candidate. We re-walk operational.file_capabilities
    //     (the text-pattern facts) and cross-reference; sites that
    //     have NO typed fact for the same path:line are typing
    //     failures — promote with a typed error.
    for text_fact in &operational.file_capabilities {
        let has_typed = operational
            .file_capability_facts
            .iter()
            .any(|tf| tf.path == text_fact.path && tf.line == text_fact.line);
        if !has_typed {
            diagnostics.push(DoctorDiagnostic {
                path: text_fact.path.clone(),
                line: text_fact.line,
                column: text_fact.column,
                severity: DoctorSeverity::Error,
                code: "cap_file_size_unit_invalid".to_owned(),
                message:
                    "`@cap.File(max_size:<literal>)` must use a positive integer with unit `kb`, `mb`, or `gb`; the surrounding `@cap.File(...)` shape did not lower to typed IR."
                        .to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // (2) cap_file_accept_input_output_mismatch — per-feature, pair
    //     resource-field `@cap.File` inputs with api-output `@cap.File`
    //     outputs and require the accept sets to intersect.
    let mut by_feature: BTreeMap<&str, (Vec<&FileCapabilityFact>, Vec<&FileCapabilityFact>)> =
        BTreeMap::new();
    for fact in &operational.file_capability_facts {
        let entry = by_feature.entry(fact.feature.as_str()).or_default();
        match fact.binding {
            FileCapabilityBinding::ResourceField { .. } => entry.0.push(fact),
            FileCapabilityBinding::ApiOutput { .. } => entry.1.push(fact),
        }
    }
    for (_, (inputs, outputs)) in by_feature {
        if inputs.is_empty() || outputs.is_empty() {
            continue;
        }
        for output in &outputs {
            for input in &inputs {
                if !mime_sets_intersect(&output.capability.accept, &input.capability.accept) {
                    let api_name = match &output.binding {
                        FileCapabilityBinding::ApiOutput { api } => api.as_str(),
                        _ => "<unknown>",
                    };
                    let (resource_name, field_name) = match &input.binding {
                        FileCapabilityBinding::ResourceField { resource, field } => {
                            (resource.as_str(), field.as_str())
                        }
                        _ => ("<unknown>", "<unknown>"),
                    };
                    diagnostics.push(DoctorDiagnostic {
                        path: output.path.clone(),
                        line: output.line,
                        column: output.column,
                        severity: DoctorSeverity::Error,
                        code: "cap_file_accept_input_output_mismatch".to_owned(),
                        message: format!(
                            "api `{api_name}` output declares `@cap.File(accept:{})` but resource `{resource_name}.{field_name}` declares `@cap.File(accept:{})`; accept lists must intersect for the round-trip to be valid.",
                            format_accept_list(&output.capability.accept),
                            format_accept_list(&input.capability.accept),
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

    diagnostics
}

/// Wave B.4 — `query.view` is a typed SQL-backed screen-read primitive.
/// The analyzer lowers `source @file.<name>.sql` into the canonical
/// project-relative file path; doctor owns the filesystem and best-effort
/// unsafe-SQL checks.
pub(super) fn query_view_sql_file_diagnostics(
    facts: &[Tier3FeatureFacts],
    project_root: &Path,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for feature in facts {
        for query in &feature.queries {
            let lazuli_ir::Query::Sql(query) = query else {
                continue;
            };
            if query.sql_kind != lazuli_ir::SqlQueryKind::View {
                continue;
            }

            let line = feature
                .query_lines
                .get(query.name.as_str())
                .copied()
                .unwrap_or(feature.feature_line);
            let sql_path = resolve_query_view_sql_path(project_root, &query.sql_path);
            if !sql_path.is_file() {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "QUERY-VIEW-SQL-FILE-001".to_owned(),
                    message: format!(
                        "`query.view {}` references SQL source `{}` but the file does not exist.",
                        query.name, query.sql_path
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
                continue;
            }

            let Ok(sql) = fs::read_to_string(&sql_path) else {
                continue;
            };
            if let Some((sql_line, reason)) = query_view_unsafe_sql_line(&sql) {
                diagnostics.push(DoctorDiagnostic {
                    path: sql_path,
                    line: sql_line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "QUERY-VIEW-SQL-UNSAFE-001".to_owned(),
                    message: format!(
                        "`query.view {}` SQL looks like it builds user-influenced text instead of binding parameters: {reason}.",
                        query.name
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

    diagnostics
}

pub(super) fn resolve_query_view_sql_path(project_root: &Path, sql_path: &str) -> PathBuf {
    let path = Path::new(sql_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

pub(super) fn query_view_unsafe_sql_line(sql: &str) -> Option<(usize, &'static str)> {
    for (idx, line) in sql.lines().enumerate() {
        if line.contains("'%s'") || line.contains("\"%s\"") || line.contains("%s") {
            return Some((idx + 1, "`%s` formatting marker"));
        }
        if plus_near_dollar_placeholder(line) {
            return Some((idx + 1, "`+` near a `$<n>` placeholder"));
        }
    }
    None
}

pub(super) fn plus_near_dollar_placeholder(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (idx, byte) in bytes.iter().enumerate() {
        if *byte != b'$'
            || !bytes
                .get(idx + 1)
                .map(|b| b.is_ascii_digit())
                .unwrap_or(false)
        {
            continue;
        }
        let start = idx.saturating_sub(48);
        let end = (idx + 48).min(bytes.len());
        let window = &bytes[start..end];
        if window.contains(&b'+') && window.contains(&b'\'') {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
