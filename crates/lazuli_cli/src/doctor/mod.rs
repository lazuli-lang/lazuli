mod aggregators;
pub mod auth;
pub mod auth_refresh;
pub mod folder;
mod helpers;
pub mod lifecycle_gate;
pub mod lzx;
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
    collect_deprecated_exports, collect_lazuli_paths_recursive, derive_feature_name,
    is_ident_char, is_identifier, is_type_name, lazuli_version_line, matches_word, package_stem,
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
        let specs = parse_fail_on_specs(&opts.fail_on)
            .map_err(|e| anyhow::anyhow!("--fail-on: {e}"))?;
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
    let specs = parse_fail_on_specs(&opts.fail_on)
        .map_err(|e| anyhow::anyhow!("--fail-on: {e}"))?;
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
fn build_doctor_report(
    diagnostics: &[DoctorDiagnostic],
    want_coverage: bool,
    package: &DoctorPackage,
) -> crate::doctor_report::DoctorReport {
    use crate::doctor_report::{
        FindingBuilder, FindingJson, classify_result, DoctorReport, DoctorSummary, SpanJson,
        Severity as JsonSeverity,
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

#[derive(Debug)]
struct DoctorPackage {
    project_root: PathBuf,
    security_profile: SecurityProfile,
    /// `true` when `lazuli doctor` was invoked on a single `.lzi`/`.lzx`
    /// file rather than a project directory. Single-file mode skips
    /// project-level checks (e.g. `MANIFEST-REQUIRED-001`) that depend
    /// on having a real project root with `app.lzi` + `Lazurite.toml`.
    single_file_input: bool,
    lazurite_manifest: Option<Manifest>,
    files: Vec<DoctorFile>,
    workspace: Option<DoctorAppWorkspace>,
    contracts: Vec<DoctorAppContract>,
    app: Option<DoctorAppManifest>,
    registry: Option<DoctorAppRegistry>,
    profiles: Vec<DoctorAppProfile>,
    commands: BTreeMap<CommandKey, CommandPolicy>,
    experiences: BTreeMap<String, ExperienceFacts>,
    operational: OperationalFacts,
    /// Cut A: agent IR per feature, loaded through
    /// `lazuli_syntax::parse_feature_skeletons` +
    /// `lazuli_analyzer::lower_feature_skeleton`.
    agents: Vec<AgentFacts>,
    /// Cut A: per-feature enum/record/query/command symbol tables used
    /// for discriminator + tool-policy cross-resolution.
    feature_symbols: BTreeMap<String, FeatureSymbols>,
    /// Cut A: registry `tool <name>` headers that lacked `effect`.
    registry_tool_defects: Vec<RegistryToolDefect>,
    /// Phase L Tier 4b — minimal text-pattern walk of `approval` blocks
    /// inside command bodies. Only used for the `missing children`
    /// variant of `approval_contract_diagnostics`; every other approval
    /// check reads `Command.approval` from `Tier3FeatureFacts` (IR).
    /// The walker exists because parse-error approval blocks never
    /// reach the IR — they short-circuit the feature lift.
    approval_presences: Vec<ApprovalBlockPresence>,
    /// Phase L: lowered `auth` block per feature, paired with source
    /// line anchors for subblock-precise diagnostics.
    auth_facts: Vec<AuthFacts>,
    /// Phase L: per-feature resource declarations + field type text.
    /// Used to resolve `auth identity Customer.email` and
    /// `auth sessions resource CustomerSession` and to read
    /// `@cap.Hashed(algorithm:…)` axes off session resource fields.
    feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>>,
    /// Phase L: per-feature `extensions adapter <local>` declarations
    /// for the `auth_oauth_adapter_unbound` adapter resolution scope.
    feature_adapters: BTreeMap<String, BTreeSet<String>>,
    /// Phase L: per-feature `uses <other_feature>, ...` references so
    /// `auth identity Customer.email` in `feature customer_auth` can
    /// resolve `Customer` in `feature customer` when `uses customer` is
    /// declared.
    feature_uses: BTreeMap<String, BTreeSet<String>>,
    /// Phase L Tier 3: lifted `Job` / `Webhook` / `Notification` /
    /// `EventGroup` per feature, paired with source line anchors so the
    /// six new diagnostics (`JOB-*`, `WEBHOOK-SCOPE-*`,
    /// `NOTIF-CHANNEL-*`, `EVENTGROUP-NESTING-*`) attach to the right
    /// authoring site.
    tier3_facts: Vec<Tier3FeatureFacts>,
    /// PG.B — package-wide plan-and-gate facts (closed plan catalog,
    /// subscription anchor, per-callable gate directives). `None` when
    /// the package authors no `plan` blocks and no `gate` directives.
    plan_gate_facts: Option<lazuli_analyzer::PlanGateFacts>,
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
struct ResourcePreviousFact {
    /// Current resource name.
    current_name: String,
    /// Previously-known name(s).
    previous_names: Vec<String>,
    line: usize,
}

/// Migrations bucket cycle Route C — `Field` rename fact captured
/// from `previously migrated <old>` on a resource field.
#[derive(Debug, Clone)]
struct FieldPreviousFact {
    resource_name: String,
    current_name: String,
    previous_names: Vec<String>,
    line: usize,
}

impl DoctorPackage {
    fn load(input: &Path, security_profile: SecurityProfile) -> Result<Self> {
        let paths = collect_package_paths(input)?;
        if paths.is_empty() {
            bail!("no .lzi or .lzx files found for {}", input.display());
        }
        // Track whether the doctor was invoked on a single file vs a project
        // directory so project-level rules (MANIFEST-REQUIRED-001) can skip
        // when no real project root is present. Audit ref: R1.C sweep
        // produced 12 false positives because the parent dir of standalone
        // `.lzi` fixtures was scanned for @lazuli/plugin-* refs from sibling files.
        let single_file_input = input.is_file();
        let project_root = doctor_project_root(input);
        let lazurite_manifest = lazurite_manifest::load(&project_root).with_context(|| {
            format!(
                "failed to load {}",
                project_root.join("Lazurite.toml").display()
            )
        })?;

        let mut files = Vec::new();
        let mut workspace = None;
        let mut contracts = Vec::new();
        let mut app = None;
        let mut registry = None;
        let mut profiles = Vec::new();
        let mut commands = BTreeMap::new();
        let mut experiences = BTreeMap::new();
        let mut operational = OperationalFacts::default();
        let mut agents: Vec<AgentFacts> = Vec::new();
        let mut feature_symbols: BTreeMap<String, FeatureSymbols> = BTreeMap::new();
        let mut registry_tool_defects: Vec<RegistryToolDefect> = Vec::new();
        let mut approval_presences: Vec<ApprovalBlockPresence> = Vec::new();
        let mut auth_facts: Vec<AuthFacts> = Vec::new();
        let mut feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>> =
            BTreeMap::new();
        let mut feature_adapters: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut feature_uses: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut tier3_facts: Vec<Tier3FeatureFacts> = Vec::new();

        for path in paths {
            let source = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let mut file = DoctorFile {
                path,
                source,
                local_diagnostics: Vec::new(),
                lzx: None,
            };

            for diagnostic in
                lazuli_lsp::diagnostics_for_source_with_profile(&file.source, security_profile)
            {
                file.local_diagnostics
                    .push(DoctorDiagnostic::from_lsp(file.path.clone(), &diagnostic));
            }

            if is_lzi_path(&file.path) {
                contracts.extend(
                    parse_app_contracts(&file.source)
                        .into_iter()
                        .map(|manifest| DoctorAppContract {
                            path: file.path.clone(),
                            manifest,
                        }),
                );
                if let Some(manifest) = parse_app_workspace(&file.source) {
                    if workspace.is_none() {
                        workspace = Some(DoctorAppWorkspace {
                            path: file.path.clone(),
                            manifest,
                        });
                    } else {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: 1,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "WS-001".to_owned(),
                            message: "package should declare at most one workspace contract."
                                .to_owned(),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
                if let Some(manifest) = parse_app_manifest(&file.source) {
                    if app.is_none() {
                        app = Some(DoctorAppManifest {
                            path: file.path.clone(),
                            source: file.source.clone(),
                            manifest,
                        });
                    } else {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: 1,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "APP-001".to_owned(),
                            message: "package should declare exactly one app manifest entrypoint."
                                .to_owned(),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
                let RegistryParseOutput {
                    registry: parsed_registry,
                    tool_defects,
                } = parse_app_registry_with_defects(&file.source);
                if let Some(manifest) = parsed_registry {
                    if registry.is_none() {
                        registry = Some(DoctorAppRegistry {
                            path: file.path.clone(),
                            manifest,
                        });
                    } else {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: 1,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "REG-001".to_owned(),
                            message: "package should declare at most one registry manifest."
                                .to_owned(),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
                registry_tool_defects.extend(tool_defects.into_iter().map(|defect| {
                    RegistryToolDefect {
                        path: file.path.clone(),
                        line: defect.line,
                        name: defect.name,
                        reason: defect.reason,
                    }
                }));

                // Cut A — agent IR collection + feature symbol scan.
                match parse_feature_skeletons(&file.source) {
                    Ok(features) => {
                        for skeleton in &features {
                            match lower_feature_skeleton(skeleton) {
                                Ok(feature) => {
                                    let header_line =
                                        line_col_for_offset(&file.source, skeleton.span.start).0;
                                    let semantic_type_diagnostics =
                                        semantic_type_unknown_diagnostics_for_feature(
                                            &file.path,
                                            &file.source,
                                            &feature,
                                        );
                                    file.local_diagnostics.extend(semantic_type_diagnostics);
                                    let semantic_type_surface_diagnostics =
                                        semantic_type_unknown_diagnostics_for_syntax_feature(
                                            &file.path,
                                            &file.source,
                                            skeleton,
                                        );
                                    file.local_diagnostics
                                        .extend(semantic_type_surface_diagnostics);
                                    // MONEY-1 §3.2 — currency-tagged Money
                                    // doctor checks. Severity is fixed at
                                    // `Error` regardless of security profile
                                    // because mixed-currency arithmetic /
                                    // comparison is a structural bug (loses
                                    // money silently), not a style nit.
                                    file.local_diagnostics.extend(money_compare_001_diagnostics(
                                        &file.path, &feature,
                                    ));
                                    file.local_diagnostics.extend(
                                        money_arithmetic_001_diagnostics(&file.path, &feature),
                                    );
                                    // Wave 0 — wire VOCAB-TESTS-MISSING-001
                                    // through `DoctorPackage::load`'s
                                    // per-feature loop. The detector has
                                    // existed since 2026-05-15 but was
                                    // never invoked from any dispatcher;
                                    // see Issue Zero of
                                    // `docs/proposals/tdd-bdd-first-2026-05-23.md`.
                                    file.local_diagnostics.extend(
                                        vocab_tests_missing_001_diagnostics(
                                            &file.path,
                                            &feature,
                                            header_line,
                                            security_profile,
                                        ),
                                    );
                                    // Wave 1 — test-discipline + adjacent
                                    // runtime/migration lints. Seven rules
                                    // dispatched per-feature; the rule modules
                                    // live in `lazuli_doctor::test_discipline`.
                                    // Resolve handler app_root from manifest (defaults to project_root
                                    // when manifest absent or [lazurite].app_dir unset; handler rules
                                    // gracefully return empty when path doesn't exist).
                                    let app_root_for_handlers = lazurite_manifest
                                        .as_ref()
                                        .map(|m| m.app_root(&project_root))
                                        .unwrap_or_else(|| project_root.clone());
                                    // W1.5 — resolve [doctor.test_discipline].preset
                                    // once per feature loop. Under `tdd-iron-hand`,
                                    // every TEST-* / DOCTOR-* / MIGRATION-* / RUNTIME-*
                                    // rule fires at Error regardless of profile.
                                    let test_discipline_preset = lazurite_manifest
                                        .as_ref()
                                        .and_then(|m| m.doctor.as_ref())
                                        .and_then(|d| d.test_discipline.as_ref())
                                        .and_then(|td| td.preset.as_deref())
                                        .and_then(
                                            lazuli_doctor::test_discipline::preset::TestDisciplinePreset::parse,
                                        );
                                    file.local_diagnostics
                                        .extend(aggregators::test_discipline::diagnostics(
                                            &file.path,
                                            &project_root,
                                            &app_root_for_handlers,
                                            &feature,
                                            &file.source,
                                            security_profile,
                                            test_discipline_preset,
                                        ));
                                    // Tier 3 facts harvest — done before
                                    // `feature.agents` is consumed below.
                                    // Migrations bucket cycle Route C —
                                    // resource/field rename facts harvested
                                    // from the IR's `previous_names` slots.
                                    let mut resource_previous_names: Vec<ResourcePreviousFact> =
                                        Vec::new();
                                    let mut field_previous_names: Vec<FieldPreviousFact> =
                                        Vec::new();
                                    let mut all_resource_names_in_feature: BTreeSet<String> =
                                        BTreeSet::new();
                                    let mut all_field_names_in_feature: BTreeMap<
                                        String,
                                        BTreeSet<String>,
                                    > = BTreeMap::new();
                                    if !feature.resources.is_empty() {
                                        let resource_header_lines = collect_construct_lines(
                                            &file.source,
                                            "resource ",
                                            feature
                                                .resources
                                                .iter()
                                                .map(|r| r.name.as_str())
                                                .collect(),
                                        );
                                        for res in &feature.resources {
                                            all_resource_names_in_feature.insert(res.name.clone());
                                            let field_set = all_field_names_in_feature
                                                .entry(res.name.clone())
                                                .or_default();
                                            for fld in &res.fields {
                                                field_set.insert(fld.name.clone());
                                            }
                                            let res_line = resource_header_lines
                                                .get(&res.name)
                                                .copied()
                                                .unwrap_or(header_line);
                                            if !res.previous_names.is_empty() {
                                                resource_previous_names.push(
                                                    ResourcePreviousFact {
                                                        current_name: res.name.clone(),
                                                        previous_names: res.previous_names.clone(),
                                                        line: res_line,
                                                    },
                                                );
                                            }
                                            for field in &res.fields {
                                                if !field.previous_names.is_empty() {
                                                    field_previous_names.push(FieldPreviousFact {
                                                        resource_name: res.name.clone(),
                                                        current_name: field.name.clone(),
                                                        previous_names: field
                                                            .previous_names
                                                            .clone(),
                                                        line: res_line,
                                                    });
                                                }
                                            }
                                        }
                                    }

                                    // OpenAPI/Cache/i18n bucket cycles —
                                    // commands/queries/apis/translation are
                                    // always harvested (no gate); the buckets'
                                    // diagnostics walk over `Tier3FeatureFacts`
                                    // unconditionally. Doctor skips features
                                    // with empty slots in the diagnostic
                                    // bodies themselves.
                                    let has_text_pattern_api = file
                                        .source
                                        .lines()
                                        .any(|line| line.trim_start().starts_with("api "));
                                    if !feature.jobs.is_empty()
                                        || !feature.webhooks.is_empty()
                                        || !feature.notifications.is_empty()
                                        || !feature.event_groups.is_empty()
                                        || !feature.tenant_migrations.is_empty()
                                        || !resource_previous_names.is_empty()
                                        || !field_previous_names.is_empty()
                                        || !feature.commands.is_empty()
                                        || !feature.queries.is_empty()
                                        || !feature.apis.is_empty()
                                        || !feature.records.is_empty()
                                        || !feature.enums.is_empty()
                                        || !feature.reports.is_empty()
                                        || !feature.resources.is_empty()
                                        || feature.translation.is_some()
                                        || has_text_pattern_api
                                    {
                                        let job_lines = collect_construct_lines(
                                            &file.source,
                                            "job ",
                                            feature.jobs.iter().map(|j| j.name.as_str()).collect(),
                                        );
                                        let webhook_lines = collect_construct_lines(
                                            &file.source,
                                            "webhook ",
                                            feature
                                                .webhooks
                                                .iter()
                                                .map(|w| w.name.as_str())
                                                .collect(),
                                        );
                                        let notification_lines = collect_construct_lines(
                                            &file.source,
                                            "notification ",
                                            feature
                                                .notifications
                                                .iter()
                                                .map(|n| n.name.as_str())
                                                .collect(),
                                        );
                                        let tenant_migration_lines = collect_construct_lines(
                                            &file.source,
                                            "tenant_migration ",
                                            feature
                                                .tenant_migrations
                                                .iter()
                                                .map(|t| t.name.as_str())
                                                .collect(),
                                        );
                                        let event_group_lines = collect_event_group_lines(
                                            &file.source,
                                            feature
                                                .event_groups
                                                .iter()
                                                .map(|g| g.pattern.as_str())
                                                .collect(),
                                        );
                                        let command_lines = collect_construct_lines(
                                            &file.source,
                                            "command ",
                                            feature
                                                .commands
                                                .iter()
                                                .map(|c| c.name.as_str())
                                                .collect(),
                                        );
                                        let query_lines =
                                            collect_query_lines(&file.source, &feature.queries);
                                        let api_names_text_pattern =
                                            collect_text_pattern_api_names(&file.source);
                                        let api_lines = collect_construct_lines(
                                            &file.source,
                                            "api ",
                                            feature.apis.iter().map(|a| a.name.as_str()).collect(),
                                        );
                                        let report_lines = collect_construct_lines(
                                            &file.source,
                                            "report ",
                                            feature
                                                .reports
                                                .iter()
                                                .map(|r| r.name.as_str())
                                                .collect(),
                                        );
                                        let cache_lines = collect_construct_lines(
                                            &file.source,
                                            "cache ",
                                            feature
                                                .caches
                                                .iter()
                                                .map(|c| c.name.as_str())
                                                .collect(),
                                        );
                                        let translation_line = feature
                                            .translation
                                            .as_ref()
                                            .map(|_| {
                                                find_keyword_line(&file.source, "translation")
                                                    .unwrap_or(header_line)
                                            })
                                            .unwrap_or(header_line);
                                        // CL.C.4 — line lookup for aggregates so
                                        // domain diagnostics anchor at the
                                        // `aggregate <Name>` header.
                                        let mut aggregate_lines: BTreeMap<String, usize> =
                                            BTreeMap::new();
                                        for agg in &feature.aggregates {
                                            let agg_line = agg
                                                .span_ref
                                                .as_ref()
                                                .map(|s| {
                                                    line_col_for_offset(&file.source, s.start).0
                                                })
                                                .unwrap_or(header_line);
                                            aggregate_lines.insert(agg.name.clone(), agg_line);
                                        }
                                        tier3_facts.push(Tier3FeatureFacts {
                                            feature: feature.name.clone(),
                                            path: file.path.clone(),
                                            feature_line: header_line,
                                            tenancy_axis: tenancy_axis_for(&feature),
                                            defaults_policy: feature.defaults.policy.clone(),
                                            defaults_timestamps: feature.defaults.timestamps,
                                            jobs: feature.jobs.clone(),
                                            webhooks: feature.webhooks.clone(),
                                            notifications: feature.notifications.clone(),
                                            event_groups: feature.event_groups.clone(),
                                            tenant_migrations: feature.tenant_migrations.clone(),
                                            resource_previous_names,
                                            field_previous_names,
                                            all_resource_names_in_feature,
                                            all_field_names_in_feature,
                                            job_lines,
                                            webhook_lines,
                                            notification_lines,
                                            tenant_migration_lines,
                                            event_group_lines,
                                            commands: feature.commands.clone(),
                                            command_lines,
                                            queries: feature.queries.clone(),
                                            query_lines,
                                            caches: feature.caches.clone(),
                                            cache_lines,
                                            api_names_text_pattern,
                                            apis: feature.apis.clone(),
                                            api_lines,
                                            agents: feature.agents.clone(),
                                            translation: feature.translation.clone(),
                                            translation_line,
                                            records: feature.records.clone(),
                                            enums: feature.enums.clone(),
                                            events: feature.events.clone(),
                                            policies_declared: feature.policies.span_ref.is_some(),
                                            policies: feature.policies.clone(),
                                            extensions: feature.extensions.clone(),
                                            reports: feature.reports.clone(),
                                            report_lines,
                                            resources: feature.resources.clone(),
                                            report_decls: skeleton.reports.clone(),
                                            aggregates: feature.aggregates.clone(),
                                            aggregate_lines,
                                            errors: feature.errors.clone(),
                                            uses: feature.uses.clone(),
                                            channels: feature.channels.clone(),
                                        });
                                    }
                                    // Phase L Tier 4 follow-up — populate the
                                    // command policy/route map from the lifted
                                    // IR instead of the text-walker. Mirrors
                                    // the legacy `collect_feature_commands`
                                    // contract: only commands with `policy`
                                    // are inserted.
                                    populate_commands_from_ir(&feature, &mut commands);
                                    // Phase L Tier 4 follow-up — populate the
                                    // resource field map from typed IR.
                                    // Replaces `collect_feature_resources`
                                    // for `auth_*` cross-checks.
                                    populate_feature_resources_from_ir(
                                        &file.path,
                                        &file.source,
                                        &feature,
                                        &mut feature_resources,
                                    );
                                    // Phase L Tier 4 follow-up — emit the
                                    // typed command + job `external_calls`
                                    // facts (replaces the retired
                                    // `collect_external_calls_in_block`).
                                    populate_command_external_calls_from_ir(
                                        &file,
                                        &feature,
                                        &mut operational,
                                    );
                                    populate_job_external_calls_from_ir(
                                        &file,
                                        &feature,
                                        &mut operational,
                                    );
                                    for agent in feature.agents {
                                        let agent_line = agent
                                            .span_ref
                                            .as_ref()
                                            .map(|s| line_col_for_offset(&file.source, s.start).0)
                                            .unwrap_or(header_line);
                                        agents.push(AgentFacts {
                                            feature: feature.name.clone(),
                                            agent,
                                            path: file.path.clone(),
                                            line: agent_line,
                                        });
                                    }
                                    if let Some(auth) = feature.auth {
                                        let auth_line = auth
                                            .span_ref
                                            .as_ref()
                                            .map(|s| line_col_for_offset(&file.source, s.start).0)
                                            .unwrap_or(header_line);
                                        let anchors = collect_auth_anchors(&file.source, auth_line);
                                        auth_facts.push(AuthFacts {
                                            feature: feature.name.clone(),
                                            auth,
                                            path: file.path.clone(),
                                            line: auth_line,
                                            identity_line: anchors.identity_line,
                                            password_line: anchors.password_line,
                                            password_algorithm_line: anchors
                                                .password_algorithm_line,
                                            sessions_line: anchors.sessions_line,
                                            sessions_resource_line: anchors.sessions_resource_line,
                                            mfa_line: anchors.mfa_line,
                                            oauth_lines: anchors.oauth_lines,
                                        });
                                    }
                                }
                                Err(error) => {
                                    file.local_diagnostics.push(DoctorDiagnostic {
                                        path: file.path.clone(),
                                        line: line_col_for_offset(
                                            &file.source,
                                            skeleton.span.start,
                                        )
                                        .0,
                                        column: 1,
                                        severity: DoctorSeverity::Error,
                                        code: "agent_lower_failed_diagnostics".to_owned(),
                                        message: format!("agent lowering failed: {error}"),
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
                    Err(error) => {
                        file.local_diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: line_col_for_offset(&file.source, error.span().start).0,
                            column: line_col_for_offset(&file.source, error.span().start).1,
                            severity: DoctorSeverity::Error,
                            code: "agent_parse_failed_diagnostics".to_owned(),
                            message: error.to_string(),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
                collect_approval_block_presence(&file, &mut approval_presences);
                collect_feature_adapters(&file, &mut feature_adapters);
                collect_feature_uses(&file, &mut feature_uses);
                profiles.extend(parse_app_profiles(&file.source).into_iter().map(|profile| {
                    DoctorAppProfile {
                        path: file.path.clone(),
                        profile,
                    }
                }));
                collect_canonical_facts(&file, &mut operational);
            } else if is_lzx_path(&file.path) {
                match lazuli_syntax::parse_lzx_document(&file.source) {
                    Ok(document) => {
                        collect_lzx_experience_facts(&document, &mut experiences);
                        collect_lzx_operational_facts(&file, &document, &mut operational);
                        // Wave 3.5 + Wave 4 — view test_discipline rules. All
                        // operate on the LzxDocument or its lowered IR.
                        // Severity per profile: prototype=info, strict=warning,
                        // production=error. W1.5: under
                        // [doctor.test_discipline].preset = "tdd-iron-hand",
                        // every rule below escalates to Error.
                        if security_profile != SecurityProfile::Prototype {
                            let severity_warn = match security_profile {
                                SecurityProfile::Production => DoctorSeverity::Error,
                                _ => DoctorSeverity::Warning,
                            };
                            // W1.5 — resolve preset once per .lzx file.
                            let lzx_test_discipline_preset = lazurite_manifest
                                .as_ref()
                                .and_then(|m| m.doctor.as_ref())
                                .and_then(|d| d.test_discipline.as_ref())
                                .and_then(|td| td.preset.as_deref())
                                .and_then(
                                    lazuli_doctor::test_discipline::preset::TestDisciplinePreset::parse,
                                );
                            // Wave 3.5 — TEST-VIEW-E2E-MISSING-001 (file-pair).
                            // Doctor does NOT invoke Playwright; only Path::exists().
                            let view_e2e_code = lazuli_doctor::test_discipline::test_view_e2e_missing_001
                                ::Finding::CODE;
                            for finding in
                                lazuli_doctor::test_discipline::test_view_e2e_missing_001::check(
                                    &document,
                                    &file.path,
                                    &file.source,
                                    &project_root,
                                )
                            {
                                let message = finding.message();
                                file.local_diagnostics.push(DoctorDiagnostic {
                                    path: finding.path,
                                    line: finding.line,
                                    column: finding.column,
                                    severity: resolve_test_discipline_severity(
                                        severity_warn,
                                        view_e2e_code,
                                        lzx_test_discipline_preset,
                                    ),
                                    code: view_e2e_code.to_owned(),
                                    message,
                                    category: None,
                                    feature_name: None,
                                    construct: None,
                                    fix: None,
                                    group: None,
                                });
                            }
                            // Wave 4 — TEST-VIEW-EXTENSIBILITY-001 + TEST-VIEW-DRIFT-001.
                            // Both walk the lowered ExperienceModule.
                            let experience_module =
                                lazuli_analyzer::lower_lzx_document(&document);
                            let view_ext_code = lazuli_doctor::test_discipline::test_view_extensibility_001
                                ::Finding::CODE;
                            for finding in
                                lazuli_doctor::test_discipline::test_view_extensibility_001::check(
                                    &experience_module,
                                    &file.path,
                                )
                            {
                                let message = finding.message();
                                file.local_diagnostics.push(DoctorDiagnostic {
                                    path: finding.path,
                                    line: 1,
                                    column: 1,
                                    severity: resolve_test_discipline_severity(
                                        severity_warn,
                                        view_ext_code,
                                        lzx_test_discipline_preset,
                                    ),
                                    code: view_ext_code.to_owned(),
                                    message,
                                    category: None,
                                    feature_name: None,
                                    construct: None,
                                    fix: None,
                                    group: None,
                                });
                            }
                            let view_drift_code = lazuli_doctor::test_discipline::test_view_drift_001
                                ::Finding::CODE;
                            for finding in
                                lazuli_doctor::test_discipline::test_view_drift_001::check(
                                    &experience_module,
                                    &file.path,
                                )
                            {
                                let message = finding.message();
                                file.local_diagnostics.push(DoctorDiagnostic {
                                    path: finding.path,
                                    line: 1,
                                    column: 1,
                                    severity: resolve_test_discipline_severity(
                                        DoctorSeverity::Error,
                                        view_drift_code,
                                        lzx_test_discipline_preset,
                                    ),
                                    code: view_drift_code.to_owned(),
                                    message,
                                    category: None,
                                    feature_name: None,
                                    construct: None,
                                    fix: None,
                                    group: None,
                                });
                            }
                        }
                        file.lzx = Some(document);
                    }
                    Err(error) => file.local_diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: line_col_for_offset(&file.source, error.span().start).0,
                        column: line_col_for_offset(&file.source, error.span().start).1,
                        severity: DoctorSeverity::Error,
                        code: "LZX-PARSE".to_owned(),
                        message: error.to_string(),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    }),
                }
            }

            files.push(file);
        }

        // Phase L Tier 4 follow-up (final wave) — `feature_symbols.commands`
        // is now populated from the typed `Tier3FeatureFacts.commands` slot
        // built earlier in this loop. Replaces the per-file
        // `collect_feature_symbols` text walker. Runs once after every file
        // has lifted its IR slice so cross-feature command policy hints are
        // available to `agent_tool_diagnostics`.
        populate_feature_symbols_from_ir(&tier3_facts, &mut feature_symbols);

        // PG.B — aggregate package-wide plan-and-gate facts. Walks every
        // .lzi source once to collect top-level `plan` blocks +
        // per-feature `gate ...` directives, and reads the subscription
        // anchor from app.lzi. The output is consumed by the doctor
        // diagnostics pass below and by codegen later.
        let mut plan_blocks_raw: Vec<lazuli_syntax::PlanBlockAst> = Vec::new();
        let mut feature_gates_raw: Vec<(String, lazuli_syntax::FeatureGatesAst)> = Vec::new();
        for file in &files {
            if !is_lzi_path(&file.path) {
                continue;
            }
            if let Ok(blocks) = lazuli_syntax::parse_plan_blocks(&file.source) {
                plan_blocks_raw.extend(blocks);
            }
            if let Ok(fg) = lazuli_syntax::parse_feature_gates(&file.source) {
                if !fg.callables.is_empty() {
                    // Derive feature name from the file's first
                    // `feature <name>` header (mirrors the existing
                    // doctor convention).
                    let feature_name = derive_feature_name(&file.source).unwrap_or_else(|| {
                        file.path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_owned()
                    });
                    feature_gates_raw.push((feature_name, fg));
                }
            }
        }
        let anchor = app
            .as_ref()
            .and_then(|a| lazuli_analyzer::parse_subscription_anchor(&a.source));
        let plan_gate_facts =
            if plan_blocks_raw.is_empty() && feature_gates_raw.is_empty() && anchor.is_none() {
                None
            } else {
                Some(lazuli_analyzer::aggregate_plan_gate_facts(
                    &plan_blocks_raw,
                    &feature_gates_raw,
                    anchor,
                ))
            };

        Ok(Self {
            project_root,
            security_profile,
            single_file_input,
            lazurite_manifest,
            files,
            workspace,
            contracts,
            app,
            registry,
            profiles,
            commands,
            experiences,
            operational,
            agents,
            feature_symbols,
            registry_tool_defects,
            approval_presences,
            auth_facts,
            feature_resources,
            feature_adapters,
            feature_uses,
            tier3_facts,
            plan_gate_facts,
        })
    }

    /// Wave 6 — extract `(features, lzx_views)` from the loaded package
    /// for `lazuli_doctor::coverage::build_coverage_report`. Re-parses
    /// `.lzi` sources (cheap; the existing `parse_feature_skeletons` +
    /// `lower_feature_skeleton` are already cached at the syntax layer
    /// for the per-feature loop, so this second pass is mostly metadata
    /// extraction). Walks `file.lzx` documents directly for view refs.
    fn coverage_inputs(&self) -> (Vec<lazuli_ir::Feature>, Vec<lazuli_doctor::coverage::LzxViewRef>) {
        let mut features: Vec<lazuli_ir::Feature> = Vec::new();
        let mut lzx_views: Vec<lazuli_doctor::coverage::LzxViewRef> = Vec::new();
        for file in &self.files {
            if is_lzi_path(&file.path) {
                if let Ok(skeletons) = parse_feature_skeletons(&file.source) {
                    for skeleton in &skeletons {
                        if let Ok(feature) = lower_feature_skeleton(skeleton) {
                            features.push(feature);
                        }
                    }
                }
            } else if is_lzx_path(&file.path)
                && let Some(document) = &file.lzx
            {
                for experience in &document.experiences {
                    for view in &experience.views {
                        lzx_views.push(lazuli_doctor::coverage::LzxViewRef {
                            experience: experience.name.clone(),
                            view: view.name.clone(),
                        });
                    }
                }
            }
        }
        (features, lzx_views)
    }

    /// Iron-hand meta-bundle — return the active `[doctor.coverage]
    /// preset` parsed into `CoveragePreset`, or `None` when no manifest
    /// / no preset / unknown preset name. Drives both the coverage
    /// thresholds and the rule-severity escalation map applied by
    /// dispatchers (see `context_vocab_diagnostics`).
    fn coverage_preset(&self) -> Option<lazuli_doctor::coverage::CoveragePreset> {
        use lazuli_doctor::coverage::CoveragePreset;
        self.lazurite_manifest
            .as_ref()
            .and_then(|m| m.doctor.as_ref())
            .and_then(|d| d.coverage.as_ref())
            .and_then(|cov| cov.preset.as_deref())
            .and_then(CoveragePreset::parse)
    }

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
    fn context_vocab_diagnostics(&self) -> Vec<DoctorDiagnostic> {
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
                for finding in vocab_context_ctxmd_001::check(
                    &feature,
                    &file.path,
                    Some(&self.project_root),
                ) {
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
            build_coverage_report_with_e2e_root, resolve_coverage_thresholds, CoveragePreset,
            CoverageProfile, LayerThreshold,
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
                let preset = cov
                    .preset
                    .as_deref()
                    .and_then(CoveragePreset::parse);
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

    fn diagnostics(&self) -> Vec<DoctorDiagnostic> {
        let mut diagnostics = Vec::new();

        diagnostics.extend(manifest_required_diagnostics(
            &self.project_root,
            self.single_file_input,
        ));
        diagnostics.extend(lazurite_manifest_diagnostics(self));

        // Iron-hand context-vocabulary lints (VOCAB-CONTEXT-PURPOSE-001,
        // VOCAB-CONTEXT-NONGOALS-001, VOCAB-CONTEXT-CTXMD-001). Severity
        // resolves through: manifest override > preset escalation
        // (iron-hand promotes to error) > category default.
        diagnostics.extend(self.context_vocab_diagnostics());

        // PG.B — plan-and-gate cross-feature checks.
        if let Some(facts) = &self.plan_gate_facts {
            let eval_order_inputs = collect_callable_bodies_for_eval_order(&self.files);
            for diag in lazuli_analyzer::diagnose_plan_gate_facts(facts, &eval_order_inputs) {
                let path = self
                    .app
                    .as_ref()
                    .map(|a| a.path.clone())
                    .or_else(|| self.files.first().map(|f| f.path.clone()))
                    .unwrap_or_else(|| self.project_root.clone());
                diagnostics.push(DoctorDiagnostic {
                    path,
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: diag.code.as_str().to_owned(),
                    message: diag.message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        let declared_env_names: BTreeSet<&str> = self
            .app
            .as_ref()
            .map(|app| operational_env_names(&app.manifest, self.registry.as_ref()))
            .unwrap_or_default();
        for file in &self.files {
            let after_dedupe = dedupe_env_contract_diagnostics(&file.local_diagnostics);
            diagnostics.extend(suppress_env_schema_when_declared(
                &after_dedupe,
                &declared_env_names,
            ));
        }
        diagnostics.extend(vocab_grammar_form_diagnostics(
            &self.files,
            self.security_profile,
        ));

        diagnostics.extend(lazuli_version_001_diagnostics(
            self.app.as_ref(),
            LZIR_SCHEMA,
        ));
        diagnostics.extend(lazuli_version_002_diagnostics(
            self.app.as_ref(),
            LZIR_SCHEMA,
            &self.project_root,
        ));

        diagnostics.extend(policy_reachability_diagnostics(
            &self.files,
            &self.experiences,
            &self.commands,
        ));
        diagnostics.extend(cap_file_policy_implicit_diagnostics(&self.tier3_facts));
        diagnostics.extend(schema_rich_gap_diagnostics(&self.tier3_facts));
        diagnostics.extend(manual_param_coercion_diagnostics(&self.project_root));
        diagnostics.extend(import_deprecated_alias_diagnostics(&self.project_root));
        diagnostics.extend(duplicate_query_name_diagnostics(&self.tier3_facts));
        diagnostics.extend(missing_policy_on_query_diagnostics(&self.tier3_facts));
        diagnostics.extend(mutation_without_readback_diagnostics(&self.tier3_facts));
        diagnostics.extend(updates_missing_updated_at_diagnostics(&self.tier3_facts));
        diagnostics.extend(route_id_effect_consistency_diagnostics(&self.tier3_facts));
        // Cycle-2 cell DC1 — sweep the rest of `lazuli_doctor::correctness`
        // into the doctor dispatch so `lazuli doctor` reaches every
        // diagnostic the crate carries (the 11 sibling rules that until
        // now only fired in their `#[cfg(test)] mod tests`).
        diagnostics.extend(aggregators::correctness::diagnostics(
            &self.tier3_facts,
            self.registry.as_ref().map(|reg| &reg.manifest),
            &self.project_root,
            self.security_profile,
            self.single_file_input,
        ));
        diagnostics.extend(returns_list_001::diagnostics(
            &self.tier3_facts,
            &self.project_root,
        ));
        diagnostics.extend(returns_list_002::diagnostics(&self.tier3_facts));
        diagnostics.extend(app_contract_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref(),
            &self.profiles,
            &self.operational,
        ));
        diagnostics.extend(workspace_contract_diagnostics(self.workspace.as_ref()));
        diagnostics.extend(external_contract_diagnostics(
            &self.contracts,
            self.workspace.as_ref(),
        ));

        // Cut A — agent + tool + eval + discriminator cross-feature checks.
        diagnostics.extend(registry_tool_effect_diagnostics(
            &self.registry_tool_defects,
        ));
        diagnostics.extend(agent_tool_diagnostics(
            &self.agents,
            &self.feature_symbols,
            self.registry.as_ref(),
            &self.tier3_facts,
        ));
        diagnostics.extend(agent_discriminator_diagnostics(
            &self.agents,
            &self.tier3_facts,
        ));
        diagnostics.extend(agent_eval_diagnostics(&self.agents));
        diagnostics.extend(cross_feature_type_unresolved_diagnostics(
            &self.files,
            &self.tier3_facts,
            &self.feature_resources,
        ));
        diagnostics.extend(feature_uses_missing_diagnostics(
            &self.files,
            &self.tier3_facts,
            &self.feature_resources,
            &self.feature_uses,
        ));

        // Cut A.7 — `expose http` cross-feature checks.
        let known_audiences = collect_known_audiences(&self.files);
        diagnostics.extend(agent_expose_diagnostics(
            &self.agents,
            &self.tier3_facts,
            &known_audiences,
        ));

        // Cut A.8 — built-in trace event reservation + subscriber
        // payload drift checks.
        diagnostics.extend(agent_run_trace_diagnostics(&self.files));

        // Observability bucket cycle row 37 — `audit emit_to`
        // resolution, `event.trace level` closed catalog, and health
        // probe path shape. Phase L Tier 4b — `audit emit_to` for
        // commands is now IR-driven via `tier3_facts`; the text walker
        // is narrowed to skip command bodies.
        diagnostics.extend(audit_event_health_diagnostics(
            &self.files,
            self.app.as_ref(),
            &self.tier3_facts,
        ));
        diagnostics.extend(resource_policy_and_command_audit_hints(
            &self.tier3_facts,
            &self.feature_resources,
        ));

        // RB.B — RBAC catalog diagnostics.
        // Run BEFORE legacy `collect_known_roles`/approval checks so
        // `@role.*` resolution uses the catalog when present and falls
        // back to text-walk only when no catalog is declared.
        let (rbac_diags, rbac_catalog) = rbac_catalog_diagnostics(&self.files);
        diagnostics.extend(rbac_diags);
        if let Some(catalog) = &rbac_catalog {
            diagnostics.extend(rbac_role_undeclared_diagnostics(&self.files, catalog));
        }
        diagnostics.extend(rbac_catalog_missing_diagnostics(
            &self.files,
            rbac_catalog.is_some(),
        ));
        diagnostics.extend(rbac_missing_policy_diagnostics(&self.files));

        // Cut A.9 — `approval` primitive contract + role resolution.
        // When the RBAC catalog is present, prefer its role set; fall
        // back to the legacy `collect_known_roles` text walk when no
        // catalog is declared (back-compat per
        // `docs/proposals/rbac-catalog-vocab.md` §Backwards compatibility).
        let known_roles = if let Some(catalog) = &rbac_catalog {
            catalog.roles.iter().map(|r| r.name.clone()).collect()
        } else {
            collect_known_roles(&self.files)
        };
        diagnostics.extend(approval_diagnostics(&self.tier3_facts, &known_roles));
        diagnostics.extend(scope_owner_column_diagnostics(&self.tier3_facts));
        diagnostics.extend(field_derived_from_unresolved_diagnostics(&self.tier3_facts));
        diagnostics.extend(resource_unique_qualifier_unknown_diagnostics(
            &self.tier3_facts,
        ));
        diagnostics.extend(resource_validates_path_unknown_diagnostics(
            &self.tier3_facts,
        ));
        diagnostics.extend(approval_missing_children_diagnostics(
            &self.approval_presences,
        ));

        diagnostics.extend(app_urls_missing_diagnostics(self.app.as_ref()));

        // Cut A.11 — `cors` block cross-checks against the app's
        // declared environments + urls.
        diagnostics.extend(aggregators::cors::diagnostics(self.app.as_ref()));

        // Roadmap §1.2 — HTTP hygiene contracts: cookie / proxy /
        // limits. Each block's typed lift is doctor-validated against
        // the closed catalog (same_site, parseable CIDR/size/duration).
        diagnostics.extend(aggregators::http_hygiene::cookie_diagnostics(
            self.app.as_ref(),
        ));
        diagnostics.extend(aggregators::http_hygiene::proxy_diagnostics(
            self.app.as_ref(),
        ));
        diagnostics.extend(aggregators::http_hygiene::limits_diagnostics(
            self.app.as_ref(),
        ));

        // Roadmap §1.10 — `app.headers` production-completeness +
        // closed-catalog gate.
        diagnostics.extend(aggregators::headers_secrets::headers_diagnostics(
            self.app.as_ref(),
            self.security_profile,
        ));
        // Roadmap §1.10 — `secret_rotation` overlap + binding
        // cross-check.
        diagnostics.extend(aggregators::headers_secrets::secret_rotation_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));

        // Observability bucket cycle row 36 — `app.logging` and
        // `app.tracing` closed-catalog + range + exporter binding
        // checks.
        diagnostics.extend(aggregators::observability::logging_tracing_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));
        diagnostics.extend(aggregators::observability::app_diagnostics(
            self.app.as_ref(),
        ));

        // Phase L — auth block cross-feature diagnostics.
        diagnostics.extend(auth_diagnostics(
            &self.auth_facts,
            &self.feature_resources,
            &self.feature_adapters,
            &self.feature_uses,
            self.registry.as_ref(),
        ));
        diagnostics.extend(auth_refresh::diagnostics(
            &self.auth_facts,
            &self.feature_resources,
            &self.feature_uses,
            self.app.as_ref(),
            &self.files,
        ));
        diagnostics.extend(check_auth_session_callsite_001(
            &self.auth_facts,
            &self.project_root,
        ));

        // Row 30 — Storage bucket cycle: 5 typed `@cap.File`
        // diagnostics. See `docs/proposals/bucket-storage-cycle.md`
        // §Doctor/LSP.
        diagnostics.extend(cap_file_storage_diagnostics(&self.operational));

        // Row 33 — Jobs bucket cycle: six IR-driven diagnostics on the
        // Tier 3 lift (`JOB-TIMEOUT-001`, `JOB-FANOUT-001`,
        // `JOB-FANOUT-002`, `WEBHOOK-SCOPE-001`, `NOTIF-CHANNEL-001`,
        // `EVENTGROUP-NESTING-001`). See
        // `docs/proposals/bucket-jobs-cycle.md` §Doctor/LSP.
        //
        // Rows 38–39 — Webhooks expanded cycle: eight additional
        // IR-driven diagnostics (`WEBHOOK-PAYLOAD-001/002`,
        // `WEBHOOK-REPLAY-001/002`, `WEBHOOK-DLQ-001/002/003`,
        // `WEBHOOK-EVENT-001`). Threaded through the same
        // `tier3_diagnostics` entry-point so the iteration over
        // feature webhooks stays single-pathed.
        diagnostics.extend(tier3_diagnostics(
            &self.tier3_facts,
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));
        diagnostics.extend(aggregators::webhook_event_registry::diagnostics(
            self.registry.as_ref(),
        ));

        // Row 34 — `event_group` pattern-prefix rule promoted from LSP
        // to doctor now that `EventGroup` IR exists.
        diagnostics.extend(aggregators::event_group::diagnostics(&self.tier3_facts));

        // Rows 41-43 — Migrations bucket cycle Route C: eight new
        // IR-driven diagnostics covering rename hints, the
        // `tenant_migration` kind, and the deploy block expansion. See
        // `docs/proposals/bucket-migrations-cycle.md` §Doctor.
        diagnostics.extend(aggregators::migrations::diagnostics(
            &self.tier3_facts,
            self.app.as_ref(),
        ));

        // Row 48 — OpenAPI bucket cycle: five `deprecated_*` + text-pattern
        // api detection. See `docs/proposals/bucket-openapi-cycle.md`
        // §Doctor/LSP.
        diagnostics.extend(aggregators::deprecated::diagnostics(&self.tier3_facts));

        // Row 51 — Cache bucket cycle: five `cache_*` diagnostics. See
        // `docs/proposals/bucket-cache-cycle.md` §Doctor/LSP.
        diagnostics.extend(aggregators::cache::diagnostics(
            &self.tier3_facts,
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));
        diagnostics.extend(query_view_sql_file_diagnostics(
            &self.tier3_facts,
            &self.project_root,
        ));

        // Row 54 — i18n bucket cycle: 15 locale/translation diagnostics.
        // See `docs/proposals/bucket-i18n-cycle.md` §Doctor/LSP.
        diagnostics.extend(aggregators::i18n::diagnostics(
            &self.tier3_facts,
            self.app.as_ref(),
            &self.files,
        ));
        diagnostics.extend(check_codegen_wrap_001(&self.project_root));
        diagnostics.extend(
            schema_rich_001::check(&self.project_root)
                .into_iter()
                .map(|finding| DoctorDiagnostic {
                    path: doctor_rule_path(&self.project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: schema_rich_001::Finding::CODE.to_owned(),
                    message: finding.message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }),
        );
        diagnostics.extend(check_pattern_draft_stale_001(&self.project_root));

        // Report vocab — 10 doctor codes per
        // `docs/proposals/report-vocab.md` v0.2 §Doctor / LSP. The
        // capability-aware rules (`REPORT-SIGNED-NO-STORAGE-001`,
        // `REPORT-STORAGE-AMBIGUOUS-001`) read object_storage caps from
        // the package's app manifest + registry.
        diagnostics.extend(report_diagnostics(
            &self.tier3_facts,
            self.app.as_ref().map(|a| &a.manifest),
            self.registry.as_ref(),
        ));

        // CL.C.4 — domain-model diagnostics (roadmap §1.7). Four codes:
        // `AGGREGATE-ROOT-UNKNOWN`, `AGGREGATE-CONTAINS-UNKNOWN`,
        // `INVARIANT-PREDICATE-INVALID`, `SLUG-UNIQUENESS-IMPLICIT`.
        diagnostics.extend(aggregators::domain::diagnostics(&self.tier3_facts));

        // IR Error-Vocab (Cell ANALYZE-1) — 7 typed `ERR-VOCAB-*` codes
        // per `docs/proposals/ir-error-messages-vocab.md` §6. Operates
        // on the lowered IR carried in `tier3_facts`; `files` is passed
        // for `SpanRef -> line` resolution so each diagnostic anchors at
        // the offending construct.
        diagnostics.extend(aggregators::error_vocab::diagnostics(
            &self.tier3_facts,
            &self.files,
        ));
        diagnostics.extend(route_guard::diagnostics(
            &self.files,
            self.app.as_ref(),
            &self.tier3_facts,
        ));
        diagnostics.extend(lifecycle_gate::diagnostics(
            &self.files,
            self.app.as_ref(),
            &self.tier3_facts,
        ));

        diagnostics.extend(aggregators::folder::diagnostics(
            &self.project_root,
            self.security_profile,
        ));
        diagnostics.extend(aggregators::design::diagnostics(
            &self.project_root,
            self.security_profile,
        ));

        diagnostics.sort_by(|left, right| {
            left.code
                .cmp(&right.code)
                .then(left.path.cmp(&right.path))
                .then(left.line.cmp(&right.line))
                .then(left.column.cmp(&right.column))
        });

        // B3 — suppress legacy `semantic_type_unknown` errors for any
        // `@semantic.<Name>` that resolves through a plugin manifest
        // alias map. The legacy diagnostic was authored against the
        // closed catalog; the plugin-locales proposal augments the
        // catalog without touching the diagnostic site. Filter here
        // rather than threading the alias set down to every emission
        // call site so the existing walker code stays untouched.
        // See `docs/proposals/semantic-types-plugin-locales.md`
        // §New diagnostics.
        if let Some(manifest) = self.lazurite_manifest.as_ref() {
            if let Ok(alias_map) =
                crate::plugin_manifest::build_alias_map(Some(manifest), &self.project_root)
            {
                if !alias_map.is_empty() {
                    diagnostics.retain(|d| {
                        if d.code != "semantic_type_unknown" {
                            return true;
                        }
                        // Match the alias name out of the diagnostic
                        // message (`unknown @semantic type "@semantic.X"; ...`).
                        // The message format is fixed; if it changes,
                        // this filter has a single update site.
                        let alias = d
                            .message
                            .split_once('"')
                            .and_then(|(_, rest)| rest.split_once('"').map(|(a, _)| a))
                            .unwrap_or("");
                        !alias_map.contains_key(alias)
                    });
                }
            }
        }

        diagnostics
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
fn doctor_severity_for(
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

fn vocab_grammar_form_diagnostics(
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
fn money_compare_001_diagnostics(
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
fn vocab_tests_missing_001_diagnostics(
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
fn money_arithmetic_001_diagnostics(
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
fn check_codegen_wrap_001(project_root: &Path) -> Vec<DoctorDiagnostic> {
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
fn check_pattern_draft_stale_001(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    check_pattern_draft_stale_001_at(project_root, now)
}

fn check_pattern_draft_stale_001_at(project_root: &Path, now: u64) -> Vec<DoctorDiagnostic> {
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
fn check_auth_session_callsite_001(
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

fn collect_issue_session_callsites(
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

fn is_pattern_draft_line(line: &str) -> bool {
    if !line.contains("draft") {
        return false;
    }
    (line.contains("PATTERN_") && line.contains("\"draft\"")) || line.contains("//lazuli:pattern")
}

fn git_blame_author_time(project_root: &Path, path: &Path, line: usize) -> Option<u64> {
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

fn collect_codegen_wrap_001(
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

fn is_bucket_go_source(bucket_root: &Path, path: &Path) -> bool {
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
struct DoctorAppWorkspace {
    path: PathBuf,
    manifest: AppWorkspace,
}

#[derive(Debug)]
struct DoctorAppContract {
    path: PathBuf,
    manifest: AppContract,
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
struct AgentFacts {
    feature: String,
    agent: Agent,
    path: PathBuf,
    /// 1-based source line where the `agent <name>` header lives.
    line: usize,
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
struct AuthFacts {
    feature: String,
    auth: ir::Auth,
    path: PathBuf,
    /// 1-based line for the `auth` header.
    line: usize,
    identity_line: usize,
    password_line: Option<usize>,
    password_algorithm_line: Option<usize>,
    sessions_line: Option<usize>,
    sessions_resource_line: Option<usize>,
    mfa_line: Option<usize>,
    /// Per-provider `oauth <provider>` header line.
    oauth_lines: BTreeMap<String, usize>,
}

/// Phase L Tier 4 follow-up — both `records` and `enums` slots retired
/// (lifted into `Tier3FeatureFacts.records` / `Tier3FeatureFacts.enums`
/// from the typed IR). The struct now carries only the command policy
/// hint that `agent_tool_diagnostics` still text-walks while the legacy
/// pipeline owns surface commands.
#[derive(Debug, Clone, Default)]
struct FeatureSymbols {
    /// Maps short command name (e.g. `archive`) to its registered policy
    /// + safety hint. Commands are inherently write-effect for Cut A.
    commands: BTreeMap<String, CommandSymbolFact>,
}

#[derive(Debug, Clone)]
struct SymbolFact {
    path: PathBuf,
    line: usize,
}

/// Phase L Tier 4 follow-up — typed shape of a `resource <Name>`
/// declaration for the `auth_*` cross-checks. Now populated from the
/// IR `Feature.resources` lift instead of a text walker; the
/// `type_ref` slot carries `TypeRef::Capability(CapabilityRef::Hashed(...))`
/// directly so `cap_hashed_algorithm` is a typed match.
#[derive(Debug, Clone, Default)]
struct ResourceFact {
    path: PathBuf,
    line: usize,
    fields: BTreeMap<String, ResourceFieldFact>,
}

#[derive(Debug, Clone)]
struct ResourceFieldFact {
    /// Typed `TypeRef` lifted from `Field.type_ref`. `cap_hashed_algorithm`
    /// matches `TypeRef::Capability(CapabilityRef::Hashed(...))`;
    /// `is_identity_shaped` matches `Builtin::SemanticEmail/SemanticPhone`
    /// + `Builtin::Id` + the typed `unique` axis.
    type_ref: lazuli_ir::TypeRef,
    /// `Field.unique`. Used by `is_identity_shaped` for unique-shaped
    /// identity detection.
    unique: bool,
    /// 1-based line where the field is declared. Currently unused by
    /// diagnostics; reserved for future field-anchored messages.
    #[allow(dead_code)]
    line: usize,
}

#[derive(Debug, Clone)]
struct CommandSymbolFact {
    base: SymbolFact,
    policy: Option<String>,
}

#[derive(Debug, Clone)]
struct RegistryToolDefect {
    path: PathBuf,
    line: usize,
    name: String,
    reason: RegistryToolDefectReason,
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
struct OperationalFacts {
    features: BTreeMap<String, SourceFact>,
    integration_requirements: Vec<IntegrationRequirementFact>,
    external_calls: Vec<ExternalCallFact>,
    env_references: Vec<SourceFact>,
    file_capabilities: Vec<SourceFact>,
    /// Row 30 — typed `@cap.File(...)` sites carrying the lowered
    /// `FileCapability` + origin + binding context (`ResourceField` or
    /// `ApiOutput`). Populated alongside `file_capabilities` so the
    /// storage diagnostics can run against typed IR shape, while the
    /// existing text-pattern fact powers the `APP-CAP-001` check.
    file_capability_facts: Vec<FileCapabilityFact>,
    jobs: Vec<SourceFact>,
    schedules: Vec<SourceFact>,
    webhooks: Vec<SourceFact>,
    apis: Vec<SourceFact>,
    web_surfaces: Vec<SourceFact>,
    mobile_surfaces: Vec<SourceFact>,
    web_routes: Vec<SourceFact>,
    mobile_routes: Vec<SourceFact>,
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
struct ExternalCallFact {
    path: PathBuf,
    line: usize,
    column: usize,
    feature: String,
    subject_kind: String,
    subject: String,
    slot: String,
    operation: String,
    has_timeout: bool,
    has_retry: bool,
    has_idempotency: bool,
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

fn collect_package_paths(input: &Path) -> Result<Vec<PathBuf>> {
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

fn project_uses_plugin_refs(project_root: &Path) -> bool {
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
fn dedupe_env_contract_diagnostics(diagnostics: &[DoctorDiagnostic]) -> Vec<DoctorDiagnostic> {
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
fn suppress_env_schema_when_declared(
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

fn manifest_required_diagnostics(
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

fn lazurite_manifest_diagnostics(package: &DoctorPackage) -> Vec<DoctorDiagnostic> {
    if !project_has_lazurite_manifest(&package.project_root) {
        return Vec::new();
    }

    let Some(manifest) = package.lazurite_manifest.as_ref() else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    diagnostics.extend(check_plugin_not_declared(manifest, package));
    diagnostics.extend(check_plugin_unused(manifest, package));
    diagnostics.extend(check_plugin_namespace_mismatch(manifest, package));
    diagnostics.extend(check_semantic_plugin_unresolved(manifest, package));
    diagnostics.extend(check_semantic_plugin_no_validator(manifest, package));
    diagnostics.extend(check_plugin_manifest_missing(manifest, package));
    diagnostics.extend(check_plugin_manifest_schema_legacy(manifest, package));
    diagnostics.extend(check_plugin_readme_missing(manifest, package));
    diagnostics.extend(check_plugin_catalog_drift(manifest, package));
    diagnostics.extend(check_submodule_drift(manifest, package));
    diagnostics.extend(check_migration_strategy_conflict(manifest, package));
    diagnostics.extend(check_frontend_audience_unknown(manifest, package));
    diagnostics.extend(check_audience_no_frontend(manifest, package));
    diagnostics.extend(check_frontend_out_collision(manifest, package));
    // Wave 0.5 — `DOCTOR-OVERRIDE-NEEDS-REASON-001`. Fires when any
    // `[doctor.<category>].severity_override.<RULE-CODE>` entry lacks a
    // non-blank `reason` justification.
    diagnostics.extend(check_doctor_override_needs_reason(manifest, package));
    // Frente 1 — `COVERAGE-PRESET-UNKNOWN-001`. Fires when
    // `[doctor.coverage] preset = "<name>"` names a preset that does
    // not exist in `CoveragePreset::parse`. Surfacing this as an error
    // avoids silent "vacuous pass" behavior on a typo.
    diagnostics.extend(check_coverage_preset_unknown(manifest, package));
    // Frente 1 — `CONFIG-NOISE-001`. Warning when a config file's
    // comment ratio is dominated by commentary (more comment lines than
    // semantic lines). Anchors at `Lazurite.toml` and `Lazuli.toml`.
    diagnostics.extend(check_config_noise(package));
    diagnostics
}

/// Wave 0.5 — `DOCTOR-OVERRIDE-NEEDS-REASON-001` dispatcher.
///
/// Lifts the `[doctor.test_discipline].severity_override` table from
/// the parsed manifest into the rule's portable `OverrideEntry` shape,
/// invokes the rule, and maps findings to `DoctorDiagnostic`. Anchors
/// the diagnostic at `Lazurite.toml` line 1 (the rule's structural
/// payload doesn't yet carry exact TOML line spans; that refinement
/// lands post-Wave-0.5 once the toml crate exposes spans cleanly).
fn check_doctor_override_needs_reason(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    use crate::lazurite_manifest as lzr;
    use lazuli_doctor::test_discipline::override_needs_reason_001::{self, OverrideEntry};

    let Some(doctor) = manifest.doctor.as_ref() else {
        return Vec::new();
    };
    let mut entries: Vec<OverrideEntry> = Vec::new();
    if let Some(td) = doctor.test_discipline.as_ref() {
        for (code, ov) in td.severity_override.iter() {
            let _: &lzr::SeverityOverride = ov; // keep the type pinned
            entries.push(OverrideEntry {
                category: RuleCategory::TestDiscipline.as_str().to_owned(),
                rule_code: code.clone(),
                severity: ov.severity.clone(),
                reason: ov.reason.clone(),
            });
        }
    }

    let manifest_path = package.project_root.join(lzr::MANIFEST_FILENAME);
    let findings = override_needs_reason_001::check(&entries, &manifest_path);
    findings
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            let severity = doctor_severity_for(
                override_needs_reason_001::Finding::CODE,
                RuleCategory::TestDiscipline,
                package.security_profile,
                &std::collections::BTreeMap::new(),
            );
            DoctorDiagnostic {
                path: finding.path,
                line: 1,
                column: 1,
                severity,
                code: override_needs_reason_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::TestDiscipline),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// Frente 1 — `COVERAGE-PRESET-UNKNOWN-001`. Fires when
/// `[doctor.coverage] preset = "<name>"` names a preset that
/// `CoveragePreset::parse` does not recognize. Listing the recognized
/// preset names in the message keeps the diagnostic self-explanatory
/// to an LLM authoring `Lazurite.toml` cold.
fn check_coverage_preset_unknown(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::coverage::CoveragePreset;

    let Some(preset_name) = manifest
        .doctor
        .as_ref()
        .and_then(|d| d.coverage.as_ref())
        .and_then(|c| c.preset.as_deref())
    else {
        return Vec::new();
    };
    if CoveragePreset::parse(preset_name).is_some() {
        return Vec::new();
    }
    vec![DoctorDiagnostic {
        path: package
            .project_root
            .join(crate::lazurite_manifest::MANIFEST_FILENAME),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "COVERAGE-PRESET-UNKNOWN-001".to_owned(),
        message: format!(
            "[doctor.coverage] preset = \"{preset_name}\" is not a recognized preset. \
             Allowed values: tdd-strict, tdd-mature, off. \
             Omit the field to fall back to the security-profile defaults."
        ),
        category: Some(RuleCategory::Vocabulary),
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

/// Frente 1 — `CONFIG-NOISE-001`. Warns when the comment ratio of a
/// top-level config file exceeds 1:1 (more comment lines than semantic
/// lines). The signal: when the user's config file is mostly inline
/// commentary, the framework is hiding intent behind explanation. The
/// fix: push the explanation into framework defaults / canonical docs.
///
/// Severity is Warning by design — the rule is informational and
/// never gates. Scope: `Lazurite.toml` (and the legacy lowercase
/// `lazurite.toml`). `.lzi` / `.lzx` follow in a future cycle once
/// the comment-vs-statement counter understands Lazuli syntax.
///
/// Heuristic logic + 6 unit tests live in
/// `lazuli_doctor::config_noise`; this function only stitches the
/// metrics to a `DoctorDiagnostic`.
fn check_config_noise(package: &DoctorPackage) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::config_noise::config_noise_metrics;
    let mut diagnostics = Vec::new();
    // Prefer the canonical capitalized name; fall back to legacy only
    // if canonical is absent (mirrors `lazurite_manifest::load`). On
    // case-insensitive filesystems both `exists()` calls would
    // otherwise report the same file twice and double-fire.
    let canonical = package
        .project_root
        .join(crate::lazurite_manifest::MANIFEST_FILENAME);
    let legacy = package
        .project_root
        .join(crate::lazurite_manifest::LEGACY_MANIFEST_FILENAME);
    let (path, filename) = if canonical.exists() {
        (canonical, crate::lazurite_manifest::MANIFEST_FILENAME)
    } else if legacy.exists() {
        (legacy, crate::lazurite_manifest::LEGACY_MANIFEST_FILENAME)
    } else {
        return diagnostics;
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return diagnostics;
    };
    let metrics = config_noise_metrics(&contents);
    if metrics.fires() {
        diagnostics.push(DoctorDiagnostic {
            path,
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "CONFIG-NOISE-001".to_owned(),
            message: format!(
                "{filename} has {} comment line(s) vs {} semantic line(s) (ratio {:.2}:1). \
                 When commentary exceeds config, the framework is leaking \
                 intent into the user's file — push the knowledge into framework \
                 defaults or canonical docs. See docs/canonical-semantics.md#config-hygiene.",
                metrics.comment_lines, metrics.semantic_lines, metrics.ratio()
            ),
            category: Some(RuleCategory::Vocabulary),
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    diagnostics
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
fn cap_file_policy_implicit_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
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
fn manual_param_coercion_diagnostics(project_root: &Path) -> Vec<DoctorDiagnostic> {
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
fn import_deprecated_alias_diagnostics(project_root: &Path) -> Vec<DoctorDiagnostic> {
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
fn schema_rich_gap_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
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

/// PLUGIN-MANIFEST-MISSING (error) — every plugin declared in
/// `Lazurite.toml [plugins]` with a resolvable local path must ship a
/// `manifest.toml` at its root. Today the framework silently skips
/// plugins without a manifest (the alias-builder pass returns
/// `Ok(None)`); doctor escalates that to an error so the plugin
/// catalog stays self-describing.
///
/// Remote plugins without a `dev.plugin_paths` override skip the
/// check (the manifest isn't on the local filesystem at all — a
/// different diagnostic class).
fn check_plugin_manifest_missing(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = crate::plugin_manifest::resolve_plugin_root(
            manifest,
            &package.project_root,
            plugin_ref,
        ) else {
            continue;
        };
        let manifest_path = plugin_root.join(crate::plugin_manifest::PLUGIN_MANIFEST_FILENAME);
        if manifest_path.exists() {
            continue;
        }
        diagnostics.push(DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "PLUGIN-MANIFEST-MISSING".to_owned(),
            message: format!(
                "plugin `{plugin_ref}` at `{}` is missing `manifest.toml`. Every plugin must declare a `[plugin]` block (name + namespace + go_module + ts_package) so the catalog stays self-describing. Add `manifest.toml` to the plugin root or remove the plugin from Lazurite.toml [plugins].",
                plugin_root.display(),
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

/// PLUGIN-MANIFEST-SCHEMA-LEGACY (error) — every plugin
/// `manifest.toml` MUST declare a `[plugin]` block carrying
/// `name`/`namespace`/`go_module` (per
/// `crate::plugin_manifest::PluginManifest`). Some older plugins
/// (pre-2026-05-12, before the lazuli-ops 85ff076 cutover) used a
/// flat top-level `name`/`version`/`implements` shape that the
/// loader accepts silently — codegen falls back to v1 conventions
/// and the LSP catalog shows the plugin under its DSL ref, but
/// every downstream feature is degraded.
///
/// Wave §A4 hard-cutover (2026-05-23): all 10 known legacy plugins
/// have been migrated; any remaining legacy manifest is a bug, so
/// this lint runs at Error severity from day one.
fn check_plugin_manifest_schema_legacy(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = crate::plugin_manifest::resolve_plugin_root(
            manifest,
            &package.project_root,
            plugin_ref,
        ) else {
            continue;
        };
        let manifest_path = plugin_root.join(crate::plugin_manifest::PLUGIN_MANIFEST_FILENAME);
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            // PLUGIN-MANIFEST-MISSING already covers absence.
            continue;
        };
        // Parse as raw TOML so we can inspect for a `[plugin]` table.
        // The PluginManifest deserializer treats `plugin` as optional,
        // so a `PluginManifest::default()` round-trips a legacy
        // manifest cleanly — that's exactly the silent path we close
        // here.
        let Ok(value) = text.parse::<toml::Value>() else {
            // TOML syntax error is a separate concern; let the loader
            // surface it through its own error path.
            continue;
        };
        let table = match value.as_table() {
            Some(t) => t,
            None => continue,
        };
        if table.contains_key("plugin") {
            continue; // canonical v1 shape — no diagnostic
        }
        diagnostics.push(DoctorDiagnostic {
            path: manifest_path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "PLUGIN-MANIFEST-SCHEMA-LEGACY".to_owned(),
            message: format!(
                "plugin `{plugin_ref}` at `{}` uses the legacy flat manifest schema (top-level `name`/`version`/`implements`). Migrate to the v1 schema with a `[plugin]` block declaring `namespace`, `name`, `go_module`, and `ts_package` (optional). See `docs/proposals/plugin-manifest-v1-hard-cutover-2026-05-23.md` (wave §A4).",
                manifest_path.display(),
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

/// PLUGIN-README-MISSING (warning) — every plugin with a resolvable
/// local path should ship a `README.md`. Authors of new pilots (and
/// new plugin contributors) rely on the README to understand the
/// surface; missing READMEs silently degrade the catalog quality.
fn check_plugin_readme_missing(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = crate::plugin_manifest::resolve_plugin_root(
            manifest,
            &package.project_root,
            plugin_ref,
        ) else {
            continue;
        };
        // Skip when the manifest itself is missing — the manifest lint
        // anchors that failure mode, no need to double-flag.
        let manifest_path = plugin_root.join(crate::plugin_manifest::PLUGIN_MANIFEST_FILENAME);
        if !manifest_path.exists() {
            continue;
        }
        let readme_path = plugin_root.join("README.md");
        if readme_path.exists() {
            continue;
        }
        diagnostics.push(DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "PLUGIN-README-MISSING".to_owned(),
            message: format!(
                "plugin `{plugin_ref}` at `{}` is missing `README.md`. Plugins should ship a README documenting their surface (Go fns, TS fns, manifest scalars). The catalog page derives from it.",
                plugin_root.display(),
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

/// PLUGIN-CATALOG-DRIFT (warning) — `dist/plugin-catalog.json` is
/// expected to be regenerated whenever a plugin's manifest or README
/// changes. When the catalog's mtime predates any plugin's
/// `manifest.toml` or `README.md`, the catalog is stale and the LSP /
/// docs site / `lazuli plugins` CLI will show outdated info.
///
/// Quietly skips when the catalog file doesn't exist yet (the next
/// `lazuli generate ts` will produce it) and when no plugins are
/// declared. Spec: `docs/proposals/plugin-catalog-file-2026-05-23.md`.
fn check_plugin_catalog_drift(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    if manifest.plugins.is_empty() {
        return Vec::new();
    }
    let catalog_path = package
        .project_root
        .join("dist")
        .join("plugin-catalog.json");
    let Ok(catalog_meta) = std::fs::metadata(&catalog_path) else {
        return Vec::new();
    };
    let Ok(catalog_mtime) = catalog_meta.modified() else {
        return Vec::new();
    };

    let mut stale_sources: Vec<String> = Vec::new();
    for plugin_ref in manifest.plugins.keys() {
        let Some(plugin_root) = crate::plugin_manifest::resolve_plugin_root(
            manifest,
            &package.project_root,
            plugin_ref,
        ) else {
            continue;
        };
        for relpath in [
            crate::plugin_manifest::PLUGIN_MANIFEST_FILENAME,
            "README.md",
        ] {
            let p = plugin_root.join(relpath);
            let Ok(meta) = std::fs::metadata(&p) else {
                continue;
            };
            let Ok(mtime) = meta.modified() else { continue };
            if mtime > catalog_mtime {
                stale_sources.push(format!("{plugin_ref} ({relpath})"));
                break;
            }
        }
    }

    if stale_sources.is_empty() {
        return Vec::new();
    }
    stale_sources.sort();

    vec![DoctorDiagnostic {
        path: catalog_path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "PLUGIN-CATALOG-DRIFT".to_owned(),
        message: format!(
            "`dist/plugin-catalog.json` is older than {} plugin source(s) ({}). Run `lazuli generate ts` to refresh the catalog so the LSP / docs site / `lazuli plugins` CLI see current plugin info.",
            stale_sources.len(),
            stale_sources.join(", "),
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

/// SEMANTIC-PLUGIN-002 (B4) — `@semantic.<Name>` references that
/// resolve to a plugin scalar with NO `validator` declared. The type
/// alias exists but no runtime check enforces it; the field accepts
/// any string at the wire boundary. Warn-level: some plugins ship
/// brand aliases intentionally without validation.
///
/// Source-of-truth: `docs/proposals/ir-semantic-auto-validate-2026-05-22.md`
/// (W2 §"Doctor B4").
fn check_semantic_plugin_no_validator(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let alias_map =
        match crate::plugin_manifest::build_alias_map(Some(manifest), &package.project_root) {
            Ok(map) => map,
            Err(_) => return Vec::new(), // SEMANTIC-PLUGIN-001 already covers this
        };
    let mut diagnostics = Vec::new();
    for file in &package.files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        for reference in collect_at_references_in_source(&file.path, &file.source) {
            let Some(rest) = reference.reference.strip_prefix("@semantic.") else {
                continue;
            };
            let head = rest.split('(').next().unwrap_or(rest);
            let alias = format!("@semantic.{}", head);
            let Some(resolved) = alias_map.get(&alias) else {
                continue;
            };
            if !resolved.validator.is_empty() {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                path: reference.path.clone(),
                line: reference.line,
                column: reference.column,
                severity: DoctorSeverity::Warning,
                code: "SEMANTIC-PLUGIN-002".to_owned(),
                message: format!(
                    "plugin semantic type `{alias}` from `{}` does not declare a `validator` in its manifest. The type alias is accepted, but no runtime check enforces the value — invalid input is silently stored. Add a `validator` to the plugin's `[[semantic_types]]` entry, or annotate the field with `@validate.skip` to acknowledge the bypass.",
                    resolved.plugin_namespace
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

/// SEMANTIC-PLUGIN-001 — `@semantic.<Name>` references in `.lzi` files
/// that resolve neither against the built-in closed catalog nor any
/// plugin's `manifest.toml`. Per
/// `docs/proposals/semantic-types-plugin-locales.md` §New diagnostics.
///
/// Three failure modes share the diagnostic code:
/// 1. Plugin not declared in `Lazurite.toml [plugins]` (the source of
///    truth for alias activation).
/// 2. Plugin manifest missing or malformed.
/// 3. Two or more active plugins declare the same alias (conflict).
///
/// The shared error code is intentional — every failure has the same
/// resolution path (declare the right plugin, fix the manifest).
fn check_semantic_plugin_unresolved(
    manifest: &crate::lazurite_manifest::Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    // Build the alias map. Map-construction errors (conflict, mismatch,
    // unsupported carrier) surface as SEMANTIC-PLUGIN-001 anchored at
    // the project root because they're project-wide.
    let alias_map = match crate::plugin_manifest::build_alias_map(
        Some(manifest),
        &package.project_root,
    ) {
        Ok(map) => map,
        Err(err) => {
            return vec![DoctorDiagnostic {
                path: package.project_root.join("Lazurite.toml"),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "SEMANTIC-PLUGIN-001".to_owned(),
                message: format!(
                    "plugin semantic alias map: {}. Fix the affected plugin manifest under [plugins] in Lazurite.toml.",
                    err
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }];
        }
    };

    // Closed catalog of built-in `@semantic.<X>` names; matches the
    // analyzer's `type_ref_from_syntax` match arm. Authors writing one
    // of these never hit the plugin path.
    const BUILT_IN_SEMANTIC: &[&str] = &[
        "Email", "Phone", "Url", "Uuid", "Currency", "GeoPoint", "Money",
    ];

    let mut diagnostics = Vec::new();
    for file in &package.files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        // Walk every `@semantic.<Name>` reference. The shared
        // `collect_at_references_in_source` picks up the full set of
        // `@namespace.name` references; we filter to `semantic` here.
        for reference in collect_at_references_in_source(&file.path, &file.source) {
            // `reference.reference` is the raw `@semantic.<Name>` text.
            let Some(rest) = reference.reference.strip_prefix("@semantic.") else {
                continue;
            };
            // Built-ins resolve syntactically — never SEMANTIC-PLUGIN-001.
            if BUILT_IN_SEMANTIC.contains(&rest) {
                continue;
            }
            // `@semantic.Money(currency:USD)` lifts via parens — pick
            // the head token before `(` so we don't false-flag a typed
            // money reference.
            let head = rest.split('(').next().unwrap_or(rest);
            if BUILT_IN_SEMANTIC.contains(&head) {
                continue;
            }
            // Strip any trailing non-name punctuation (whitespace lifts
            // already drop everything after the first non-ident char,
            // but defensive normalisation here helps when the reference
            // came from a typed-block line ending in `@validator.x`).
            let alias = format!("@semantic.{}", head);
            if alias_map.contains_key(&alias) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                path: reference.path.clone(),
                line: reference.line,
                column: reference.column,
                severity: DoctorSeverity::Error,
                code: "SEMANTIC-PLUGIN-001".to_owned(),
                message: format!(
                    "unknown plugin semantic type `{alias}`. No plugin in Lazurite.toml [plugins] declares this alias. Add the appropriate `@lazuli/plugin-<name>` to [plugins] or replace the field with a built-in `@semantic.*` type.",
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

fn lazuli_version_001_diagnostics(
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

fn lazuli_version_002_diagnostics(
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




fn check_plugin_not_declared(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let declared: BTreeSet<&str> = manifest.plugins.keys().map(|key| key.as_str()).collect();
    collect_package_plugin_references(package)
        .into_iter()
        .filter(|reference| !declared.contains(reference.reference.as_str()))
        .map(|reference| DoctorDiagnostic {
            path: reference.path,
            line: reference.line,
            column: reference.column,
            severity: DoctorSeverity::Error,
            code: "PLUGIN-NOT-DECLARED-001".to_owned(),
            message: format!(
                "`.lzi` references `{}`, but Lazurite.toml does not declare it in `[plugins]`.",
                reference.reference
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn check_plugin_unused(manifest: &Manifest, package: &DoctorPackage) -> Vec<DoctorDiagnostic> {
    let used: BTreeSet<String> = collect_package_plugin_references(package)
        .into_iter()
        .map(|reference| reference.reference)
        .collect();

    manifest
        .plugins
        .keys()
        .filter(|plugin_ref| !used.contains(*plugin_ref))
        .map(|plugin_ref| DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "PLUGIN-UNUSED-001".to_owned(),
            message: format!(
                "Lazurite.toml declares `{plugin_ref}`, but no `.lzi` file references it."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn check_plugin_namespace_mismatch(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let declared_short: BTreeSet<String> = manifest
        .plugins
        .keys()
        .filter_map(|key| {
            key.strip_prefix("@lazuli/plugin-")
                .map(|name| name.to_owned())
        })
        .collect();

    for key in manifest.plugins.keys() {
        if !key.starts_with("@lazuli/plugin-") {
            diagnostics.push(DoctorDiagnostic {
                path: package.project_root.join("Lazurite.toml"),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                message: format!(
                    "manifest plugin key `{key}` does not use the canonical plugin namespace; plugins must be declared as `@lazuli/plugin-<name>`."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for file in &package.files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        for reference in collect_at_references_in_source(&file.path, &file.source) {
            // A canonical plugin reference is `@lazuli/plugin-<name>` —
            // the @-reference parser yields namespace=`lazuli`,
            // name=`plugin-<name>` for that shape. Skip it before the
            // mismatch detector runs.
            if reference.namespace == "lazuli" && reference.name.starts_with("plugin-") {
                continue;
            }
            if reference.namespace == "adapter" && declared_short.contains(&reference.name) {
                diagnostics.push(DoctorDiagnostic {
                    path: reference.path,
                    line: reference.line,
                    column: reference.column,
                    severity: DoctorSeverity::Error,
                    code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                    message: format!(
                        "`{}` uses the local adapter namespace, but Lazurite.toml declares `@lazuli/plugin-{}`; use the plugin reference.",
                        reference.reference, reference.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            } else if !is_allowed_reference_namespace_for_doctor(&reference.namespace)
                && declared_short.contains(&reference.name)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: reference.path,
                    line: reference.line,
                    column: reference.column,
                    severity: DoctorSeverity::Error,
                    code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                    message: format!(
                        "`{}` uses unknown namespace `@{}`, but Lazurite.toml declares `@lazuli/plugin-{}`; use the plugin reference.",
                        reference.reference, reference.namespace, reference.name
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

fn check_submodule_drift(manifest: &Manifest, package: &DoctorPackage) -> Vec<DoctorDiagnostic> {
    if !manifest
        .generate
        .go
        .as_ref()
        .map(|go| go.submodule)
        .unwrap_or(false)
    {
        return Vec::new();
    }

    let root_go_mod = package.project_root.join("go.mod");
    let dist_go_mod = package.project_root.join("dist/go/go.mod");
    if !dist_go_mod.is_file() {
        return Vec::new();
    }

    let Ok(root_source) = fs::read_to_string(&root_go_mod) else {
        return Vec::new();
    };
    let Ok(dist_source) = fs::read_to_string(&dist_go_mod) else {
        return Vec::new();
    };
    let Some(root_version) = go_mod_lazuli_runtime_version(&root_source) else {
        return Vec::new();
    };
    let Some(dist_version) = go_mod_lazuli_runtime_version(&dist_source) else {
        return Vec::new();
    };

    if root_version == dist_version {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: dist_go_mod,
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "SUBMODULE-DRIFT-001".to_owned(),
        message: format!(
            "`dist/go/go.mod` requires lazuli.dev/runtime {dist_version}, but root go.mod requires {root_version}."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

fn check_migration_strategy_conflict(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    if !matches!(
        manifest
            .migrations
            .as_ref()
            .map(|migrations| &migrations.strategy),
        Some(MigrationStrategy::Manual)
    ) {
        return Vec::new();
    }

    let Some(app) = package.app.as_ref() else {
        return Vec::new();
    };
    if app
        .manifest
        .deploy
        .as_ref()
        .and_then(|deploy| deploy.migrations.as_deref())
        != Some("before_deploy")
    {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: app.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "MIGRATION-STRATEGY-CONFLICT-001".to_owned(),
        message: "`[migrations].strategy = \"manual\"` conflicts with `app.lzi deploy migrations before_deploy`."
            .to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

fn check_frontend_audience_unknown(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let known = collect_known_audiences(&package.files);
    let mut diagnostics = Vec::new();
    for (frontend_name, frontend) in &manifest.frontends {
        for audience in &frontend.audiences {
            if !known.contains(audience) {
                diagnostics.push(DoctorDiagnostic {
                    path: package.project_root.join("Lazurite.toml"),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "FRONTEND-AUDIENCE-UNKNOWN-001".to_owned(),
                    message: format!(
                        "`[frontends.{frontend_name}]` ships audience `{audience}`, but no `.lzx` surface declares it."
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

fn check_audience_no_frontend(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let shipped: BTreeSet<&str> = manifest
        .frontends
        .values()
        .flat_map(|frontend| frontend.audiences.iter().map(|audience| audience.as_str()))
        .collect();

    collect_known_audiences(&package.files)
        .into_iter()
        .filter(|audience| !shipped.contains(audience.as_str()))
        .map(|audience| DoctorDiagnostic {
            path: package.project_root.join("Lazurite.toml"),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "AUDIENCE-NO-FRONTEND-001".to_owned(),
            message: format!(
                "`.lzx` declares audience `{audience}`, but no `[frontends.*]` block ships it."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn check_frontend_out_collision(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    let mut first_by_out: BTreeMap<&str, &str> = BTreeMap::new();
    let mut diagnostics = Vec::new();
    for (name, frontend) in &manifest.frontends {
        if let Some(first) = first_by_out.insert(frontend.out.as_str(), name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: package.project_root.join("Lazurite.toml"),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "FRONTEND-OUT-COLLISION-001".to_owned(),
                message: format!(
                    "`[frontends.{name}]` and `[frontends.{first}]` both declare output path `{}`.",
                    frontend.out
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


/// PG.B — collect `(callable_key, body_text, span)` tuples for
/// every callable in every `.lzi` source so
/// `diagnose_plan_gate_facts` can run the `GATE-EVAL-ORDER-001`
/// per-body scan. Body text is the indented region under the callable
/// header.
fn collect_callable_bodies_for_eval_order(
    files: &[DoctorFile],
) -> Vec<(String, String, lazuli_syntax::Span)> {
    let mut out = Vec::new();
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let feature = derive_feature_name(&file.source).unwrap_or_else(|| {
            file.path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_owned()
        });
        // Walk source lines tracking callable header + indent.
        let mut current: Option<(String, usize, usize, String)> = None; // (key, header_indent, body_offset_start, body)
        let mut offset = 0usize;
        for line in file.source.lines() {
            let line_len = line.len();
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if let Some((_, header_indent, _, _)) = &current {
                if !trimmed.is_empty() && indent <= *header_indent {
                    // Close current.
                    if let Some((key, _, body_start, body)) = current.take() {
                        out.push((
                            format!("{}/{}", feature, key),
                            body,
                            lazuli_syntax::Span::new(body_start, offset),
                        ));
                    }
                }
            }
            if let Some(key) = callable_header_key_from_trimmed(trimmed) {
                if indent == 2 {
                    current = Some((key, indent, offset + line_len + 1, String::new()));
                    offset += line_len + 1;
                    continue;
                }
            }
            if let Some((_, _, _, body)) = &mut current {
                body.push_str(line);
                body.push('\n');
            }
            offset += line_len + 1;
        }
        if let Some((key, _, body_start, body)) = current.take() {
            out.push((
                format!("{}/{}", feature, key),
                body,
                lazuli_syntax::Span::new(body_start, offset),
            ));
        }
    }
    out
}

fn callable_header_key_from_trimmed(trimmed: &str) -> Option<String> {
    let prefixes: &[(&str, &str)] = &[
        ("command ", "command"),
        ("job ", "job"),
        ("webhook ", "webhook"),
        ("api ", "api"),
        ("query.list ", "query.list"),
        ("query.lookup ", "query.lookup"),
        ("query.sql ", "query.sql"),
        ("query.view ", "query.view"),
    ];
    for (prefix, kind) in prefixes {
        if let Some(rest) = trimmed.strip_prefix(*prefix) {
            let name = rest.split_whitespace().next().unwrap_or_default();
            if !name.is_empty() {
                return Some(format!("{}:{}", kind, name));
            }
        }
    }
    None
}

fn collect_canonical_facts(file: &DoctorFile, operational: &mut OperationalFacts) {
    let lines: Vec<_> = file.source.lines().collect();
    collect_operational_lzi_facts(file, &lines, operational);
    collect_file_capability_facts(file, &lines, operational);

    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if leading_spaces(lines[index]) != 0 || !trimmed.starts_with("feature ") {
            index += 1;
            continue;
        }

        let feature = trimmed
            .split_whitespace()
            .nth(1)
            .unwrap_or("<anonymous>")
            .to_owned();
        let start = index;
        index += 1;
        while index < lines.len()
            && !(leading_spaces(lines[index]) == 0
                && lines[index].trim_start().starts_with("feature "))
        {
            index += 1;
        }

        collect_feature_integration_requirements(
            file,
            &feature,
            start,
            &lines[start..index],
            operational,
        );
        // Phase L Tier 4 follow-up — the `job` branch of
        // `collect_external_calls_in_block` is retired alongside the
        // legacy `command` branch. Jobs now flow through
        // `populate_job_external_calls_from_ir` which reads
        // `Job.external_calls[*].span_ref` plus the typed
        // `Job.timeout` / `Job.retry` / `Job.idempotency` axes.
    }
}

fn collect_feature_integration_requirements(
    file: &DoctorFile,
    feature: &str,
    feature_start: usize,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    let mut in_requires_block = false;

    for (offset, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 2 {
            in_requires_block = trimmed == "requires";
            if let Some(requirement) = trimmed.strip_prefix("requires ") {
                if let Some((slot, contract)) = parse_integration_requirement(requirement) {
                    operational
                        .integration_requirements
                        .push(IntegrationRequirementFact {
                            path: file.path.clone(),
                            line: feature_start + offset + 1,
                            column: leading + 1,
                            feature: feature.to_owned(),
                            slot: slot.to_owned(),
                            contract: contract.to_owned(),
                        });
                }
            }
            continue;
        }

        if leading <= 2 {
            in_requires_block = false;
        }

        if in_requires_block
            && leading == 4
            && let Some((slot, contract)) = parse_integration_requirement(trimmed)
        {
            operational
                .integration_requirements
                .push(IntegrationRequirementFact {
                    path: file.path.clone(),
                    line: feature_start + offset + 1,
                    column: leading + 1,
                    feature: feature.to_owned(),
                    slot: slot.to_owned(),
                    contract: contract.to_owned(),
                });
        }
    }
}

/// Phase L Tier 4 follow-up — IR-driven replacement for the retired
/// `job` branch of `collect_external_calls_in_block`. Walks each
/// `job.external_calls` entry, reads `ExternalCallRef.span_ref` to
/// anchor the diagnostic at the `calls <slot>.<op>` line, and emits an
/// `ExternalCallFact` carrying the typed `has_timeout` / `has_retry` /
/// `has_idempotency` axes lifted from the `Job` IR. The job branch of
/// the legacy text walker is gone.
fn populate_job_external_calls_from_ir(
    file: &DoctorFile,
    feature: &lazuli_ir::Feature,
    operational: &mut OperationalFacts,
) {
    if feature.jobs.is_empty() {
        return;
    }
    let job_lines = collect_construct_lines(
        &file.source,
        "job ",
        feature.jobs.iter().map(|j| j.name.as_str()).collect(),
    );
    for job in &feature.jobs {
        if job.external_calls.is_empty() {
            continue;
        }
        let header_line = job_lines.get(&job.name).copied().unwrap_or(1);
        let has_timeout = job.timeout.is_some();
        let has_retry = job.retry.is_some();
        let has_idempotency = job.idempotency.is_some();
        let subject = format!("{}.job.{}", feature.name, job.name);
        for call in &job.external_calls {
            let (call_line, call_column) = match call.span_ref.as_ref() {
                Some(span) => {
                    let (line, col) = line_col_for_offset(&file.source, span.start);
                    (line, col)
                }
                None => (header_line, 1),
            };
            operational.external_calls.push(ExternalCallFact {
                path: file.path.clone(),
                line: call_line,
                column: call_column,
                feature: feature.name.clone(),
                subject_kind: "job".to_owned(),
                subject: subject.clone(),
                slot: call.slot.clone(),
                operation: call.op.clone(),
                has_timeout,
                has_retry,
                has_idempotency,
            });
        }
    }
}

/// Phase L Tier 4 follow-up — IR-driven replacement for the retired
/// `command` branch of `collect_external_calls_in_block`. Walks each
/// `command.external_calls` entry, finds its `calls <slot>.<op>` line
/// in the source, and emits an `ExternalCallFact` carrying the typed
/// `has_timeout` / `has_retry` / `has_idempotency` axes lifted from the
/// `Command` IR. The line lookup is keyed on the verbatim `calls
/// <slot>.<op>` substring inside the command body's source range, so
/// the diagnostic anchors stay precise.
fn populate_command_external_calls_from_ir(
    file: &DoctorFile,
    feature: &lazuli_ir::Feature,
    operational: &mut OperationalFacts,
) {
    if feature.commands.is_empty() {
        return;
    }
    let command_lines = collect_construct_lines(
        &file.source,
        "command ",
        feature.commands.iter().map(|c| c.name.as_str()).collect(),
    );
    let source_lines: Vec<&str> = file.source.lines().collect();
    for command in &feature.commands {
        if command.external_calls.is_empty() {
            continue;
        }
        let header_line = command_lines
            .get(&command.name)
            .copied()
            .unwrap_or(1)
            .saturating_sub(1);
        // Block ends at the next top-level construct (indent <= 2).
        let mut block_end = header_line + 1;
        while block_end < source_lines.len() && leading_spaces(source_lines[block_end]) > 2 {
            block_end += 1;
        }
        let has_timeout = command.timeout.is_some();
        let has_retry = command.retry.is_some();
        let has_idempotency = command.idempotency.is_some();
        let subject = format!("{}.command.{}", feature.name, command.name);
        for call in &command.external_calls {
            let needle = format!("calls {}.{}", call.slot, call.op);
            let mut call_line = header_line + 1; // fall back to header
            let mut call_column = 1;
            for i in (header_line + 1)..block_end {
                if source_lines[i].trim_start().starts_with(needle.as_str()) {
                    call_line = i + 1;
                    call_column = leading_spaces(source_lines[i]) + 1;
                    break;
                }
            }
            operational.external_calls.push(ExternalCallFact {
                path: file.path.clone(),
                line: call_line,
                column: call_column,
                feature: feature.name.clone(),
                subject_kind: "command".to_owned(),
                subject: subject.clone(),
                slot: call.slot.clone(),
                operation: call.op.clone(),
                has_timeout,
                has_retry,
                has_idempotency,
            });
        }
    }
}

fn collect_operational_lzi_facts(
    file: &DoctorFile,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let line_number = index + 1;
        let column = leading_spaces(line) + 1;

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some(feature) = trimmed.split_whitespace().nth(1) {
                operational.features.insert(
                    feature.to_owned(),
                    SourceFact {
                        path: file.path.clone(),
                        line: line_number,
                        column,
                        name: feature.to_owned(),
                    },
                );
            }
        }

        for reference in path_references(trimmed, "env.") {
            operational.env_references.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: reference.to_owned(),
            });
        }

        if trimmed.contains("@cap.File") {
            operational.file_capabilities.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: "@cap.File".to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("api ") {
            operational.apis.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: named_block_name(trimmed, "api")
                    .unwrap_or("unknown")
                    .to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("webhook ") {
            operational.webhooks.push(SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: named_block_name(trimmed, "webhook")
                    .unwrap_or("unknown")
                    .to_owned(),
            });
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("job ") {
            let job_name = named_block_name(trimmed, "job").unwrap_or("unknown");
            let fact = SourceFact {
                path: file.path.clone(),
                line: line_number,
                column,
                name: job_name.to_owned(),
            };
            if job_block_has_schedule(lines, index) {
                operational.schedules.push(fact.clone());
            }
            operational.jobs.push(fact);
        }
    }
}

fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?.trim_start();
    rest.split_whitespace().next()
}

fn job_block_has_schedule(lines: &[&str], start: usize) -> bool {
    let mut index = start + 1;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if !trimmed.is_empty() && leading_spaces(line) <= 2 {
            break;
        }
        if leading_spaces(line) == 4 && trimmed.starts_with("trigger schedule ") {
            return true;
        }
        index += 1;
    }
    false
}

/// Phase L Tier 4 follow-up — IR-driven replacement for the retired
/// `collect_feature_commands` text-walker. Reads `feature.commands`
/// (typed) and populates the `commands: BTreeMap<CommandKey, CommandPolicy>`
/// map that `policy_reachability_diagnostics` +
/// `command_route_binding_diagnostics` consume.
///
/// Commands without a `policy` clause are skipped (mirroring the old
/// walker, which only inserted entries when it saw `policy ...`).
///
/// `@policy.<name>` atoms expand against the typed
/// `feature.policies.categories` slot, populated by
/// `lower_feature_skeleton` from the canonical-indent `policies` block.
/// The retired `collect_policy_atoms` text-walker that previously
/// resolved these names is gone. Route slot binding-from-context reads
/// typed `RouteSlot.from`.
fn populate_commands_from_ir(
    feature: &lazuli_ir::Feature,
    commands: &mut BTreeMap<CommandKey, CommandPolicy>,
) {
    use lazuli_ir::PolicyRef;

    let local_policies: BTreeMap<&str, &Vec<String>> = feature
        .policies
        .categories
        .iter()
        .map(|c| (c.name.as_str(), &c.atoms))
        .collect();

    for command in &feature.commands {
        let (reference, atoms) = match &command.policy {
            PolicyRef::None | PolicyRef::Unresolved(_) => continue,
            PolicyRef::Atom(atom) => {
                let reference = format!("@{atom}");
                let atoms = if let Some(local) = atom.strip_prefix("policy.") {
                    local_policies
                        .get(local)
                        .map(|atoms| (*atoms).clone())
                        .unwrap_or_default()
                } else {
                    vec![reference.clone()]
                };
                (reference, atoms)
            }
            PolicyRef::Local(name) => (name.clone(), Vec::new()),
            PolicyRef::External { feature, name } => {
                let reference = format!("{feature}.policy.{name}");
                (reference, Vec::new())
            }
        };

        let routes = command
            .route
            .iter()
            .map(|slot| {
                (
                    slot.name.clone(),
                    CommandRouteSlot {
                        bound_from_context: slot.from.is_some(),
                    },
                )
            })
            .collect();

        commands.insert(
            CommandKey {
                feature: feature.name.clone(),
                command: command.name.clone(),
            },
            CommandPolicy {
                reference,
                atoms,
                routes,
            },
        );
    }
}

fn collect_lzx_experience_facts(
    document: &LzxDocument,
    experiences: &mut BTreeMap<String, ExperienceFacts>,
) {
    for experience in &document.experiences {
        let facts = experiences.entry(experience.name.clone()).or_default();
        for view in &experience.views {
            facts.view_routes.insert(
                view.name.clone(),
                view.routes
                    .iter()
                    .filter_map(|route| route_slot_name(route).map(str::to_owned))
                    .collect(),
            );
            let actions = facts.view_actions.entry(view.name.clone()).or_default();
            for action in &view.actions {
                actions.insert(action.name.clone(), action.target.clone());
            }
        }
    }
}

fn collect_lzx_operational_facts(
    file: &DoctorFile,
    document: &LzxDocument,
    operational: &mut OperationalFacts,
) {
    for route in &document.routes {
        let (line, column) = line_col_for_offset(&file.source, route.span.start);
        let fact = SourceFact {
            path: file.path.clone(),
            line,
            column,
            name: route.name.clone(),
        };
        match lzx_route_surface_platform(route.surface.as_deref()) {
            Some(LzxPlatform::Web) => operational.web_routes.push(fact),
            Some(LzxPlatform::Mobile) => operational.mobile_routes.push(fact),
            None => {
                if route.path.is_some() {
                    operational.web_routes.push(fact);
                }
            }
        }
    }

    for surface in &document.surfaces {
        let (line, column) = line_col_for_offset(&file.source, surface.span.start);
        let fact = SourceFact {
            path: file.path.clone(),
            line,
            column,
            name: surface.experience.clone(),
        };
        match surface.platform {
            LzxPlatform::Web => operational.web_surfaces.push(fact),
            LzxPlatform::Mobile => operational.mobile_surfaces.push(fact),
        }
    }
}

fn lzx_route_surface_platform(surface: Option<&str>) -> Option<LzxPlatform> {
    match surface?.split_whitespace().last()? {
        "web" => Some(LzxPlatform::Web),
        "mobile" => Some(LzxPlatform::Mobile),
        _ => None,
    }
}

fn policy_reachability_diagnostics(
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

fn missing_policy_on_query_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
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

fn duplicate_query_name_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
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
fn route_id_effect_consistency_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
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
fn mutation_without_readback_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
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
fn updates_missing_updated_at_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
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


fn app_contract_diagnostics(
    app: Option<&DoctorAppManifest>,
    registry: Option<&DoctorAppRegistry>,
    profiles: &[DoctorAppProfile],
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let Some(app) = app else {
        if !profiles.is_empty() {
            return profiles
                .iter()
                .map(|profile| DoctorDiagnostic {
                    path: profile.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PROFILE-APP-001".to_owned(),
                    message: format!(
                        "profile `{}` is declared, but no package app manifest was found.",
                        profile.profile.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                })
                .collect();
        }
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let manifest = &app.manifest;
    let env_names = operational_env_names(manifest, registry);
    let used_features: BTreeSet<_> = manifest.uses.iter().map(String::as_str).collect();
    let pack_features = enabled_pack_provided_features(manifest, registry);

    for feature in operational.features.values() {
        if !used_features.contains(feature.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-USES-001".to_owned(),
                message: format!(
                    "app manifest does not list local feature `{}` in `uses`; generated app registration may omit it.",
                    feature.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for used in &manifest.uses {
        if !operational.features.contains_key(used) && !pack_features.contains(used.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "APP-USES-002".to_owned(),
                message: format!(
                    "app manifest lists `{used}` in `uses`, but no local feature with that name was found in this package."
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    if !manifest.services.is_empty() {
        diagnostics.extend(app_service_contract_diagnostics(
            app,
            operational,
            &pack_features,
        ));
    }

    diagnostics.extend(adapter_provenance_diagnostics(app, registry, profiles));
    diagnostics.extend(app_pack_contract_diagnostics(app, registry));
    diagnostics.extend(app_binding_contract_diagnostics(app, registry, operational));
    diagnostics.extend(external_call_contract_diagnostics(operational));
    diagnostics.extend(app_route_redirect_diagnostics(app, operational));
    diagnostics.extend(error_page_contract_diagnostics(app));
    diagnostics.extend(profile_contract_diagnostics(
        app,
        registry,
        profiles,
        operational,
    ));

    for env_ref in &operational.env_references {
        if !env_names.contains(env_ref.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: env_ref.path.clone(),
                line: env_ref.line,
                column: env_ref.column,
                severity: DoctorSeverity::Error,
                code: "APP-ENV-001".to_owned(),
                message: format!(
                    "environment reference `env.{}` is not declared in `app.lzi` or `registry.lzi` env.",
                    env_ref.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    if !operational.file_capabilities.is_empty()
        && !app_has_any_capability(manifest, registry, &["object_storage", "storage"])
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-CAP-001",
            "package uses `@cap.File`, but app/registry contract does not declare `object_storage` or `storage` capability.",
        ));
    }

    if !operational.jobs.is_empty() && !app_runtime_runs(manifest, "jobs") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-001",
            "package declares jobs, but app manifest runtime does not declare a unit that `runs jobs *`.",
        ));
    }

    if !operational.schedules.is_empty() && !app_runtime_runs(manifest, "schedules") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-002",
            "package declares scheduled jobs, but app manifest runtime does not declare a unit that `runs schedules *`.",
        ));
    }

    if !operational.webhooks.is_empty() && !app_runtime_serves(manifest, "webhooks") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-003",
            "package declares webhooks, but app manifest runtime does not declare a unit that `serves webhooks`.",
        ));
    }

    if !operational.apis.is_empty() && !app_runtime_serves(manifest, "apis") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-RUNTIME-004",
            "package declares custom APIs, but app manifest runtime does not declare a unit that `serves apis`.",
        ));
    }

    if (!operational.web_routes.is_empty() || !operational.web_surfaces.is_empty())
        && !app_has_target(manifest, "web")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-TARGET-001",
            "package declares web routes/surfaces, but app manifest targets do not include `web <runtime>`.",
        ));
    }

    if (!operational.mobile_routes.is_empty() || !operational.mobile_surfaces.is_empty())
        && !app_has_target(manifest, "mobile")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-TARGET-002",
            "package declares mobile routes/surfaces, but app manifest targets do not include `mobile <runtime>`.",
        ));
    }

    if !operational.web_routes.is_empty() && !app_has_url(manifest, profiles, "web") {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-URL-001",
            "package declares web routes, but app manifest URLs do not include a `web` URL.",
        ));
    }

    if (!operational.webhooks.is_empty() || !operational.apis.is_empty())
        && !app_has_url(manifest, profiles, "api")
    {
        diagnostics.push(app_missing_contract_diagnostic(
            app,
            "APP-URL-002",
            "package declares webhooks or custom APIs, but app manifest URLs do not include an `api` URL.",
        ));
    }

    diagnostics
}

fn app_missing_contract_diagnostic(
    app: &DoctorAppManifest,
    code: &str,
    message: &str,
) -> DoctorDiagnostic {
    DoctorDiagnostic {
        path: app.path.clone(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: code.to_owned(),
        message: message.to_owned(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }
}

fn app_route_redirect_diagnostics(
    app: &DoctorAppManifest,
    operational: &OperationalFacts,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let manifest = &app.manifest;

    let mut declared = BTreeSet::new();
    for fact in operational
        .web_routes
        .iter()
        .chain(operational.mobile_routes.iter())
    {
        declared.insert(fact.name.as_str());
    }

    for (field, value, code) in [
        (
            "auth_failed_redirect",
            manifest.auth_failed_redirect.as_deref(),
            "APP-ROUTE-001",
        ),
        ("not_found", manifest.not_found.as_deref(), "APP-ROUTE-002"),
    ] {
        let Some(target) = value else {
            continue;
        };
        let target = target.trim();
        if target.is_empty() {
            continue;
        }
        if !declared.contains(target) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: code.to_owned(),
                message: format!(
                    "app `{field}` references route `{target}`, but no top-level `.lzx route {target}` was declared in this package."
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

fn error_page_contract_diagnostics(app: &DoctorAppManifest) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen = BTreeSet::new();
    let app_dir = app.path.parent().unwrap_or_else(|| Path::new("."));

    for page in &app.manifest.error_pages {
        let line = error_page_line(app, page.status);
        if !ir::ERROR_PAGE_STATUS_CATALOG.contains(&page.status) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "error-page-contract".to_owned(),
                message: format!(
                    "`error_page {}` is outside the closed status catalog: {}.",
                    page.status,
                    error_page_catalog_display()
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        if !seen.insert(page.status) {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "error-page-duplicate".to_owned(),
                message: format!(
                    "`error_page {}` is declared more than once in the app manifest.",
                    page.status
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
        if page.template.trim().is_empty() {
            continue;
        }
        let template_path = app_dir.join(&page.template);
        if !template_path.exists() {
            diagnostics.push(DoctorDiagnostic {
                path: app.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "error-page-template-missing".to_owned(),
                message: format!(
                    "`error_page {}` template `{}` does not resolve relative to `{}`.",
                    page.status,
                    page.template,
                    app.path.display()
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

fn error_page_line(app: &DoctorAppManifest, status: u16) -> usize {
    let needle = format!("error_page {status}");
    app.source
        .lines()
        .position(|line| line.trim_start() == needle)
        .map(|index| index + 1)
        .unwrap_or(1)
}

fn workspace_contract_diagnostics(workspace: Option<&DoctorAppWorkspace>) -> Vec<DoctorDiagnostic> {
    let Some(workspace) = workspace else {
        return Vec::new();
    };

    let mut diagnostics = Vec::new();
    let mut app_names = BTreeSet::new();
    let mut published = Vec::new();

    for app in &workspace.manifest.apps {
        if !app_names.insert(app.name.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-APP-001".to_owned(),
                message: format!(
                    "workspace declares app `{}` more than once; app ids must be unique.",
                    app.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if app.kind == "local"
            && app
                .path
                .as_deref()
                .is_some_and(|path| !path.ends_with(".lzi"))
        {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "WS-APP-002".to_owned(),
                message: format!(
                    "workspace local app `{}` should point at an `app.lzi` entrypoint.",
                    app.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for boundary in &workspace.manifest.boundaries {
        if !app_names.contains(boundary.app.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-BOUNDARY-001".to_owned(),
                message: format!(
                    "workspace boundary references unknown app `{}`.",
                    boundary.app
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if boundary.direction == "publishes" {
            published.push(boundary.pattern.as_str());
        }
    }

    for boundary in &workspace.manifest.boundaries {
        if boundary.direction != "consumes" {
            continue;
        }
        if !published
            .iter()
            .any(|published| event_pattern_covers(published, &boundary.pattern))
        {
            diagnostics.push(DoctorDiagnostic {
                path: workspace.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "WS-EVENT-001".to_owned(),
                message: format!(
                    "workspace app `{}` consumes `{}`, but no workspace app publishes a compatible event pattern.",
                    boundary.app, boundary.pattern
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    for gateway in &workspace.manifest.gateways {
        for route in &gateway.routes {
            if route.target_kind != "app" {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "WS-GW-001".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` targets `{}`; only `to app <name>` is supported in the language contract.",
                        gateway.name, route.path, route.target_kind
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            } else if !app_names.contains(route.target.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "WS-GW-002".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` targets unknown app `{}`.",
                        gateway.name, route.path, route.target
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if route.auth.as_deref() != Some("propagate") {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-GW-003".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` should declare `auth propagate` so the runtime does not infer auth context.",
                        gateway.name, route.path
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if route.tenant.as_deref() != Some("propagate") {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-GW-004".to_owned(),
                    message: format!(
                        "workspace gateway `{}` route `{}` should declare `tenant propagate` so tenant context crosses app boundaries explicitly.",
                        gateway.name, route.path
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

    if !workspace.manifest.gateways.is_empty() {
        let propagated: BTreeSet<_> = workspace
            .manifest
            .communication
            .as_ref()
            .map(|communication| {
                communication
                    .propagate
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        for required in ["tenant", "trace_id", "request_id"] {
            if !propagated.contains(required) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-COMM-001".to_owned(),
                    message: format!(
                        "workspace gateways should propagate `{required}` in the `communication` block."
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

fn event_pattern_covers(published: &str, consumed: &str) -> bool {
    if published == consumed {
        return true;
    }

    published
        .strip_suffix('*')
        .is_some_and(|prefix| consumed.starts_with(prefix))
}

fn external_contract_diagnostics(
    contracts: &[DoctorAppContract],
    workspace: Option<&DoctorAppWorkspace>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut contract_names = BTreeMap::new();

    for contract in contracts {
        if let Some(previous) = contract_names.insert(contract.manifest.name.as_str(), contract) {
            diagnostics.push(DoctorDiagnostic {
                path: contract.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "CONTRACT-001".to_owned(),
                message: format!(
                    "contract `{}` is declared more than once; first seen in {}.",
                    contract.manifest.name,
                    previous.path.display()
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if contract.manifest.imports.is_empty()
            && contract.manifest.operations.is_empty()
            && contract.manifest.events.is_empty()
        {
            diagnostics.push(DoctorDiagnostic {
                path: contract.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "CONTRACT-002".to_owned(),
                message: format!(
                    "contract `{}` declares no imports, operations, or events.",
                    contract.manifest.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        for operation in &contract.manifest.operations {
            if operation.transport.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-001".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare `transport http|rpc|event`.",
                        contract.manifest.name, operation.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if operation.transport.as_deref() == Some("http")
                && (operation.method.is_none() || operation.path.is_none())
            {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-002".to_owned(),
                    message: format!(
                        "contract `{}` HTTP operation `{}` should declare both `method` and `path`.",
                        contract.manifest.name, operation.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if operation.input.is_none() || operation.output.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-003".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare input and output records.",
                        contract.manifest.name, operation.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if operation.timeout.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-OP-004".to_owned(),
                    message: format!(
                        "contract `{}` operation `{}` should declare timeout so Go transport bindings do not infer it.",
                        contract.manifest.name, operation.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        for event in &contract.manifest.events {
            if event.topic.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: contract.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "CONTRACT-EVENT-001".to_owned(),
                    message: format!(
                        "contract `{}` event `{}` should declare a topic.",
                        contract.manifest.name, event.name
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

    if let Some(workspace) = workspace
        && !contracts.is_empty()
    {
        for app in &workspace.manifest.apps {
            let Some(contract_name) = app.contract.as_deref() else {
                continue;
            };
            if !contract_names.contains_key(contract_name) {
                diagnostics.push(DoctorDiagnostic {
                    path: workspace.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WS-CONTRACT-001".to_owned(),
                    message: format!(
                        "workspace app `{}` references external contract `{contract_name}`, but no local `contract {contract_name}` block was found in this package.",
                        app.name
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

fn app_binding_contract_diagnostics(
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

fn external_call_contract_diagnostics(operational: &OperationalFacts) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let declared_slots: BTreeSet<_> = operational
        .integration_requirements
        .iter()
        .map(|requirement| (requirement.feature.as_str(), requirement.slot.as_str()))
        .collect();

    for call in &operational.external_calls {
        if !declared_slots.contains(&(call.feature.as_str(), call.slot.as_str())) {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Error,
                code: "INT-CALL-001".to_owned(),
                message: format!(
                    "`{}` calls `{}.{}`, but feature `{}` does not declare `requires integration {}: <Contract>`.",
                    call.subject, call.slot, call.operation, call.feature, call.slot
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if !call.has_timeout {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Error,
                code: "INT-CALL-002".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without an explicit `timeout \"...\"` on the {} block.",
                    call.subject, call.slot, call.operation, call.subject_kind
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if !call.has_retry {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Warning,
                code: "INT-CALL-003".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without a visible `retry <count> backoff <strategy>` policy.",
                    call.subject, call.slot, call.operation
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if call.subject_kind == "job" && !call.has_idempotency {
            diagnostics.push(DoctorDiagnostic {
                path: call.path.clone(),
                line: call.line,
                column: call.column,
                severity: DoctorSeverity::Warning,
                code: "INT-CALL-004".to_owned(),
                message: format!(
                    "`{}` calls external operation `{}.{}` without a visible job `idempotency by ...` key.",
                    call.subject, call.slot, call.operation
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

/// Closed catalog for notification channels. `NOTIF-CHANNEL-001` rejects
/// any value not in this list. SPECULATIVE channels (`push`, `sms`)
/// gate on adapter binding evidence; the catalog stays narrow today.
const NOTIFICATION_CHANNEL_CATALOG: &[&str] = &["email", "in_app", "slack", "discord", "webhook"];

fn tier3_diagnostics(
    facts: &[Tier3FeatureFacts],
    registry: Option<&lazuli_ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let webhook_events: BTreeMap<&str, &lazuli_ir::WebhookEvent> = registry
        .map(|r| {
            r.webhook_events
                .iter()
                .map(|e| (e.name.as_str(), e))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    // Notifications expanded bucket cycle — cross-feature event
    // payload index keyed on the qualified `<feature>.<event>` name a
    // `notification.trigger event ...` reference uses. The map carries
    // the union of (event-specific payload fields) and (event_group
    // payload fields inherited via pattern match) so
    // `NOTIF-DIGEST-001`'s `group_by` resolution mirrors what the
    // runtime actually sees on the wire.
    let event_payload_index = build_event_payload_index(facts);

    // Webhooks expanded cycle — track which `webhook_events.<name>`
    // entries are referenced anywhere across the package. Anything
    // unreferenced at the end fires `WEBHOOK-EVENT-001`.
    let mut referenced_envelopes: BTreeSet<&str> = BTreeSet::new();

    for feature in facts {
        // Webhooks expanded cycle — single feature event set used by
        // `WEBHOOK-DLQ-001` (event-name resolution). Pulls from every
        // construct in the feature that can declare or emit an event.
        let mut declared_events: BTreeSet<String> = BTreeSet::new();
        for job in &feature.jobs {
            for e in &job.emits {
                declared_events.insert(e.clone());
            }
        }
        for webhook in &feature.webhooks {
            for e in &webhook.emits {
                declared_events.insert(e.clone());
            }
        }
        for notification in &feature.notifications {
            for e in &notification.emits {
                declared_events.insert(e.clone());
            }
        }
        for group in &feature.event_groups {
            for e in &group.events {
                declared_events.insert(e.clone());
            }
        }

        for job in &feature.jobs {
            tier3_job_diagnostics(feature, job, &mut diagnostics);
        }
        for webhook in &feature.webhooks {
            tier3_webhook_diagnostics(
                feature,
                webhook,
                &webhook_events,
                &declared_events,
                &mut referenced_envelopes,
                &mut diagnostics,
            );
        }
        for notification in &feature.notifications {
            tier3_notification_diagnostics(
                feature,
                notification,
                &event_payload_index,
                &mut diagnostics,
            );
        }
    }

    // WEBHOOK-EVENT-001 — every declared `webhook_events.<X>` envelope
    // must be referenced by at least one `webhook ... payload from`.
    // Dead-letter envelope catalog entries are an authoring smell.
    if let Some(reg) = registry {
        for envelope in &reg.webhook_events {
            if !referenced_envelopes.contains(envelope.name.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    // Without a registry-source line map the diagnostic
                    // points at the package root. The LSP rule still
                    // gives the precise underline on the source line.
                    path: PathBuf::from("registry.lzi"),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "WEBHOOK-EVENT-001".to_owned(),
                    message: format!(
                        "`registry.webhook_events.{}` is declared but no `webhook ... payload from` references it.",
                        envelope.name
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


fn tier3_job_diagnostics(
    feature: &Tier3FeatureFacts,
    job: &lazuli_ir::Job,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let line = feature
        .job_lines
        .get(&job.name)
        .copied()
        .unwrap_or(feature.feature_line);

    // JOB-TIMEOUT-001: job declares external calls but no timeout. The
    // `INT-CALL-002` text-pattern check on `ExternalCallFact` covers
    // the same ground today; this rule fires from the IR lift so the
    // diagnostic survives `parse_command` arriving in Tier 4 and the
    // text-pattern fact disappearing.
    if !job.external_calls.is_empty() && job.timeout.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "JOB-TIMEOUT-001".to_owned(),
            message: format!(
                "job `{}` declares external `calls` but no `timeout \"...\"` — external operations require an explicit timeout.",
                job.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // JOB-FANOUT-001: fanout.axis must match the feature's tenancy axis.
    if let (Some(fanout), Some(axis)) = (&job.fanout, &feature.tenancy_axis) {
        if &fanout.axis != axis {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "JOB-FANOUT-001".to_owned(),
                message: format!(
                    "job `{}` declares `fanout tenants {}` but feature `{}` uses tenancy axis `{}`.",
                    job.name, fanout.axis, feature.feature, axis
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // JOB-FANOUT-002: scheduled job declares fanout but no idempotency
    // key — without a key, fanout re-fire can double-execute on tenants.
    if matches!(job.trigger, lazuli_ir::JobTrigger::Schedule { .. })
        && job.fanout.is_some()
        && job.idempotency.is_none()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "JOB-FANOUT-002".to_owned(),
            message: format!(
                "scheduled job `{}` declares `fanout` but no `idempotency by ...` — re-fires may double-execute per tenant.",
                job.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

fn tier3_webhook_diagnostics<'a>(
    feature: &Tier3FeatureFacts,
    webhook: &'a lazuli_ir::Webhook,
    webhook_events: &BTreeMap<&'a str, &'a lazuli_ir::WebhookEvent>,
    declared_events: &BTreeSet<String>,
    referenced_envelopes: &mut BTreeSet<&'a str>,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let line = feature
        .webhook_lines
        .get(&webhook.name)
        .copied()
        .unwrap_or(feature.feature_line);

    // WEBHOOK-SCOPE-001: webhook missing `tenant_from` on a multi-tenant
    // app. Pilot-gated until the tenancy axis lifter lands (Tier 4);
    // we fire the diagnostic conservatively when the webhook simply
    // lacks `tenant_from` so the LSP rule keeps its coverage.
    //
    // An explicit `scope global` + `reason "..."` declaration silences
    // the lint — matches the LSP rule at `lazuli_lsp/src/lib.rs:10720`
    // (closes the doctor-side gap surfaced by multi-tenant pilot port).
    if webhook.tenant_from.is_none() && webhook.scope_global.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-SCOPE-001".to_owned(),
            message: format!(
                "webhook `{}` does not declare `tenant_from payload.<axis>_id` or explicit `scope global` with a reason — verify it should be globally scoped.",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // Webhooks expanded cycle — `payload from webhook_events.<X>` must
    // resolve to a declared envelope.
    let envelope: Option<&lazuli_ir::WebhookEvent> = match &webhook.payload_from {
        Some(reference) => {
            let resolved = webhook_events.get(reference.name.as_str()).copied();
            if resolved.is_some() {
                referenced_envelopes.insert(reference.name.as_str());
            } else {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "WEBHOOK-PAYLOAD-001".to_owned(),
                    message: format!(
                        "webhook `{}` references `webhook_events.{}` but no such envelope is declared in `registry.webhook_events`.",
                        webhook.name, reference.name
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
            resolved
        }
        None => None,
    };

    // WEBHOOK-PAYLOAD-002 — when the envelope is declared, every
    // `tenant_from payload.<axis>_id` must point at a field that
    // actually exists in the envelope. We only check the first
    // segment after `payload.` so structured/nested probes are
    // tolerated until pilot evidence justifies deeper traversal.
    if let (Some(envelope), Some(tenant_from)) = (envelope, webhook.tenant_from.as_ref())
        && tenant_from.path.segments.first().map(String::as_str) == Some("payload")
        && let Some(axis) = tenant_from.path.segments.get(1)
        && !envelope.payload.iter().any(|f| &f.name == axis)
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-PAYLOAD-002".to_owned(),
            message: format!(
                "webhook `{}` uses `tenant_from payload.{}` but envelope `webhook_events.{}` declares no `{}` field — the runtime will fail at decode time.",
                webhook.name, axis, envelope.name, axis
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-REPLAY-001 — `replay allow` must carry `within "..."`.
    if let Some(replay) = webhook.replay.as_ref()
        && matches!(replay.mode, lazuli_ir::ReplayMode::Allow)
        && replay.within.is_none()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "WEBHOOK-REPLAY-001".to_owned(),
            message: format!(
                "webhook `{}` declares `replay allow` but no `within \"<duration>\"` window — the adapter has no SLA to enforce.",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-REPLAY-002 — replay without `idempotency by ...` has no
    // dedupe key.
    if webhook.replay.is_some()
        && webhook.idempotency.is_none()
        && webhook
            .replay
            .as_ref()
            .and_then(|r| r.dedupe_by.as_ref())
            .is_none()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-REPLAY-002".to_owned(),
            message: format!(
                "webhook `{}` declares `replay` but no `idempotency by ...` nor `dedupe by ...` — replay dedupe has no key.",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-DLQ-001 — `dlq emit <X>` must resolve to a declared
    // event in the same feature.
    if let Some(lazuli_ir::DlqSpec::Emit { event }) = webhook.dlq.as_ref()
        && !declared_events.contains(event)
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "WEBHOOK-DLQ-001".to_owned(),
            message: format!(
                "webhook `{}` `dlq emit {}` references event `{}` that is not declared in feature `{}` (no `emits`, `event_group`, or `event.trace` matches).",
                webhook.name, event, event, feature.feature
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-DLQ-002 — `dlq drop` requires `reason "..."`. The parser
    // already rejects the empty form; doctor keeps a defensive check
    // in case a future analyzer path lowers the field without a
    // reason.
    if let Some(lazuli_ir::DlqSpec::Drop { reason }) = webhook.dlq.as_ref()
        && reason.trim().is_empty()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "WEBHOOK-DLQ-002".to_owned(),
            message: format!(
                "webhook `{}` declares `dlq drop` without `reason \"...\"` — silent drops on dead-letter must be explicit waivers.",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // WEBHOOK-DLQ-003 — `retry` without `dlq` falls through to the
    // adapter default (silent drop on River etc.).
    if webhook.retry.is_some() && webhook.dlq.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-DLQ-003".to_owned(),
            message: format!(
                "webhook `{}` declares `retry` but no `dlq` — after exhaustion the runtime falls back to the adapter default (silent drop on River).",
                webhook.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

fn tier3_notification_diagnostics(
    feature: &Tier3FeatureFacts,
    notification: &lazuli_ir::Notification,
    _event_payload_index: &BTreeMap<String, BTreeSet<String>>,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let line = feature
        .notification_lines
        .get(&notification.name)
        .copied()
        .unwrap_or(feature.feature_line);

    // NOTIF-CHANNEL-001: every channel literal must be in the closed
    // catalog. Empty channel list is also rejected.
    if notification.channels.is_empty() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "NOTIF-CHANNEL-001".to_owned(),
            message: format!(
                "notification `{}` declares no `channel` — at least one of {} is required.",
                notification.name,
                NOTIFICATION_CHANNEL_CATALOG.join(", ")
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    } else {
        for channel in &notification.channels {
            if !NOTIFICATION_CHANNEL_CATALOG.contains(&channel.as_str()) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "NOTIF-CHANNEL-001".to_owned(),
                    message: format!(
                        "notification `{}` declares channel `{}` outside the closed catalog ({}).",
                        notification.name,
                        channel,
                        NOTIFICATION_CHANNEL_CATALOG.join(", ")
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

    // ---------------------------------------------------------------
    // Notifications expanded bucket cycle — six new diagnostics.
    //
    // The shape contracts:
    //   - `digest` aggregates triggers per window per group_by key.
    //   - `throttle` rate-limits per recipient/channel with optional
    //     burst, distinct from scalar `rate_limit`.
    //
    // Each diagnostic anchors at the notification header line; the
    // LSP sub-block hover surfaces the precise child token at edit
    // time.
    // ---------------------------------------------------------------

    if let Some(digest) = notification.digest.as_ref() {
        // NOTIF-DIGEST-001 — `every` must parse as `<N> <unit>` where
        // unit is in the closed catalog (seconds, minutes, hours,
        // days). The catalog mirrors Go's `time.ParseDuration` plus
        // `days` so authors can write "1 day" without dropping into
        // `"24h"`. The doctor reads the literal verbatim; precise
        // numeric ranges are left to the adapter.
        if !is_valid_notification_duration(&digest.every) {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-DIGEST-001".to_owned(),
                message: format!(
                    "notification `{}` declares `digest every \"{}\"` outside the closed shape `<N> (seconds|minutes|hours|days)`.",
                    notification.name, digest.every
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // NOTIF-DIGEST-002 — `max_size` must be in (0, 10_000].
        // Above the ceiling, the adapter would buffer arbitrarily
        // many payloads per window; doctor caps it explicitly so the
        // contract states the bound rather than leaving it implicit.
        if let Some(max_size) = digest.max_size {
            if max_size == 0 || max_size > 10_000 {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "NOTIF-DIGEST-002".to_owned(),
                    message: format!(
                        "notification `{}` declares `digest max_size {}` outside the supported range 1..=10000.",
                        notification.name, max_size
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // NOTIF-DIGEST-003 — `template_strategy` is a closed catalog.
        if let Some(strategy) = digest.invalid_template_strategy.as_deref() {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-DIGEST-003".to_owned(),
                message: format!(
                    "notification `{}` declares `digest template_strategy {}` outside the closed catalog (merge, append).",
                    notification.name, strategy
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    if let Some(throttle) = notification.throttle.as_ref() {
        // NOTIF-THROTTLE-001 — at least one key axis is required.
        if !throttle.per_recipient && !throttle.per_channel {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-THROTTLE-001".to_owned(),
                message: format!(
                    "notification `{}` declares `throttle` without `per_recipient` or `per_channel`; at least one throttle axis is required.",
                    notification.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        if let Some(max_per_seconds) = parse_notification_duration_seconds(&throttle.max_per) {
            // NOTIF-THROTTLE-002 — `burst` cannot exceed the max-per
            // window expressed in seconds. This keeps an immediate
            // burst from being larger than the configured bucket.
            if let Some(burst) = throttle.burst {
                if u64::from(burst) > max_per_seconds {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "NOTIF-THROTTLE-002".to_owned(),
                        message: format!(
                            "notification `{}` declares `throttle burst {}` greater than `max_per \"{}\"`.",
                            notification.name, burst, throttle.max_per
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
        } else {
            // NOTIF-THROTTLE-003 — `max_per` must parse as `<N> <unit>`.
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-THROTTLE-003".to_owned(),
                message: format!(
                    "notification `{}` declares `throttle max_per \"{}\"` outside the closed shape `<N> (seconds|minutes|hours|days)`.",
                    notification.name, throttle.max_per
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

/// Notifications expanded bucket cycle — closed-catalog duration
/// matcher reused by `NOTIF-DIGEST-001` and `NOTIF-THROTTLE-003`.
/// Accepts `<N> <unit>` and `<N><unit>` (Go-style), with units in
/// `{s,sec,secs,second,seconds,m,min,mins,minute,minutes,h,hr,hour,hours,d,day,days}`.
/// The runtime resolves the final string via Go's `time.ParseDuration`;
/// doctor's job is to reject obviously wrong literals at design
/// time so the adapter never sees `"1 month"` or `"forever"`.

/// Notifications expanded bucket cycle — cross-feature event-payload
/// index keyed on `<feature>.<event-name>`. Each entry stores the
/// union of (a) event-specific typed payload fields, (b) `event_group`
/// `raw_payload` lines that apply to the event via the group's glob
/// pattern. Built once per doctor run so `NOTIF-DIGEST-001` is
/// constant-time per notification.
fn build_event_payload_index(facts: &[Tier3FeatureFacts]) -> BTreeMap<String, BTreeSet<String>> {
    let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for feature in facts {
        // (a) Typed events lifted on the feature (legacy flow may
        //     populate `Feature.events` in the future; today the
        //     canonical-indent slice leaves it empty, but the loop is
        //     here so the index stays correct when the legacy lifter
        //     catches up).
        for event in &feature.events {
            let key = format!("{}.{}", feature.feature, event.name);
            let fields: BTreeSet<String> = event.payload.iter().map(|f| f.name.clone()).collect();
            index.entry(key).or_default().extend(fields);
        }

        // (b) Concrete events authored under `event_group <prefix>*`
        //     blocks. The lift stores them as short names; the
        //     qualified event name a notification references is
        //     `<feature>.<prefix><short>`. The payload set is the
        //     union of the group's `payload` block (raw `<name> =
        //     <expr>` lines, plus payload-shaped lines like
        //     `customer_id`).
        for group in &feature.event_groups {
            let prefix = group.pattern.strip_suffix('*').unwrap_or(&group.pattern);
            let mut group_fields: BTreeSet<String> = BTreeSet::new();
            for raw in &group.raw_payload {
                if let Some(name) = leading_assignment_lhs(raw) {
                    group_fields.insert(name.to_owned());
                }
            }
            for short_name in &group.events {
                // Avoid double-prefixing when the author already wrote
                // the full prefixed name (`event customer_archived`
                // instead of `event archived` under `customer_*`).
                let qualified = if short_name.starts_with(prefix) {
                    format!("{}.{}", feature.feature, short_name)
                } else {
                    format!("{}.{}{}", feature.feature, prefix, short_name)
                };
                index
                    .entry(qualified)
                    .or_default()
                    .extend(group_fields.iter().cloned());
            }
        }
    }

    index
}

/// Notifications expanded bucket cycle — extract the LHS of an
/// `<name> = <expr>` assignment captured in `event_group.raw_payload`.
/// Returns the bare field name or `None` if the line is not an
/// assignment (e.g. a deeper `audit ...` or comment leftover).
fn leading_assignment_lhs(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let (lhs, _rest) = trimmed.split_once('=')?;
    let lhs = lhs.trim();
    if lhs.is_empty() || lhs.contains(char::is_whitespace) {
        return None;
    }
    Some(lhs)
}

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


fn profile_contract_diagnostics(
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

fn operational_integrations<'a>(
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

fn app_pack_contract_diagnostics(
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

fn adapter_provenance_diagnostics(
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

fn adapter_source_diagnostic(
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

fn enabled_pack_provided_features<'a>(
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

fn enabled_pack_integration_requirements<'a>(
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

fn integration_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))
}

fn pack_source_name(source: &str) -> Option<&str> {
    source
        .strip_prefix("packs.")
        .or_else(|| source.strip_prefix("registry.packs."))
}

fn integration_environment_allowed(
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

fn app_service_contract_diagnostics(
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

fn app_has_target(app: &AppManifest, target: &str) -> bool {
    app.targets
        .iter()
        .any(|entry| entry.split_whitespace().next() == Some(target))
}

fn profile_url_target_valid(app: &AppManifest, target: &str) -> bool {
    target == "api" && app_has_target(app, "backend") || app_has_target(app, target)
}

fn app_has_url(app: &AppManifest, profiles: &[DoctorAppProfile], target: &str) -> bool {
    app.urls.iter().any(|url| url.target == target)
        || profiles
            .iter()
            .flat_map(|profile| profile.profile.urls.iter())
            .any(|url| url.target == target)
}

fn operational_env_names<'a>(
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
fn collect_object_storage_caps(
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

fn app_has_any_capability(
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

fn app_runtime_serves(app: &AppManifest, service: &str) -> bool {
    app.runtime
        .iter()
        .flat_map(|unit| unit.serves.iter())
        .any(|item| runtime_item_matches(item, service))
}

fn app_runtime_runs(app: &AppManifest, service: &str) -> bool {
    app.runtime
        .iter()
        .flat_map(|unit| unit.runs.iter())
        .any(|item| runtime_item_matches(item, service))
}

fn runtime_item_matches(item: &str, service: &str) -> bool {
    item == "*"
        || item == service
        || item
            .split_whitespace()
            .next()
            .is_some_and(|first| first == service)
}

fn command_reachability_diagnostic(
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

fn command_route_binding_diagnostics(
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

fn resolve_platform_action_target(
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

fn resolve_command_target(target: &str, default_feature: &str) -> Option<ResolvedCommandTarget> {
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

fn split_target_call(target: &str) -> (&str, BTreeSet<String>) {
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

fn parse_integration_requirement(trimmed: &str) -> Option<(&str, &str)> {
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

fn route_slot_name(route: &str) -> Option<&str> {
    route
        .split_once(':')
        .map(|(name, _)| name.trim())
        .or_else(|| route.split_whitespace().next())
        .filter(|name| is_identifier(name))
}

fn audience_can_reach_policy(audience: &str, qualifiers: &[String], atoms: &[String]) -> bool {
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

fn audience_roles(audience: &str, qualifiers: &[String]) -> BTreeSet<String> {
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

fn line_col_for_offset(source: &str, offset: usize) -> (usize, usize) {
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


fn path_references<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
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

fn collect_package_plugin_references(package: &DoctorPackage) -> Vec<PluginReferenceFact> {
    package
        .files
        .iter()
        .filter(|file| is_lzi_path(&file.path))
        .flat_map(|file| collect_plugin_references_in_source(&file.path, &file.source))
        .collect()
}

fn collect_plugin_references_in_source(path: &Path, source: &str) -> Vec<PluginReferenceFact> {
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

fn collect_at_references_in_source(path: &Path, source: &str) -> Vec<AtReferenceFact> {
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

fn plugin_reference_name_len(source: &str) -> usize {
    source
        .bytes()
        .take_while(|byte| {
            byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'.' | b'/')
        })
        .count()
}

fn reference_name_len(source: &str) -> usize {
    source
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'-' | b'/'))
        .count()
}

fn reference_namespace(reference: &str) -> Option<&str> {
    let after_at = reference.strip_prefix('@')?;
    let end = after_at.find(['.', '/']).unwrap_or(after_at.len());
    (end > 0).then_some(&after_at[..end])
}

fn is_allowed_reference_namespace_for_doctor(namespace: &str) -> bool {
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

fn go_mod_lazuli_runtime_version(source: &str) -> Option<String> {
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

const SEMANTIC_TYPE_UNKNOWN_CODE: &str = "semantic_type_unknown";
const SEMANTIC_TYPE_CATALOG: &str =
    "EMAIL, PHONE, URL, UUID, DATE, CURRENCY, MONEY, JSON, GEOPOINT";

fn semantic_type_unknown_diagnostics_for_syntax_feature(
    path: &Path,
    source: &str,
    feature: &lazuli_syntax::FeatureSkeleton,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for command in &feature.commands {
        for route in &command.route {
            push_unknown_semantic_type_text(
                path,
                source,
                &route.type_text,
                route.span.start,
                &mut diagnostics,
            );
        }
        if let lazuli_syntax::CommandInputDecl::Typed(slots) = &command.input {
            for slot in slots {
                push_unknown_semantic_type_text(
                    path,
                    source,
                    &slot.type_text,
                    slot.span.start,
                    &mut diagnostics,
                );
            }
        }
        if let Some(returns) = command.returns.as_deref() {
            push_unknown_semantic_type_text(
                path,
                source,
                returns,
                command.span.start,
                &mut diagnostics,
            );
        }
        if let Some(handler) = command.handler.as_ref() {
            if let Some(returns) = handler.returns.as_deref() {
                push_unknown_semantic_type_text(
                    path,
                    source,
                    returns,
                    command.span.start,
                    &mut diagnostics,
                );
            }
        }
    }

    for query in &feature.queries {
        match query {
            lazuli_syntax::QueryDecl::List(query) => {
                for param in &query.params {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        &param.type_text,
                        param.span.start,
                        &mut diagnostics,
                    );
                }
            }
            lazuli_syntax::QueryDecl::Lookup(query) => {
                for key in &query.keys {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        &key.type_text,
                        key.span.start,
                        &mut diagnostics,
                    );
                }
            }
            lazuli_syntax::QueryDecl::Sql(query) => {
                for param in &query.params {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        &param.type_text,
                        param.span.start,
                        &mut diagnostics,
                    );
                }
                push_unknown_semantic_type_text(
                    path,
                    source,
                    &query.returns,
                    query.span.start,
                    &mut diagnostics,
                );
            }
        }
    }

    for api in &feature.apis {
        push_unknown_semantic_type_text(
            path,
            source,
            &api.output,
            api.span.start,
            &mut diagnostics,
        );
    }

    for job in &feature.jobs {
        match &job.body {
            lazuli_syntax::JobBody::Handler(handler) => {
                if let Some(returns) = handler.returns.as_deref() {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        returns,
                        job.span.start,
                        &mut diagnostics,
                    );
                }
            }
            lazuli_syntax::JobBody::Declarative(_) => {}
            lazuli_syntax::JobBody::None => {}
        }
    }

    for webhook in &feature.webhooks {
        if let Some(handler) = webhook.handler.as_ref() {
            if let Some(returns) = handler.returns.as_deref() {
                push_unknown_semantic_type_text(
                    path,
                    source,
                    returns,
                    webhook.span.start,
                    &mut diagnostics,
                );
            }
        }
    }

    for agent in &feature.agents {
        for slot in &agent.input {
            push_unknown_semantic_type_text(
                path,
                source,
                &slot.type_text,
                slot.span.start,
                &mut diagnostics,
            );
        }
        if let Some(output) = agent.output.as_ref() {
            match output {
                lazuli_syntax::AgentOutput::Stream(type_text)
                | lazuli_syntax::AgentOutput::Plain(type_text) => {
                    push_unknown_semantic_type_text(
                        path,
                        source,
                        type_text,
                        agent.span.start,
                        &mut diagnostics,
                    );
                }
                lazuli_syntax::AgentOutput::Discriminator(_) => {}
            }
        }
        if let Some(expose) = agent.expose.as_ref() {
            for slot in &expose.route_slots {
                push_unknown_semantic_type_text(
                    path,
                    source,
                    &slot.type_text,
                    slot.span.start,
                    &mut diagnostics,
                );
            }
        }
    }

    diagnostics
}

fn semantic_type_unknown_diagnostics_for_feature(
    path: &Path,
    source: &str,
    feature: &lazuli_ir::Feature,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let feature_loc = span_line_col(source, feature.span_ref.as_ref()).unwrap_or((1, 1));

    for resource in &feature.resources {
        let resource_loc = span_line_col(source, resource.span_ref.as_ref()).unwrap_or(feature_loc);
        for field in &resource.fields {
            let loc = span_line_col(source, field.span_ref.as_ref())
                .or_else(|| find_nested_type_site_line(source, resource_loc.0, &field.name))
                .unwrap_or(resource_loc);
            push_unknown_semantic_type(path, &field.type_ref, loc, &mut diagnostics);
        }
    }

    for record in &feature.records {
        let record_loc = span_line_col(source, record.span_ref.as_ref()).unwrap_or(feature_loc);
        for field in &record.fields {
            let loc = span_line_col(source, field.span_ref.as_ref())
                .or_else(|| find_nested_type_site_line(source, record_loc.0, &field.name))
                .unwrap_or(record_loc);
            push_unknown_semantic_type(path, &field.type_ref, loc, &mut diagnostics);
        }
    }

    for event in &feature.events {
        let event_loc = span_line_col(source, event.span_ref.as_ref()).unwrap_or(feature_loc);
        for field in &event.payload {
            let loc =
                find_nested_type_site_line(source, event_loc.0, &field.name).unwrap_or(event_loc);
            push_unknown_semantic_type(path, &field.type_ref, loc, &mut diagnostics);
        }
    }

    for command in &feature.commands {
        let command_loc = span_line_col(source, command.span_ref.as_ref()).unwrap_or(feature_loc);
        for slot in &command.route {
            let loc = find_nested_type_site_line(source, command_loc.0, &slot.name)
                .unwrap_or(command_loc);
            push_unknown_semantic_type(path, &slot.type_ref, loc, &mut diagnostics);
        }
        if let lazuli_ir::CommandInput::Typed(slots) = &command.input {
            check_typed_slots_for_unknown_semantics(
                path,
                source,
                slots,
                command_loc,
                &mut diagnostics,
            );
        }
        check_command_effect_for_unknown_semantics(
            path,
            &command.effect,
            command_loc,
            &mut diagnostics,
        );
    }

    for query in &feature.queries {
        let query_loc = query_line_col(source, query).unwrap_or(feature_loc);
        match query {
            lazuli_ir::Query::List(query) => {
                check_typed_slots_for_unknown_semantics(
                    path,
                    source,
                    &query.params,
                    query_loc,
                    &mut diagnostics,
                );
            }
            lazuli_ir::Query::Lookup(query) => {
                check_typed_slots_for_unknown_semantics(
                    path,
                    source,
                    &query.params,
                    query_loc,
                    &mut diagnostics,
                );
            }
            lazuli_ir::Query::Sql(query) => {
                check_typed_slots_for_unknown_semantics(
                    path,
                    source,
                    &query.params,
                    query_loc,
                    &mut diagnostics,
                );
                push_unknown_semantic_type(path, &query.returns, query_loc, &mut diagnostics);
            }
        }
    }

    for job in &feature.jobs {
        let job_loc = span_line_col(source, job.span_ref.as_ref()).unwrap_or(feature_loc);
        match &job.body {
            lazuli_ir::JobBody::Handler(handler) => {
                if let Some(returns) = handler.returns.as_ref() {
                    push_unknown_semantic_type(path, returns, job_loc, &mut diagnostics);
                }
            }
            lazuli_ir::JobBody::Declarative(body) => {
                check_command_effect_for_unknown_semantics(
                    path,
                    &body.effect,
                    job_loc,
                    &mut diagnostics,
                );
            }
        }
    }

    for webhook in &feature.webhooks {
        let webhook_loc = span_line_col(source, webhook.span_ref.as_ref()).unwrap_or(feature_loc);
        if let Some(returns) = webhook.returns.as_ref() {
            push_unknown_semantic_type(path, returns, webhook_loc, &mut diagnostics);
        }
    }

    for api in &feature.apis {
        let api_loc = span_line_col(source, api.span_ref.as_ref()).unwrap_or(feature_loc);
        push_unknown_semantic_type(path, &api.output, api_loc, &mut diagnostics);
    }

    for agent in &feature.agents {
        let agent_loc = span_line_col(source, agent.span_ref.as_ref()).unwrap_or(feature_loc);
        check_typed_slots_for_unknown_semantics(
            path,
            source,
            &agent.input,
            agent_loc,
            &mut diagnostics,
        );
        if let Some(output_type) = agent.output_type.as_ref() {
            push_unknown_semantic_type(path, output_type, agent_loc, &mut diagnostics);
        }
        if let Some(expose) = agent.expose_http.as_ref() {
            let expose_loc = span_line_col(source, expose.span_ref.as_ref()).unwrap_or(agent_loc);
            check_typed_slots_for_unknown_semantics(
                path,
                source,
                &expose.route_slots,
                expose_loc,
                &mut diagnostics,
            );
        }
    }

    for extension in &feature.extensions {
        let extension_loc =
            span_line_col(source, extension.span_ref.as_ref()).unwrap_or(feature_loc);
        check_extension_contract_for_unknown_semantics(
            path,
            &extension.contract,
            extension_loc,
            &mut diagnostics,
        );
    }

    diagnostics
}

fn check_typed_slots_for_unknown_semantics(
    path: &Path,
    source: &str,
    slots: &[lazuli_ir::TypedSlot],
    parent_loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    for slot in slots {
        let loc =
            find_nested_type_site_line(source, parent_loc.0, &slot.name).unwrap_or(parent_loc);
        push_unknown_semantic_type(path, &slot.type_ref, loc, diagnostics);
    }
}

fn check_command_effect_for_unknown_semantics(
    path: &Path,
    effect: &lazuli_ir::CommandEffect,
    loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    if let lazuli_ir::CommandEffect::Returns(returns) = effect {
        push_unknown_semantic_type(path, &returns.return_type, loc, diagnostics);
    }
}

fn check_extension_contract_for_unknown_semantics(
    path: &Path,
    contract: &lazuli_ir::ExtensionContract,
    loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    match contract {
        lazuli_ir::ExtensionContract::CellRenderer { type_arg }
        | lazuli_ir::ExtensionContract::ViewBlock { type_arg }
        | lazuli_ir::ExtensionContract::FormField { type_arg }
        | lazuli_ir::ExtensionContract::Hook { type_arg }
        | lazuli_ir::ExtensionContract::Validator { type_arg }
        | lazuli_ir::ExtensionContract::QueryModifier { type_arg }
        | lazuli_ir::ExtensionContract::IntegrationAdapter { type_arg } => {
            push_unknown_semantic_type(path, type_arg, loc, diagnostics);
        }
        lazuli_ir::ExtensionContract::Function { input, output } => {
            push_unknown_semantic_type(path, input, loc, diagnostics);
            push_unknown_semantic_type(path, output, loc, diagnostics);
        }
    }
}

fn push_unknown_semantic_type(
    path: &Path,
    type_ref: &lazuli_ir::TypeRef,
    loc: (usize, usize),
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    if let Some(name) = unknown_semantic_type_name(type_ref) {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line: loc.0,
            column: loc.1,
            severity: DoctorSeverity::Error,
            code: SEMANTIC_TYPE_UNKNOWN_CODE.to_owned(),
            message: format!(
                "unknown @semantic type \"{name}\"; the closed catalog is {{{SEMANTIC_TYPE_CATALOG}}}."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

fn push_unknown_semantic_type_text(
    path: &Path,
    source: &str,
    type_text: &str,
    offset: usize,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let loc = line_col_for_offset(source, offset);
    for name in unknown_semantic_type_names_in_text(type_text) {
        diagnostics.push(DoctorDiagnostic {
            path: path.to_path_buf(),
            line: loc.0,
            column: loc.1,
            severity: DoctorSeverity::Error,
            code: SEMANTIC_TYPE_UNKNOWN_CODE.to_owned(),
            message: format!(
                "unknown @semantic type \"{name}\"; the closed catalog is {{{SEMANTIC_TYPE_CATALOG}}}."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}

fn unknown_semantic_type_name(type_ref: &lazuli_ir::TypeRef) -> Option<&str> {
    match type_ref {
        lazuli_ir::TypeRef::UserDefined(qname)
            if qname.name.starts_with("@semantic.")
                && !is_known_semantic_type_name(qname.name.as_str()) =>
        {
            Some(qname.name.as_str())
        }
        lazuli_ir::TypeRef::Many(inner) => unknown_semantic_type_name(inner),
        _ => None,
    }
}

fn unknown_semantic_type_names_in_text(type_text: &str) -> Vec<&str> {
    type_text
        .split(|ch: char| !(ch == '@' || ch == '.' || ch == '_' || ch.is_ascii_alphanumeric()))
        .filter(|token| token.starts_with("@semantic.") && !is_known_semantic_type_name(token))
        .collect()
}

fn is_known_semantic_type_name(name: &str) -> bool {
    let Some(short) = name.strip_prefix("@semantic.") else {
        return false;
    };
    matches!(
        short,
        "Email"
            | "Phone"
            | "URL"
            | "Url"
            | "UUID"
            | "Uuid"
            | "Date"
            | "Currency"
            | "Money"
            | "JSON"
            | "Json"
            | "GeoPoint"
    )
}

fn span_line_col(source: &str, span: Option<&lazuli_ir::SpanRef>) -> Option<(usize, usize)> {
    span.map(|span| line_col_for_offset(source, span.start))
}

fn query_line_col(source: &str, query: &lazuli_ir::Query) -> Option<(usize, usize)> {
    match query {
        lazuli_ir::Query::List(query) => span_line_col(source, query.span_ref.as_ref()),
        lazuli_ir::Query::Lookup(query) => span_line_col(source, query.span_ref.as_ref()),
        lazuli_ir::Query::Sql(query) => span_line_col(source, query.span_ref.as_ref()),
    }
}

fn find_nested_type_site_line(
    source: &str,
    parent_line: usize,
    site_name: &str,
) -> Option<(usize, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let parent_index = parent_line.checked_sub(1)?;
    let parent_indent = lines
        .get(parent_index)
        .map(|line| leading_spaces(line))
        .unwrap_or(0);
    let field_prefix = format!("{site_name}:");
    let route_prefix = format!("route {site_name}:");

    for (idx, line) in lines.iter().enumerate().skip(parent_index + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= parent_indent {
            break;
        }
        if trimmed.starts_with(&field_prefix) || trimmed.starts_with(&route_prefix) {
            let column = line
                .find(site_name)
                .map(|offset| offset + 1)
                .unwrap_or(indent + 1);
            return Some((idx + 1, column));
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

/// Phase L Tier 4 follow-up (third wave, final) — populate
/// `feature_symbols.commands` from the typed `Tier3FeatureFacts.commands`
/// slot. Replaces the retired `collect_feature_symbols` /
/// `scan_feature_range` / `scan_block_for_policy` text walkers. Only the
/// `policy: Option<String>` text-hint for `agent_tool_policy_diagnostics`
/// survives in `CommandSymbolFact`; the `base: SymbolFact` slot is dead
/// (kept default for shape stability until `FeatureSymbols` itself is
/// retired by Cut A.5 / Cut B follow-ups). `PolicyRef` is rendered into
/// the same surface text the walker produced — `@policy.<name>` for
/// local, `@<atom>` for atom, `<feature>.<name>` for external,
/// `Unresolved` text verbatim — so `policy_atoms_more_restrictive`
/// substring matching stays a one-to-one swap.
fn populate_feature_symbols_from_ir(
    tier3_facts: &[Tier3FeatureFacts],
    feature_symbols: &mut BTreeMap<String, FeatureSymbols>,
) {
    for fact in tier3_facts {
        let symbols = feature_symbols.entry(fact.feature.clone()).or_default();
        for command in &fact.commands {
            symbols.commands.insert(
                command.name.clone(),
                CommandSymbolFact {
                    base: SymbolFact::default(),
                    policy: policy_ref_surface_text(&command.policy),
                },
            );
        }
    }
}

/// Render a `PolicyRef` into the same surface text the retired
/// `scan_block_for_policy` walker captured verbatim from `policy <text>`
/// child lines. `PolicyRef::None` returns `None` so the IR-driven
/// populator skips empty entries the way the walker skipped missing
/// `policy` clauses.
fn policy_ref_surface_text(p: &ir::PolicyRef) -> Option<String> {
    match p {
        ir::PolicyRef::Local(name) => Some(format!("@policy.{name}")),
        ir::PolicyRef::Atom(atom) => Some(atom.clone()),
        ir::PolicyRef::External { feature, name } => Some(format!("{feature}.{name}")),
        ir::PolicyRef::Unresolved(text) => Some(text.clone()),
        ir::PolicyRef::None => None,
    }
}

// -----------------------------------------------------------------------------
// Diagnostic id: cross_feature_type_unresolved
// -----------------------------------------------------------------------------

fn cross_feature_type_unresolved_diagnostics(
    files: &[DoctorFile],
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> Vec<DoctorDiagnostic> {
    let declared_types = build_cross_feature_declared_type_index(tier3_facts, feature_resources);
    let mut diagnostics = Vec::new();
    let mut reported: BTreeSet<(PathBuf, String, String)> = BTreeSet::new();

    for (feature_name, resources) in feature_resources {
        for (resource_name, resource) in resources {
            for (field_name, field) in &resource.fields {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &field.type_ref,
                    &resource.path,
                    field.line.max(resource.line).max(1),
                    format!("{feature_name}.{resource_name}.{field_name}"),
                );
            }
        }
    }

    for fact in tier3_facts {
        for record in &fact.records {
            for field in &record.fields {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &field.type_ref,
                    &fact.path,
                    span_line(files, &fact.path, field.span_ref, fact.feature_line),
                    format!("{}.{}.{}", fact.feature, record.name, field.name),
                );
            }
        }

        for command in &fact.commands {
            let command_line = fact
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, command.span_ref, fact.feature_line)
                });

            for slot in &command.route {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &slot.type_ref,
                    &fact.path,
                    command_line.max(1),
                    format!("{}.{}.route.{}", fact.feature, command.name, slot.name),
                );
            }

            if let ir::CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    push_unresolved_type_ref_diagnostic(
                        &mut diagnostics,
                        &mut reported,
                        &declared_types,
                        &slot.type_ref,
                        &fact.path,
                        command_line.max(1),
                        format!("{}.{}.input.{}", fact.feature, command.name, slot.name),
                    );
                }
            }

            if let ir::CommandEffect::Returns(returns) = &command.effect {
                push_unresolved_type_ref_diagnostic(
                    &mut diagnostics,
                    &mut reported,
                    &declared_types,
                    &returns.return_type,
                    &fact.path,
                    command_line.max(1),
                    format!("{}.{}.returns", fact.feature, command.name),
                );
            }
        }
    }

    diagnostics
}

fn build_cross_feature_declared_type_index(
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> BTreeSet<String> {
    let mut declared = BTreeSet::new();

    for resources in feature_resources.values() {
        declared.extend(resources.keys().cloned());
    }
    for fact in tier3_facts {
        declared.extend(fact.records.iter().map(|record| record.name.clone()));
        declared.extend(fact.enums.iter().map(|enum_decl| enum_decl.name.clone()));
    }

    declared
}

fn push_unresolved_type_ref_diagnostic(
    diagnostics: &mut Vec<DoctorDiagnostic>,
    reported: &mut BTreeSet<(PathBuf, String, String)>,
    declared_types: &BTreeSet<String>,
    type_ref: &ir::TypeRef,
    path: &Path,
    line: usize,
    site: String,
) {
    let Some(name) = unresolved_bare_user_type_name(type_ref, declared_types) else {
        return;
    };
    if !reported.insert((path.to_path_buf(), site.clone(), name.to_owned())) {
        return;
    }

    diagnostics.push(DoctorDiagnostic {
        path: path.to_path_buf(),
        line,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "cross_feature_type_unresolved".to_owned(),
        message: format!(
            "type `{name}` referenced by `{site}` is not declared in any feature. Add a `resource`/`record`/`enum {name}` block, or check for a typo."
        ),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    });
}

fn unresolved_bare_user_type_name<'a>(
    type_ref: &'a ir::TypeRef,
    declared_types: &BTreeSet<String>,
) -> Option<&'a str> {
    match type_ref {
        ir::TypeRef::UserDefined(qname) | ir::TypeRef::EnumRef(qname)
            if qname.feature.is_none()
                && !declared_types.contains(&qname.name)
                && !is_trivial_type_ref_name(&qname.name) =>
        {
            Some(qname.name.as_str())
        }
        _ => None,
    }
}

fn is_trivial_type_ref_name(name: &str) -> bool {
    let trimmed = name.trim();
    trimmed.len() <= 1 || trimmed.starts_with('@')
}

pub(crate) fn span_line(
    files: &[DoctorFile],
    path: &Path,
    span_ref: Option<lazuli_ir::SpanRef>,
    fallback: usize,
) -> usize {
    span_ref
        .and_then(|span| {
            files
                .iter()
                .find(|file| file.path.as_path() == path)
                .map(|file| line_col_for_offset(&file.source, span.start).0)
        })
        .unwrap_or(fallback.max(1))
}

// -----------------------------------------------------------------------------
// Diagnostic id: feature_uses_missing
// -----------------------------------------------------------------------------

fn feature_uses_missing_diagnostics(
    files: &[DoctorFile],
    tier3_facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<DoctorDiagnostic> {
    let type_owners = DoctorCrossFeatureTypeIndex::build(tier3_facts, feature_resources);
    let mut missing: BTreeMap<(String, String), MissingUsesRef> = BTreeMap::new();

    for (feature_name, resources) in feature_resources {
        for resource in resources.values() {
            for field in resource.fields.values() {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    feature_name,
                    &field.type_ref,
                    &resource.path,
                    field.line.max(resource.line).max(1),
                );
            }
        }
    }

    for fact in tier3_facts {
        for record in &fact.records {
            for field in &record.fields {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &field.type_ref,
                    &fact.path,
                    span_line(files, &fact.path, field.span_ref, fact.feature_line),
                );
            }
        }

        for command in &fact.commands {
            let command_line = fact
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, command.span_ref, fact.feature_line)
                })
                .max(1);

            for slot in &command.route {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &slot.type_ref,
                    &fact.path,
                    command_line,
                );
            }

            if let ir::CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    record_missing_uses_ref(
                        &mut missing,
                        &type_owners,
                        &fact.feature,
                        &slot.type_ref,
                        &fact.path,
                        command_line,
                    );
                }
            }

            if let ir::CommandEffect::Returns(returns) = &command.effect {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &returns.return_type,
                    &fact.path,
                    command_line,
                );
            }
        }

        for query in &fact.queries {
            let query_line = fact
                .query_lines
                .get(query.name())
                .copied()
                .unwrap_or_else(|| {
                    span_line(files, &fact.path, query_span_ref(query), fact.feature_line)
                })
                .max(1);
            for slot in query_params(query) {
                record_missing_uses_ref(
                    &mut missing,
                    &type_owners,
                    &fact.feature,
                    &slot.type_ref,
                    &fact.path,
                    query_line,
                );
            }
        }
    }

    missing
        .into_iter()
        .filter(|((feature, dependency), _)| {
            !feature_uses
                .get(feature)
                .map(|uses| uses.contains(dependency))
                .unwrap_or(false)
        })
        .map(|((feature, dependency), site)| DoctorDiagnostic {
            path: site.path,
            line: site.line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "feature_uses_missing".to_owned(),
            message: format!(
                "feature `{feature}` references types declared in feature `{dependency}` but does not declare `uses {dependency}` in its header. Add `uses {dependency}` to make the dependency explicit."
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

#[derive(Debug, Clone)]
struct MissingUsesRef {
    path: PathBuf,
    line: usize,
}

fn record_missing_uses_ref(
    missing: &mut BTreeMap<(String, String), MissingUsesRef>,
    type_owners: &DoctorCrossFeatureTypeIndex,
    feature: &str,
    type_ref: &ir::TypeRef,
    path: &Path,
    line: usize,
) {
    let mut owners = BTreeSet::new();
    collect_cross_feature_type_ref_owners(&mut owners, type_owners, feature, type_ref);
    for owner in owners {
        missing
            .entry((feature.to_owned(), owner))
            .or_insert_with(|| MissingUsesRef {
                path: path.to_path_buf(),
                line: line.max(1),
            });
    }
}

fn collect_cross_feature_type_ref_owners(
    owners: &mut BTreeSet<String>,
    type_owners: &DoctorCrossFeatureTypeIndex,
    feature: &str,
    type_ref: &ir::TypeRef,
) {
    match type_ref {
        ir::TypeRef::UserDefined(qname) | ir::TypeRef::EnumRef(qname) => {
            if qname.name.starts_with('@') {
                return;
            }
            let owner = qname
                .feature
                .as_deref()
                .or_else(|| type_owners.owner(&qname.name));
            if let Some(owner) = owner {
                if owner != feature {
                    owners.insert(owner.to_owned());
                }
            }
        }
        ir::TypeRef::Many(inner) => {
            collect_cross_feature_type_ref_owners(owners, type_owners, feature, inner);
        }
        ir::TypeRef::Unresolved(name) => {
            // Command/query lowering still carries authored custom
            // types as `Unresolved`; the codegen cross-feature pass
            // resolves these names against the same owner index.
            if name.starts_with('@') {
                return;
            }
            if let Some(owner) = type_owners.owner(name) {
                if owner != feature {
                    owners.insert(owner.to_owned());
                }
            }
        }
        _ => {}
    }
}

fn query_params(query: &ir::Query) -> &[ir::TypedSlot] {
    match query {
        ir::Query::List(query) => &query.params,
        ir::Query::Lookup(query) => &query.params,
        ir::Query::Sql(query) => &query.params,
    }
}

fn query_span_ref(query: &ir::Query) -> Option<ir::SpanRef> {
    match query {
        ir::Query::List(query) => query.span_ref,
        ir::Query::Lookup(query) => query.span_ref,
        ir::Query::Sql(query) => query.span_ref,
    }
}

#[derive(Debug, Clone, Default)]
struct DoctorCrossFeatureTypeIndex {
    map: BTreeMap<String, String>,
    ambiguous: BTreeMap<String, BTreeSet<String>>,
}

impl DoctorCrossFeatureTypeIndex {
    fn build(
        tier3_facts: &[Tier3FeatureFacts],
        feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    ) -> Self {
        let mut index = Self::default();

        for (feature, resources) in feature_resources {
            for resource_name in resources.keys() {
                index.register(resource_name, feature);
            }
        }
        for fact in tier3_facts {
            for record in &fact.records {
                index.register(&record.name, &fact.feature);
            }
            for enum_decl in &fact.enums {
                index.register(&enum_decl.name, &fact.feature);
            }
        }

        index
    }

    fn register(&mut self, name: &str, feature: &str) {
        if let Some(owners) = self.ambiguous.get_mut(name) {
            owners.insert(feature.to_owned());
            return;
        }

        match self.map.remove(name) {
            Some(existing) if existing == feature => {
                self.map.insert(name.to_owned(), existing);
            }
            Some(existing) => {
                let mut owners = BTreeSet::new();
                owners.insert(existing);
                owners.insert(feature.to_owned());
                self.ambiguous.insert(name.to_owned(), owners);
            }
            None => {
                self.map.insert(name.to_owned(), feature.to_owned());
            }
        }
    }

    fn owner(&self, name: &str) -> Option<&str> {
        self.map.get(name).map(String::as_str)
    }
}

// -----------------------------------------------------------------------------
// Diagnostic id: tool_registry_effect_required_diagnostics
// -----------------------------------------------------------------------------

fn registry_tool_effect_diagnostics(defects: &[RegistryToolDefect]) -> Vec<DoctorDiagnostic> {
    defects
        .iter()
        .map(|defect| DoctorDiagnostic {
            path: defect.path.clone(),
            line: defect.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "tool_registry_effect_required_diagnostics".to_owned(),
            message: match defect.reason {
                RegistryToolDefectReason::EffectMissing => format!(
                    "registry tool `{}` is missing `effect: read | write`; doctor cannot derive the tool's effect from the registry without it.",
                    defect.name
                ),
                RegistryToolDefectReason::EffectInvalid => format!(
                    "registry tool `{}` declares an unknown `effect`; valid values are `read` and `write`.",
                    defect.name
                ),
            },
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

// -----------------------------------------------------------------------------
// Diagnostic ids: agent_tool_policy / write_unguarded / pii_unsafetied
// -----------------------------------------------------------------------------

fn agent_tool_diagnostics(
    agents: &[AgentFacts],
    feature_symbols: &BTreeMap<String, FeatureSymbols>,
    registry: Option<&DoctorAppRegistry>,
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let registry_tools: BTreeMap<String, &lazuli_ir::RegistryToolEntry> = registry
        .map(|r| {
            r.manifest
                .tools
                .iter()
                .map(|t| (t.name.clone(), t))
                .collect()
        })
        .unwrap_or_default();

    // Cut A.9: write-tool guard accepts `approval` on the target
    // command as an alternative to the agent's `safety` validator.
    // Build a quick lookup keyed by (feature, command) so the guard
    // check resolves per-tool in O(1). Sourced from IR
    // `Command.approval` populated by `lower_feature_skeleton`;
    // Phase L Tier 4b retired the `CommandApprovalFact` text-walker.
    let approval_index: BTreeSet<(String, String)> = tier3_facts
        .iter()
        .flat_map(|f| {
            let feature = f.feature.clone();
            f.commands
                .iter()
                .filter(|c| c.approval.is_some())
                .map(move |c| (feature.clone(), c.name.clone()))
        })
        .collect();

    for fact in agents {
        let agent = &fact.agent;
        let agent_safety_empty = agent.safety.is_empty();
        let mut has_unguarded_write_tool = false;
        let mut has_pii_tool = false;
        let agent_policy_text = format_agent_policy(agent);

        for binding in &agent.tools {
            let (tool_label, resolved) =
                resolve_tool(fact, &binding.reference, feature_symbols, &registry_tools);

            if resolved.effect == ResolvedToolEffect::Write {
                // Check whether the target command carries an
                // `approval` block — that satisfies the write-tool
                // guard for this binding, regardless of the agent's
                // own `safety` list.
                let approved = match &binding.reference {
                    lazuli_ir::QualifiedToolRef::Local { kind, name }
                        if matches!(kind, lazuli_ir::ToolKind::Command) =>
                    {
                        approval_index.contains(&(fact.feature.clone(), name.clone()))
                    }
                    lazuli_ir::QualifiedToolRef::CrossFeature {
                        feature,
                        kind,
                        name,
                    } if matches!(kind, lazuli_ir::ToolKind::Command) => {
                        approval_index.contains(&(feature.clone(), name.clone()))
                    }
                    _ => false,
                };
                if !approved {
                    has_unguarded_write_tool = true;
                }
            }
            if !resolved.pii_classes.is_empty() {
                has_pii_tool = true;
            }

            // agent_tool_policy_diagnostics: when the tool's policy is
            // known and stricter than the agent's, emit. Cut A keeps the
            // comparison conservative: we report a gap only when both
            // sides resolve and the tool's policy is *more* restrictive
            // by surface (atom set is a strict superset). Full lattice
            // ranking lands when the policy lattice helper migrates here.
            if let Some(tool_policy) = &resolved.policy {
                if policy_atoms_more_restrictive(tool_policy, &agent_policy_text) {
                    diagnostics.push(DoctorDiagnostic {
                        path: fact.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_tool_policy_diagnostics".to_owned(),
                        message: format!(
                            "agent `{}` declares policy `{}`, but tool `{}` requires `{}` — agent policy must be at least as strict as every tool.",
                            agent.name,
                            agent_policy_text,
                            tool_label,
                            tool_policy,
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

        // agent_tool_write_unguarded_diagnostics: every write tool
        // must be guarded by either the agent's `safety` validator or
        // the target command's `approval` block (Cut A.9 extension).
        // `has_unguarded_write_tool` only stays true for write tools
        // whose command has no approval — agent.safety is the
        // fallback guard for those.
        if has_unguarded_write_tool && agent_safety_empty {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "agent_tool_write_unguarded_diagnostics".to_owned(),
                message: format!(
                    "agent `{}` dispatches a `write` tool with neither `safety @validator.<name>` on the agent nor `approval` on the target command; Cut A requires at least one guard for write-effect tools.",
                    agent.name
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // agent_pii_unsafetied_warning: any PII-bearing tool plus an
        // empty safety list emits a warning.
        if has_pii_tool && agent_safety_empty {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "agent_pii_unsafetied_warning".to_owned(),
                message: format!(
                    "agent `{}` invokes a tool that resolves to a `@pii.*` class but declares no `safety @validator.<name>`; consider adding a scrub validator.",
                    agent.name
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvedToolEffect {
    Read,
    Write,
    Unknown,
}

#[derive(Debug, Clone)]
struct ResolvedTool {
    effect: ResolvedToolEffect,
    policy: Option<String>,
    pii_classes: Vec<String>,
}

fn resolve_tool(
    fact: &AgentFacts,
    reference: &ir::QualifiedToolRef,
    feature_symbols: &BTreeMap<String, FeatureSymbols>,
    registry_tools: &BTreeMap<String, &lazuli_ir::RegistryToolEntry>,
) -> (String, ResolvedTool) {
    match reference {
        ir::QualifiedToolRef::Adapter { dotted } => {
            let key = dotted.join(".");
            let label = format!("@tool.{key}");
            let resolved = registry_tools
                .get(&key)
                .map(|entry| ResolvedTool {
                    effect: match entry.effect {
                        ir::ToolEffect::Read => ResolvedToolEffect::Read,
                        ir::ToolEffect::Write => ResolvedToolEffect::Write,
                    },
                    policy: None,
                    pii_classes: entry.pii_classes.iter().map(|q| q.name.clone()).collect(),
                })
                .unwrap_or(ResolvedTool {
                    effect: ResolvedToolEffect::Unknown,
                    policy: None,
                    pii_classes: Vec::new(),
                });
            (label, resolved)
        }
        ir::QualifiedToolRef::Local { kind, name }
        | ir::QualifiedToolRef::CrossFeature { kind, name, .. } => {
            let owning_feature = match reference {
                ir::QualifiedToolRef::CrossFeature { feature, .. } => feature.clone(),
                _ => fact.feature.clone(),
            };
            let kind_word = tool_kind_word(*kind);
            let label = format!("{}.{}.{}", owning_feature, kind_word, name);
            let symbols = feature_symbols.get(&owning_feature);
            let resolved = match (*kind, symbols) {
                (ir::ToolKind::Command, Some(syms)) => syms
                    .commands
                    .get(name)
                    .map(|cmd| ResolvedTool {
                        effect: ResolvedToolEffect::Write,
                        policy: cmd.policy.clone(),
                        pii_classes: Vec::new(),
                    })
                    .unwrap_or(ResolvedTool {
                        effect: ResolvedToolEffect::Write,
                        policy: None,
                        pii_classes: Vec::new(),
                    }),
                (
                    ir::ToolKind::QueryList
                    | ir::ToolKind::QueryLookup
                    | ir::ToolKind::QuerySql
                    | ir::ToolKind::QueryView
                    | ir::ToolKind::QueryUnspecified,
                    _,
                ) => {
                    // Phase L Tier 4b — the `FeatureSymbols.queries`
                    // text-walker retired. Queries are always read-effect
                    // for the tool resolver; query-level `policy`
                    // declarations are a future extension (today, queries
                    // inherit feature-level `policies` via the analyzer,
                    // which the tool resolver does not consume).
                    ResolvedTool {
                        effect: ResolvedToolEffect::Read,
                        policy: None,
                        pii_classes: Vec::new(),
                    }
                }
                (ir::ToolKind::Command, None) => ResolvedTool {
                    effect: ResolvedToolEffect::Write,
                    policy: None,
                    pii_classes: Vec::new(),
                },
                _ => ResolvedTool {
                    effect: ResolvedToolEffect::Read,
                    policy: None,
                    pii_classes: Vec::new(),
                },
            };
            (label, resolved)
        }
    }
}


/// Conservative `more restrictive than` check: a policy is considered
/// stricter than the agent's when both texts parse as `@policy.<x>` and
/// the names diverge in a documented hierarchy. For Cut A we surface a
/// gap whenever the tool policy text is a non-empty stricter category
/// (`delete`, `update`) and the agent's is a weaker one (`read`).
///
/// Plan §5.4 punts the full lattice migration to a later cut; this stub
/// keeps the diagnostic firing for the obvious cases without false
/// positives.
fn policy_atoms_more_restrictive(tool_policy: &str, agent_policy: &str) -> bool {
    let order = |text: &str| match text {
        s if s.contains("delete") => 3,
        s if s.contains("update") => 2,
        s if s.contains("create") => 1,
        s if s.contains("read") => 0,
        _ => 0,
    };
    order(tool_policy) > order(agent_policy)
}

// -----------------------------------------------------------------------------
// Diagnostic ids: agent_discriminator_target_invalid / field_invalid
// -----------------------------------------------------------------------------

/// Phase L Tier 4 follow-up — fully IR-driven replacement for the
/// records/enums branches of `scan_feature_range`. Records read from
/// `Tier3FeatureFacts.records` (typed `ir::Record` lift); enums read
/// from `Tier3FeatureFacts.enums` (typed `ir::EnumDecl` lift). The
/// retired `FeatureSymbols.enums` text walker is gone.
fn agent_discriminator_diagnostics(
    agents: &[AgentFacts],
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let any_enum = |name: &str| -> bool {
        tier3_facts
            .iter()
            .any(|f| f.enums.iter().any(|e| e.name == name))
    };
    let any_record = |name: &str| -> bool {
        tier3_facts
            .iter()
            .any(|f| f.records.iter().any(|r| r.name == name))
    };

    for fact in agents {
        let agent = &fact.agent;
        match (&agent.output_kind, agent.output_discriminator.as_ref()) {
            (ir::AgentOutputKind::DiscriminatedEnum, Some(ir::DiscriminatorRef::Enum(qn))) => {
                let enum_name = qn.name.as_str();
                if !any_enum(enum_name) {
                    diagnostics.push(DoctorDiagnostic {
                        path: fact.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_discriminator_target_invalid_diagnostics".to_owned(),
                        message: format!(
                            "agent `{}` declares `output discriminator {}` but no enum named `{}` exists in any reachable feature.",
                            agent.name, enum_name, enum_name,
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }
            // DiscriminatedRecord lowering produces output_kind=Text +
            // output_type=Unresolved("X") today; the expand pass (Phase 5)
            // is what promotes to DiscriminatedRecord and resolves the
            // discriminator field. Until then `agent_discriminator_field_invalid`
            // has no producer in IR, but we still report when the bare
            // `output <Record>` references an unknown record so the
            // author gets a fast signal.
            (ir::AgentOutputKind::Text, _) => {
                if let Some(ir::TypeRef::Unresolved(name)) = agent.output_type.as_ref() {
                    // Heuristic: titlecase first letter means it's an
                    // intended record/enum reference (vs `Text`/`Integer`
                    // which match Builtin earlier).
                    let first = name.chars().next();
                    if first.is_some_and(|c| c.is_ascii_uppercase()) {
                        if !any_record(name) && !any_enum(name) {
                            diagnostics.push(DoctorDiagnostic {
                                path: fact.path.clone(),
                                line: fact.line,
                                column: 1,
                                severity: DoctorSeverity::Error,
                                code: "agent_discriminator_target_invalid_diagnostics".to_owned(),
                                message: format!(
                                    "agent `{}` declares `output {}` but no enum or record named `{}` exists in any reachable feature.",
                                    agent.name, name, name,
                                ),
                                category: None,
                                feature_name: None,
                                construct: None,
                                fix: None,
                                group: None,
                            });
                            continue;
                        }
                        // Validate field-level discriminator marker on
                        // records — proposal §A2 requires exactly one
                        // field carrying the marker, and its type must
                        // resolve to an enum.
                        for facts in tier3_facts {
                            if let Some(record) = facts.records.iter().find(|r| r.name == *name) {
                                diagnostics.extend(check_record_discriminator(
                                    fact,
                                    agent,
                                    name,
                                    record,
                                    tier3_facts,
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    diagnostics
}

/// Phase L Tier 4 follow-up — typed `check_record_discriminator` that
/// consumes `ir::Record` directly. The discriminator-marker count
/// comes from `Record.discriminator_field` (typed `Option<String>`);
/// the discriminator field's type is read by name from the record's
/// typed field list. The enum lookup walks `Tier3FeatureFacts.enums`
/// (typed `ir::EnumDecl` lift); the legacy `FeatureSymbols.enums`
/// text walker is gone.
fn check_record_discriminator(
    fact: &AgentFacts,
    agent: &Agent,
    record_name: &str,
    record: &lazuli_ir::Record,
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let Some(field_name) = record.discriminator_field.as_deref() else {
        // No discriminator: it's a legacy `output <Record>` shape, not a
        // DiscriminatedRecord. Cut A's soft-warn for legacy output is
        // emitted in the LSP file-local layer (Phase 4); nothing to do
        // here.
        return Vec::new();
    };

    let mut diagnostics = Vec::new();

    // The IR currently captures only one discriminator field per record
    // (`discriminator_field: Option<String>`), so the "multiple
    // markers" branch from the legacy walker is structurally
    // unreachable; the parser would reject the duplicate before
    // lowering. We preserve the slot for forward compatibility but
    // skip the check.
    let Some(field) = record.fields.iter().find(|f| f.name == field_name) else {
        return diagnostics;
    };
    let type_name = type_ref_name(&field.type_ref);
    let enum_exists = tier3_facts
        .iter()
        .any(|f| f.enums.iter().any(|e| e.name == type_name));
    if !enum_exists {
        diagnostics.push(DoctorDiagnostic {
            path: fact.path.clone(),
            line: fact.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "agent_discriminator_field_invalid_diagnostics".to_owned(),
            message: format!(
                "agent `{}` references record `{}` whose discriminator field `{}` has type `{}`, but no enum by that name exists; the marked field must resolve to an enum.",
                agent.name, record_name, field_name, type_name,
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

/// Phase L Tier 4 follow-up — project a typed `TypeRef` back to its
/// short name for cross-type lookups (used by
/// `check_record_discriminator` to find the matching enum). Many
/// variants don't yield a usable name; callers fall back to the empty
/// string and the enum lookup fails as expected.

// -----------------------------------------------------------------------------
// Diagnostic ids: eval_ordered_op_invalid / eval_nondeterministic_warning
// -----------------------------------------------------------------------------

fn agent_eval_diagnostics(agents: &[AgentFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for fact in agents {
        let agent = &fact.agent;

        if !agent.evals.is_empty() && (agent.temperature != Some(0.0) || agent.seed.is_none()) {
            let reason = if agent.temperature != Some(0.0) {
                "missing `temperature 0`"
            } else {
                "missing `seed <int>`"
            };
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "eval_nondeterministic_warning".to_owned(),
                message: format!(
                    "agent `{}` declares `evals` but the agent is non-deterministic ({}); cases run as informational results until both `temperature 0` and `seed <int>` are pinned.",
                    agent.name, reason,
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        for case in &agent.evals {
            for assertion in &case.assertions {
                if let ir::EvalPredicate::Closed(ir::Predicate::Comparison { left, op, right }) =
                    &assertion.predicate
                {
                    if matches!(
                        op,
                        ir::CompareOp::Lt
                            | ir::CompareOp::Le
                            | ir::CompareOp::Gt
                            | ir::CompareOp::Ge
                    ) && !operand_resolves_numeric(left)
                        && !operand_resolves_numeric(right)
                    {
                        diagnostics.push(DoctorDiagnostic {
                            path: fact.path.clone(),
                            line: fact.line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "eval_ordered_op_invalid_diagnostics".to_owned(),
                            message: format!(
                                "agent `{}` eval case `{}` uses an ordered operator but neither operand resolves to a numeric type; ordered comparisons require numeric refs (`<ref>.length`, `<ref>.count`, integer fields).",
                                agent.name, case.name,
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

/// Best-effort numeric-operand check. The closed-predicate IR doesn't
/// carry resolved types yet, so we accept the canonical numeric paths
/// (`<x>.length`, `<x>.count`) and integer literals. Everything else is
/// rejected as non-numeric — authors who hit a false positive can split
/// the case until type resolution arrives.
fn operand_resolves_numeric(expr: &ir::Expr) -> bool {
    match expr {
        ir::Expr::Integer(_) => true,
        ir::Expr::Path(path) => {
            let last = path.segments.last().map(String::as_str);
            matches!(last, Some("length") | Some("count") | Some("size"))
        }
        _ => false,
    }
}

// -----------------------------------------------------------------------------
// Cut A.7 — `expose http` cross-feature diagnostics
// -----------------------------------------------------------------------------

/// Walk every agent with `expose_http` plus every `api` path
/// declared in source. Reject cross-feature collisions on (normalised
/// path, method) and `audience` references that don't resolve to any
/// known `.lzx` surface or `app.lzi` audience declaration.
fn agent_expose_diagnostics(
    agents: &[AgentFacts],
    tier3_facts: &[Tier3FeatureFacts],
    known_audiences: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Collect every (method, normalized_path) pair from agent expose
    // blocks + every api block, anchored to their source location.
    let mut pairs: Vec<ExposePathFact> = Vec::new();
    for fact in agents {
        let Some(expose) = fact.agent.expose_http.as_ref() else {
            continue;
        };
        pairs.push(ExposePathFact {
            path_normalised: normalise_path(&expose.path),
            path_raw: expose.path.clone(),
            method: http_method_word(expose.method).to_owned(),
            origin: format!("agent {}.{}", fact.feature, fact.agent.name),
            owner_path: fact.path.clone(),
            line: fact.line,
        });
    }
    // Phase L Tier 4b — read `Api` declarations from `Tier3FeatureFacts`
    // (IR), retiring the `ApiPathFact` text-walker.
    for feature in tier3_facts {
        for api in &feature.apis {
            let line = feature
                .api_lines
                .get(&api.name)
                .copied()
                .unwrap_or(feature.feature_line);
            pairs.push(ExposePathFact {
                path_normalised: normalise_path(&api.path),
                path_raw: api.path.clone(),
                method: http_method_word(api.method).to_owned(),
                origin: format!("api {}.{}", feature.feature, api.name),
                owner_path: feature.path.clone(),
                line,
            });
        }
    }

    // Cross-feature path collision detection. Two facts collide when
    // they share (normalized_path, method) but originate from
    // different feature/api ids — same feature/agent collisions are
    // file-local and surface in LSP instead.
    for (i, a) in pairs.iter().enumerate() {
        for b in pairs.iter().skip(i + 1) {
            if a.path_normalised == b.path_normalised
                && a.method == b.method
                && a.origin != b.origin
            {
                diagnostics.push(DoctorDiagnostic {
                    path: a.owner_path.clone(),
                    line: a.line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "agent_expose_path_conflict_cross_feature_diagnostics".to_owned(),
                    message: format!(
                        "{origin_a} declares HTTP path `{path}` ({method}) that conflicts with {origin_b}; same method+path must originate from a single feature.",
                        origin_a = a.origin,
                        origin_b = b.origin,
                        path = a.path_raw,
                        method = a.method,
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

    // Audience reachability check.
    for fact in agents {
        let Some(expose) = fact.agent.expose_http.as_ref() else {
            continue;
        };
        let Some(audience) = expose.audience.as_ref() else {
            continue;
        };
        if !known_audiences.contains(audience) {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "agent_expose_audience_unknown_diagnostics".to_owned(),
                message: format!(
                    "agent `{}` declares `expose http audience {audience}`, but no `.lzx` surface or `app.lzi` audience declares it.",
                    fact.agent.name,
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

#[derive(Debug, Clone)]
struct ExposePathFact {
    path_normalised: String,
    path_raw: String,
    method: String,
    origin: String,
    owner_path: PathBuf,
    line: usize,
}

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
fn collect_known_audiences(files: &[DoctorFile]) -> BTreeSet<String> {
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

fn app_urls_missing_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
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

fn collect_approval_block_presence(file: &DoctorFile, out: &mut Vec<ApprovalBlockPresence>) {
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
fn approval_missing_children_diagnostics(
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
fn approval_diagnostics(
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
fn scope_owner_column_diagnostics(tier3_facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
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
fn field_derived_from_unresolved_diagnostics(
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
fn line_col_for_offset_in_file(path: &Path, offset: usize) -> (usize, usize) {
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
fn collect_unresolved_field_refs(expr: &str, siblings: &BTreeSet<&str>) -> Vec<String> {
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
fn resource_unique_qualifier_unknown_diagnostics(
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
fn resource_validates_path_unknown_diagnostics(
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
fn approval_timeout_well_formed(text: &str) -> bool {
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
fn collect_known_roles(files: &[DoctorFile]) -> BTreeSet<String> {
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

fn extract_role_atoms(refs: &str, roles: &mut BTreeSet<String>) {
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
fn collect_package_rbac_catalog(
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
fn rbac_catalog_diagnostics(
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
fn line_col_for_offset_from_files(
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
fn rbac_role_undeclared_diagnostics(
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
fn rbac_catalog_missing_diagnostics(
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
fn rbac_missing_policy_diagnostics(files: &[DoctorFile]) -> Vec<DoctorDiagnostic> {
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

fn flush_missing_policy(
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
fn collect_auth_anchors(source: &str, auth_line: usize) -> AuthAnchors {
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

/// Phase L Tier 3 — best-effort line lookup for `job` / `webhook` /
/// `notification` headers inside a feature. Walks the file looking for
/// lines whose trimmed text starts with `<prefix><name>` (e.g.
/// `job process_import`). Returns a `name -> 1-based line` map.
fn collect_construct_lines(
    source: &str,
    prefix: &str,
    names: BTreeSet<&str>,
) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    if names.is_empty() {
        return out;
    }
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(prefix) else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("");
        if names.contains(name) {
            out.entry(name.to_owned()).or_insert(idx + 1);
        }
    }
    out
}

/// OpenAPI/Cache bucket cycles — line lookup for `query.list`,
/// `query.lookup`, `query.sql`, `query.view` headers. Mirrors `collect_construct_lines`
/// but the parser folds the kind into the header keyword.
fn collect_query_lines(source: &str, queries: &[lazuli_ir::Query]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    if queries.is_empty() {
        return out;
    }
    let names: BTreeSet<&str> = queries
        .iter()
        .map(|q| match q {
            lazuli_ir::Query::List(l) => l.name.as_str(),
            lazuli_ir::Query::Lookup(l) => l.name.as_str(),
            lazuli_ir::Query::Sql(s) => s.name.as_str(),
        })
        .collect();
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let after = trimmed
            .strip_prefix("query.list ")
            .or_else(|| trimmed.strip_prefix("query.lookup "))
            .or_else(|| trimmed.strip_prefix("query.sql "))
            .or_else(|| trimmed.strip_prefix("query.view "));
        let Some(rest) = after else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("");
        if names.contains(name) {
            out.entry(name.to_owned()).or_insert(idx + 1);
        }
    }
    out
}

/// OpenAPI bucket cycle — collect every `api <name>` declaration in
/// the source by text-pattern. The doctor diagnostic then subtracts
/// names that were lifted to `feature.apis` IR to know which entries
/// are still text-pattern (i.e. the OpenAPI emitter falls back to a
/// stub for them).
fn collect_text_pattern_api_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("api ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
    }
    out
}

/// i18n bucket cycle — find the first line containing the bare keyword
/// (preceded only by whitespace). Used for `translation` line anchoring.
fn find_keyword_line(source: &str, keyword: &str) -> Option<usize> {
    for (idx, line) in source.lines().enumerate() {
        if line.trim() == keyword {
            return Some(idx + 1);
        }
    }
    None
}

/// Phase L Tier 3 — line lookup for `event_group <pattern>` headers.
/// Same as `collect_construct_lines` but matches the pattern token.
fn collect_event_group_lines(source: &str, patterns: BTreeSet<&str>) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    if patterns.is_empty() {
        return out;
    }
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("event_group ") else {
            continue;
        };
        let pattern = rest.split_whitespace().next().unwrap_or("");
        if patterns.contains(pattern) {
            out.entry(pattern.to_owned()).or_insert(idx + 1);
        }
    }
    out
}

/// Phase L Tier 3 — derive the feature's tenancy axis from the lifted
/// IR `Defaults` block. Returns the axis name (`org`, `team`, custom)
/// or `None` when the feature declares `tenancy none` / inherits.
///
/// Phase L Tier 4a — `parse_feature_skeletons` now lifts
/// `defaults.tenancy`; this is a typed read of
/// `feature.defaults.tenancy`. The legacy "axis unknown → only check
/// presence" fallback that tier 3 diagnostics rode on is retired.
fn tenancy_axis_for(feature: &lazuli_ir::Feature) -> Option<String> {
    match feature.defaults.tenancy.as_ref()? {
        lazuli_ir::Tenancy::Org => Some("org".to_owned()),
        lazuli_ir::Tenancy::Team => Some("team".to_owned()),
        lazuli_ir::Tenancy::Custom(name) => Some(name.clone()),
        // `tenancy none` is an explicit opt-out — there is no axis to
        // cross-check against.
        lazuli_ir::Tenancy::None => None,
    }
}

/// Phase L Tier 4 follow-up — IR-driven replacement for the retired
/// `collect_feature_resources` text-walker. Reads typed `Feature.resources`
/// and projects each `Resource` + its fields into the `ResourceFact` /
/// `ResourceFieldFact` shape consumed by `auth_diagnostics`. The
/// resource line anchor comes from `collect_construct_lines` on the
/// file source so cross-feature anchored diagnostics still point at the
/// `resource <Name>` header.
fn populate_feature_resources_from_ir(
    file_path: &Path,
    file_source: &str,
    feature: &lazuli_ir::Feature,
    out: &mut BTreeMap<String, BTreeMap<String, ResourceFact>>,
) {
    if feature.resources.is_empty() {
        return;
    }
    let resource_lines = collect_construct_lines(
        file_source,
        "resource ",
        feature.resources.iter().map(|r| r.name.as_str()).collect(),
    );
    let entry = out.entry(feature.name.clone()).or_default();
    for resource in &feature.resources {
        let line = resource_lines.get(&resource.name).copied().unwrap_or(0);
        let mut fields = BTreeMap::new();
        for field in &resource.fields {
            let field_line = field
                .span_ref
                .map(|span| line_col_for_offset(file_source, span.start).0)
                .unwrap_or(line);
            fields.insert(
                field.name.clone(),
                ResourceFieldFact {
                    type_ref: field.type_ref.clone(),
                    unique: field.unique,
                    line: field_line,
                },
            );
        }
        entry.insert(
            resource.name.clone(),
            ResourceFact {
                path: file_path.to_path_buf(),
                line,
                fields,
            },
        );
    }
}

/// Harvest each feature's `extensions adapter <name>: <Type> at "..."`
/// declarations. Only the local name is stored; the type contract is
/// checked elsewhere.
fn collect_feature_adapters(file: &DoctorFile, out: &mut BTreeMap<String, BTreeSet<String>>) {
    if !is_lzi_path(&file.path) {
        return;
    }
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
        if leading_spaces(line) == 2 && trimmed == "extensions" {
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
                if let Some(rest) = inner_trim.strip_prefix("adapter ") {
                    let name_segment = rest.split([':', ' ']).next().unwrap_or("").trim();
                    if !name_segment.is_empty() {
                        if let Some(feature_name) = feature.as_ref() {
                            out.entry(feature_name.clone())
                                .or_default()
                                .insert(name_segment.to_owned());
                        }
                    }
                }
                j += 1;
            }
            i = j;
            continue;
        }
        i += 1;
    }
}

/// Harvest each feature's `uses <feature>, <feature>, ...` declarations.
/// Cross-feature resource resolution (e.g. `auth identity Customer.email`
/// in `customer_auth uses customer`) reads this map.
fn collect_feature_uses(file: &DoctorFile, out: &mut BTreeMap<String, BTreeSet<String>>) {
    if !is_lzi_path(&file.path) {
        return;
    }
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
        if leading_spaces(line) == 2 && trimmed.starts_with("uses ") {
            if let Some(rest) = trimmed.strip_prefix("uses ") {
                if let Some(feature_name) = feature.as_ref() {
                    // Cross-feature contracts §5.4 — strip the optional
                    // trailing `version v<N>` pin BEFORE comma-splitting.
                    // The pin applies to all entries on the line, but the
                    // legacy uses-map only tracks feature names.
                    let list_part = match rest.find(" version ") {
                        Some(idx) => &rest[..idx],
                        None => rest,
                    };
                    let entry = out.entry(feature_name.clone()).or_default();
                    for token in list_part.split(',') {
                        let name = token.trim();
                        if !name.is_empty() {
                            entry.insert(name.to_owned());
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

/// Phase L Tier 4 follow-up — read the `algorithm:<X>` axis out of a
/// typed `CapabilityRef::Hashed(...)`. Returns `None` when the field is
/// not a `@cap.Hashed` decorator. Replaces the text-walking version
/// that re-parsed `@cap.Hashed(algorithm:…)` from `type_text`.
fn cap_hashed_algorithm(type_ref: &lazuli_ir::TypeRef) -> Option<&'static str> {
    match type_ref {
        lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::Hashed(h)) => {
            Some(match h.algorithm {
                lazuli_ir::HashAlgorithm::Argon2id => "argon2id",
                lazuli_ir::HashAlgorithm::Bcrypt => "bcrypt",
            })
        }
        _ => None,
    }
}

/// Phase L Tier 4 follow-up — typed `is_identity_shaped`. Identity
/// fields are either tagged `@semantic.Email` / `@semantic.Phone`,
/// declared as `ID`, or carry the typed `unique` axis. Rejects free-
/// form `Text` fields used as login identities.
fn is_identity_shaped(field: &ResourceFieldFact) -> bool {
    use lazuli_ir::{BuiltinType, TypeRef};
    match &field.type_ref {
        TypeRef::Builtin(BuiltinType::SemanticEmail | BuiltinType::SemanticPhone) => true,
        TypeRef::Builtin(BuiltinType::Id) => true,
        _ => field.unique,
    }
}

/// Resolve `<Resource>` for a feature by searching its own resources
/// first, then falling back to resources declared in features it
/// `uses`. Returns the first hit.
fn resolve_resource_for_feature<'a>(
    feature: &str,
    resource_name: &str,
    feature_resources: &'a BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
) -> Option<&'a ResourceFact> {
    if let Some(local) = feature_resources
        .get(feature)
        .and_then(|m| m.get(resource_name))
    {
        return Some(local);
    }
    if let Some(uses) = feature_uses.get(feature) {
        for dep in uses {
            if let Some(hit) = feature_resources
                .get(dep)
                .and_then(|m| m.get(resource_name))
            {
                return Some(hit);
            }
        }
    }
    None
}

/// Emit the four `auth_*` cross-feature diagnostics. Each diagnostic
/// is anchored at the offending subblock line; the `auth` header is
/// only used as a fallback.
fn auth_diagnostics(
    auth_facts: &[AuthFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
    feature_adapters: &BTreeMap<String, BTreeSet<String>>,
    feature_uses: &BTreeMap<String, BTreeSet<String>>,
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let registry_integrations: BTreeSet<String> = registry
        .map(|r| {
            r.manifest
                .integrations
                .iter()
                .map(|i| i.name.clone())
                .collect()
        })
        .unwrap_or_default();

    for fact in auth_facts {
        let feature = fact.feature.as_str();

        // 1. `auth_identity_field_unknown` — resource and field must
        //    resolve in the same feature (or one it `uses`), and the
        //    field must be identity-shaped.
        let identity_resource = fact.auth.identity.field.resource.name.as_str();
        let identity_field = fact.auth.identity.field.field.as_str();
        let identity_resource_fact = resolve_resource_for_feature(
            feature,
            identity_resource,
            feature_resources,
            feature_uses,
        );
        match identity_resource_fact {
            None => diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.identity_line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "auth_identity_field_unknown".to_owned(),
                message: format!(
                    "auth.identity `{identity_resource}.{identity_field}` does not resolve: resource not found in feature `{feature}`.",
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
            Some(resource) => match resource.fields.get(identity_field) {
                None => diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.identity_line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_identity_field_unknown".to_owned(),
                    message: format!(
                        "auth.identity `{identity_resource}.{identity_field}` does not resolve: field not found on `{identity_resource}`.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }),
                Some(field) => {
                    if !is_identity_shaped(field) {
                        diagnostics.push(DoctorDiagnostic {
                            path: fact.path.clone(),
                            line: fact.identity_line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "auth_identity_field_unknown".to_owned(),
                            message: format!(
                                "auth.identity `{identity_resource}.{identity_field}` does not resolve: field is not identity-shaped (missing @semantic.Email / @semantic.Phone / unique).",
                            ),
                            category: None,
                            feature_name: None,
                            construct: None,
                            fix: None,
                            group: None,
                        });
                    }
                }
            },
        }

        // 2. `auth_password_no_session` — password login without an
        //    `auth.sessions` block can validate credentials but cannot
        //    issue durable sessions.
        if fact.auth.password.is_some() && fact.auth.sessions.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line: fact.password_line.unwrap_or(fact.line),
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "auth_password_no_session".to_owned(),
                message:
                    "auth.password is declared but auth.sessions is missing; login will not issue sessions."
                        .to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // 3. `auth_oauth_no_password_alt` — OAuth-only signin is a
        //    valid contract, but many apps want password fallback for
        //    break-glass administration.
        if !fact.auth.oauth.is_empty() && fact.auth.password.is_none() {
            let line = fact
                .auth
                .oauth
                .first()
                .and_then(|provider| fact.oauth_lines.get(provider.provider.as_str()).copied())
                .unwrap_or(fact.line);
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Info,
                code: "auth_oauth_no_password_alt".to_owned(),
                message:
                    "auth.oauth is declared without auth.password; signin is OAuth-only with no password fallback for break-glass access."
                        .to_owned(),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }

        // 4. `auth_sessions_resource_unknown` — sessions resource must
        //    resolve in the same feature (or one it `uses`).
        if let Some(sessions) = fact.auth.sessions.as_ref() {
            let sessions_name = sessions.resource.name.as_str();
            let resolved = resolve_resource_for_feature(
                feature,
                sessions_name,
                feature_resources,
                feature_uses,
            );
            if resolved.is_none() {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.sessions_resource_line.unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_sessions_resource_unknown".to_owned(),
                    message: format!(
                        "auth.sessions.resource `{sessions_name}` does not name a resource declared in feature `{feature}`.",
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            if auth_session_ttl_seconds(&sessions.ttl)
                .map(|seconds| seconds < 60 * 60)
                .unwrap_or(false)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.sessions_line.unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "auth_session_ttl_too_short".to_owned(),
                    message: "session TTL <1h forces frequent re-login; ensure intentional."
                        .to_owned(),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }

            // AUTH-SESSION-TENANT-001 — every extra column must map to
            // `lazuli.ID`; non-ID Go types cannot be tenant-pinned by the
            // v1 shim.
            for col in &sessions.extra_columns {
                if col.go_type != "lazuli.ID" {
                    diagnostics.push(DoctorDiagnostic {
                        path: fact.path.clone(),
                        line: fact.sessions_resource_line.unwrap_or(fact.line),
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "AUTH-SESSION-TENANT-001".to_owned(),
                        message: format!(
                            "session resource `{sessions_name}` extra column `{}` has Go type `{}` but only `lazuli.ID` is allowed; declare the field as a resource reference.",
                            col.field_name, col.go_type,
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }

            // AUTH-SESSION-EXTRA-001 — more than one extra column means
            // the generated shim has positional parameters whose order
            // matches DSL declaration; reordering silently changes tenant
            // scope.
            if sessions.extra_columns.len() > 1 {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact.sessions_resource_line.unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "AUTH-SESSION-EXTRA-001".to_owned(),
                    message: format!(
                        "session resource `{sessions_name}` declares {} extra columns; v1 emits them positionally in DSL order — reordering silently changes tenant scope. Reduce to at most 1, or verify caller argument order carefully.",
                        sessions.extra_columns.len(),
                    ),
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                });
            }
        }

        // 5. `auth_password_algorithm_hash_mismatch` — when both
        //    `auth.password.algorithm` and the session resource carry
        //    a `@cap.Hashed(algorithm:…)` field, the two axes must
        //    match.
        if let (Some(password), Some(sessions)) =
            (fact.auth.password.as_ref(), fact.auth.sessions.as_ref())
        {
            let pw_algo = password.algorithm.trim();
            if !pw_algo.is_empty() {
                let sessions_name = sessions.resource.name.as_str();
                if let Some(resource) = resolve_resource_for_feature(
                    feature,
                    sessions_name,
                    feature_resources,
                    feature_uses,
                ) {
                    // Find the first hash-shaped field on the session
                    // resource that carries a `@cap.Hashed(...)`
                    // decorator. Multiple is allowed; we pin the
                    // first divergence.
                    let mut found_hash_axis = None;
                    for (field_name, field) in &resource.fields {
                        if let Some(axis) = cap_hashed_algorithm(&field.type_ref) {
                            found_hash_axis = Some((field_name.clone(), axis.to_owned()));
                            if axis != pw_algo {
                                diagnostics.push(DoctorDiagnostic {
                                    path: fact.path.clone(),
                                    line: fact
                                        .password_algorithm_line
                                        .unwrap_or(fact.password_line.unwrap_or(fact.line)),
                                    column: 1,
                                    severity: DoctorSeverity::Error,
                                    code: "auth_password_algorithm_hash_mismatch".to_owned(),
                                    message: format!(
                                        "auth.password.algorithm `{pw_algo}` must match `@cap.Hashed(algorithm:{pw_algo})` on the session resource's hash field (found `{axis}` on `{sessions_name}.{field_name}`).",
                                        pw_algo = pw_algo,
                                        axis = axis,
                                        sessions_name = sessions_name,
                                        field_name = field_name,
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
                    let _ = found_hash_axis;
                }
            }
        }

        // 6. `auth_oauth_adapter_unbound` — each oauth provider's
        //    adapter must resolve in the feature's `extensions
        //    adapter <name>` list or `registry.integrations`.
        let feature_adapter_names = feature_adapters.get(feature);
        for provider in &fact.auth.oauth {
            let adapter_ref = provider.adapter.as_str();
            let local_name = adapter_ref.strip_prefix("@adapter.").unwrap_or("");
            let in_feature = !local_name.is_empty()
                && feature_adapter_names
                    .map(|s| s.contains(local_name))
                    .unwrap_or(false);
            let in_registry = !local_name.is_empty() && registry_integrations.contains(local_name);
            if !in_feature && !in_registry {
                diagnostics.push(DoctorDiagnostic {
                    path: fact.path.clone(),
                    line: fact
                        .oauth_lines
                        .get(provider.provider.as_str())
                        .copied()
                        .unwrap_or(fact.line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "auth_oauth_adapter_unbound".to_owned(),
                    message: format!(
                        "auth.oauth.`{provider}`.adapter `{adapter_ref}` is not declared in `extensions` of feature `{feature}` or `integrations` in `registry.lzi`.",
                        provider = provider.provider,
                        adapter_ref = adapter_ref,
                        feature = feature,
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


// -----------------------------------------------------------------------------
// Cut A.8 — built-in trace event diagnostics
//
// `agent_run` is registered by the IR as a built-in trace event. The
// language reserves the name (authored `event.trace agent_run` is
// rejected) and validates subscriber jobs against the canonical
// payload schema so a job referencing a non-existent field doesn't
// fail silently at runtime.
//
// See `docs/proposals/ai-primitives-cut-a-8.md`.
// -----------------------------------------------------------------------------

fn agent_run_trace_diagnostics(files: &[DoctorFile]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let canonical_payload: BTreeSet<String> = ir::built_in_trace_events()
        .into_iter()
        .find(|e| e.name == "agent_run")
        .map(|e| e.payload.iter().map(|f| f.name.clone()).collect())
        .unwrap_or_default();

    // Observability bucket cycle row 35 — pre-compute the set of
    // built-in trace event names once per check so `trigger
    // @trace.<X>` and `trigger event.trace <X>` resolution both
    // consult the same registry. Authored `event.trace <name>`
    // declarations in scope are gathered per file below.
    let built_in_names: BTreeSet<String> = ir::built_in_trace_events()
        .into_iter()
        .map(|e| e.name)
        .collect();

    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();

        // Observability bucket cycle row 35 — collect authored
        // `event.trace <name>` declarations *in this file* so
        // `trigger_trace_unknown` doesn't false-positive on
        // legitimate subscriber references to authored events.
        let authored_trace_names: BTreeSet<String> = lines
            .iter()
            .filter_map(|line| {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    return None;
                }
                trimmed
                    .strip_prefix("event.trace ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned)
            })
            .collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                i += 1;
                continue;
            }

            // Reserved-name: `event.trace <name>` where <name> is a
            // built-in. Reject the authored declaration.
            if let Some(rest) = trimmed.strip_prefix("event.trace ") {
                let name = rest.split_whitespace().next().unwrap_or("");
                if ir::is_reserved_trace_event_name(name) {
                    diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading_spaces(line) + 1,
                        severity: DoctorSeverity::Error,
                        code: "event_trace_reserved_name_diagnostics".to_owned(),
                        message: format!(
                            "`event.trace {name}` is reserved by the IR as a built-in trace event; the runtime emits it automatically. Authoring this declaration is rejected — remove the block and subscribe via `job ... trigger event.trace {name}` instead."
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }

            // Payload-drift: `trigger event.trace agent_run` inside a
            // job, followed by `payload <field>` references at deeper
            // indent. Reject any field not in the canonical schema.
            if trimmed.starts_with("trigger event.trace ") {
                let name = trimmed
                    .strip_prefix("trigger event.trace ")
                    .map(|n| n.trim().to_owned())
                    .unwrap_or_default();
                if canonical_payload_event(&name, &canonical_payload) {
                    diagnostics.extend(scan_payload_field_drift(
                        file,
                        &lines,
                        i,
                        &name,
                        &canonical_payload,
                    ));
                }
            }

            // Observability bucket cycle row 35 — `trigger_trace_unknown`.
            // The `@trace.<name>` namespace and the bare-form
            // `trigger event.trace <name>` both have to resolve to a
            // built-in trace event or an authored `event.trace <name>`
            // in the same file. We catch the failure here so a typo
            // doesn't fall through to runtime as "subscriber wired to
            // an event that nobody emits."
            let trace_ref = trimmed
                .strip_prefix("trigger @trace.")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
                .or_else(|| {
                    trimmed
                        .strip_prefix("trigger event.trace ")
                        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
                });
            if let Some(name) = trace_ref {
                if !name.is_empty()
                    && !built_in_names.contains(&name)
                    && !authored_trace_names.contains(&name)
                {
                    let mut known: Vec<String> = built_in_names.iter().cloned().collect();
                    known.extend(authored_trace_names.iter().cloned());
                    diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading_spaces(line) + 1,
                        severity: DoctorSeverity::Error,
                        code: "trigger_trace_unknown_diagnostics".to_owned(),
                        message: format!(
                            "`trigger @trace.{name}` does not resolve. Built-in trace events: {}. Authored trace events in scope: {}.",
                            format_name_list(&built_in_names),
                            format_name_list(&authored_trace_names),
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }
            }

            i += 1;
        }
    }

    diagnostics
}

// =============================================================================
// Observability bucket cycle row 37 — audit `emit_to` + `event.trace level`
//                                   + health probe path checks
//
// Four diagnostics:
//   - `audit_emit_to_unknown_diagnostics`             error
//   - `event_trace_level_invalid_diagnostics`         error
//   - `event_trace_level_on_domain_event_diagnostics` error
//   - `health_probe_path_invalid_diagnostics`         error
//
// `audit emit_to` resolution:
//   - Reserved streams `audit_log` / `audit_stream` always resolve.
//   - An authored `event_group <name>` in the same feature resolves.
//   - Otherwise, doctor emits `audit_emit_to_unknown_diagnostics`.
//
// `event.trace <name> level <X>`:
//   - Closed catalog `debug/info/warn/error` (shared with row 36).
//   - Per the proposal §3.4, `level` is only valid on `event.trace`.
//     A `level` slot under a domain `event` block is rejected (different
//     diagnostic code so the author sees the right fix).
//
// Health probe paths come from `app.runtime <unit>.{healthcheck,readiness}`.
// Doctor only validates the *shape* of the path string (`/foo`); the
// runtime decides which mux to mount onto. Empty or missing-leading-slash
// paths are rejected.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.3 §3.4 §Runtime.
// =============================================================================

// Mirrors `log/slog` level discipline; kept in sync with
// `aggregators::observability::LOG_LEVEL_CATALOG`.
const TRACE_LEVEL_CATALOG: &[&str] = &["debug", "info", "warn", "error"];
const RESERVED_AUDIT_STREAMS: &[&str] = &["audit_log", "audit_stream"];

/// Phase L Tier 4b — find the `emit_to <target>` line inside the body
/// of a construct whose header is at `header_line` (1-indexed). Returns
/// `(line_1_indexed, column_1_indexed)`. Used by the IR-driven
/// `audit emit_to` walker to anchor diagnostics at the exact source
/// location even when the IR side only carries the construct header.
fn locate_emit_to_line(
    path: &Path,
    files: &[DoctorFile],
    header_line: usize,
    target: &str,
) -> Option<(usize, usize)> {
    let file = files.iter().find(|f| f.path == path)?;
    let lines: Vec<&str> = file.source.lines().collect();
    if header_line == 0 || header_line > lines.len() {
        return None;
    }
    let header_indent = leading_spaces(lines[header_line - 1]);
    let needle = format!("emit_to {target}");
    for (offset, line) in lines.iter().enumerate().skip(header_line) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        // Stop at sibling or higher-level construct.
        if indent <= header_indent {
            return None;
        }
        if trimmed == needle || trimmed.starts_with(&needle) {
            return Some((offset + 1, indent + 1));
        }
    }
    None
}

fn audit_event_health_diagnostics(
    files: &[DoctorFile],
    app: Option<&DoctorAppManifest>,
    tier3_facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Phase L Tier 4b — build the feature → event-group lookup from
    // both IR (`tier3_facts`) and text-walk (for features that don't
    // lower through the canonical-indent slice). IR takes precedence
    // when a feature appears in both.
    let mut feature_event_groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for fact in tier3_facts {
        let entry = feature_event_groups
            .entry(fact.feature.clone())
            .or_default();
        for group in &fact.event_groups {
            // `EventGroup.pattern` is the whole `<name> *` or `<glob>`
            // pattern as authored. `emit_to` references the first
            // whitespace token (the group's name), matching the
            // historical text-walker behaviour.
            if let Some(name) = group.pattern.split_whitespace().next() {
                entry.insert(name.to_owned());
            }
        }
    }
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let mut current_feature: Option<String> = None;
        for line in file.source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let leading = leading_spaces(line);
            if leading == 0 {
                current_feature = trimmed
                    .strip_prefix("feature ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
                continue;
            }
            if let Some(feature) = current_feature.as_ref() {
                if let Some(rest) = trimmed.strip_prefix("event_group ") {
                    if let Some(name) = rest.split_whitespace().next() {
                        feature_event_groups
                            .entry(feature.clone())
                            .or_default()
                            .insert(name.to_owned());
                    }
                }
            }
        }
    }

    // Phase L Tier 4b — IR-driven `audit emit_to` resolution for
    // commands. Walks `Command.audit.emit_to` directly; anchors the
    // diagnostic at the `emit_to <target>` line inside the command
    // body by scanning the source range starting at the command
    // header. Retires the text-walker branch for command bodies.
    let mut command_audit_keys: BTreeSet<(PathBuf, usize)> = BTreeSet::new();
    for fact in tier3_facts {
        for command in &fact.commands {
            let Some(audit) = command.audit.as_ref() else {
                continue;
            };
            let Some(target) = audit.emit_to.as_deref() else {
                continue;
            };
            let allowed_set = feature_event_groups.get(&fact.feature);
            let resolved = RESERVED_AUDIT_STREAMS.contains(&target)
                || allowed_set.is_some_and(|set| set.contains(target));
            let Some(header_line) = fact.command_lines.get(&command.name).copied() else {
                continue;
            };
            let Some((line, column)) = locate_emit_to_line(&fact.path, files, header_line, target)
            else {
                continue;
            };
            command_audit_keys.insert((fact.path.clone(), line));
            if resolved {
                continue;
            }
            let mut allowed: Vec<String> = RESERVED_AUDIT_STREAMS
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
            if let Some(set) = allowed_set {
                allowed.extend(set.iter().cloned());
            }
            diagnostics.push(DoctorDiagnostic {
                path: fact.path.clone(),
                line,
                column,
                severity: DoctorSeverity::Error,
                code: "audit_emit_to_unknown_diagnostics".to_owned(),
                message: format!(
                    "`audit emit_to {target}` does not resolve. Allowed: {}.",
                    allowed
                        .iter()
                        .map(|s| format!("`{s}`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    // `audit ... emit_to <X>` text-walker for constructs whose IR
    // does not yet carry `audit` (webhook, job, poller, lifecycle
    // transition). Command bodies are skipped — the IR walker above
    // owns them. Detected duplicates against `command_audit_keys`
    // are suppressed defensively.
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut current_feature: Option<String> = None;
        let mut audit_pending: Option<(usize, usize)> = None; // (line_index, indent of audit)
        let mut in_command: Option<usize> = None; // indent of `command <name>` header
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let leading = leading_spaces(line);
            if leading == 0 {
                current_feature = trimmed
                    .strip_prefix("feature ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(str::to_owned);
                audit_pending = None;
                in_command = None;
                continue;
            }
            // Track `command <name>` headers so we can skip their
            // bodies — the IR walker handles command audit emit_to.
            if let Some(command_indent) = in_command {
                if leading <= command_indent {
                    in_command = None;
                }
            }
            if trimmed.starts_with("command ") {
                in_command = Some(leading);
                audit_pending = None;
                continue;
            }
            if in_command.is_some() {
                continue;
            }
            // Track audit headers as `audit <fields...>` or bare `audit`
            // at indent 4 or 6 (webhook/job/poller bodies).
            if trimmed == "audit" || trimmed.starts_with("audit ") {
                audit_pending = Some((i, leading));
                continue;
            }
            if let Some((_, audit_indent)) = audit_pending {
                if leading <= audit_indent {
                    audit_pending = None;
                } else if leading == audit_indent + 2 {
                    if let Some(rest) = trimmed.strip_prefix("emit_to ") {
                        let target = rest.trim();
                        let resolved = if RESERVED_AUDIT_STREAMS.contains(&target) {
                            true
                        } else if let Some(feature) = current_feature.as_ref() {
                            feature_event_groups
                                .get(feature)
                                .is_some_and(|set| set.contains(target))
                        } else {
                            false
                        };
                        if !resolved && !command_audit_keys.contains(&(file.path.clone(), i + 1)) {
                            let mut allowed: Vec<String> = RESERVED_AUDIT_STREAMS
                                .iter()
                                .map(|s| (*s).to_owned())
                                .collect();
                            if let Some(feature) = current_feature.as_ref() {
                                if let Some(set) = feature_event_groups.get(feature) {
                                    allowed.extend(set.iter().cloned());
                                }
                            }
                            diagnostics.push(DoctorDiagnostic {
                                path: file.path.clone(),
                                line: i + 1,
                                column: leading + 1,
                                severity: DoctorSeverity::Error,
                                code: "audit_emit_to_unknown_diagnostics".to_owned(),
                                message: format!(
                                    "`audit emit_to {target}` does not resolve. Allowed: {}.",
                                    allowed
                                        .iter()
                                        .map(|s| format!("`{s}`"))
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                ),
                                category: None,
                                feature_name: None,
                                construct: None,
                                fix: None,
                                group: None,
                            });
                        }
                        audit_pending = None;
                    }
                }
            }
        }
    }

    // `event.trace <name> level <X>` + domain-event `level` rejection.
    // Both are text-walked because the canonical-indent slice does not
    // yet lower events (Phase L row 24).
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut pending_event: Option<(usize, bool, usize)> = None; // (start_line, is_trace, indent)
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            let leading = leading_spaces(line);
            if let Some(rest) = trimmed.strip_prefix("event.trace ") {
                let _ = rest;
                pending_event = Some((i, true, leading));
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("event ") {
                let _ = rest;
                pending_event = Some((i, false, leading));
                continue;
            }
            if let Some((_, is_trace, event_indent)) = pending_event {
                if leading <= event_indent {
                    pending_event = None;
                } else if let Some(level_rest) = trimmed.strip_prefix("level ") {
                    let level = level_rest.trim();
                    if is_trace {
                        if !TRACE_LEVEL_CATALOG.contains(&level) {
                            diagnostics.push(DoctorDiagnostic {
                                path: file.path.clone(),
                                line: i + 1,
                                column: leading + 1,
                                severity: DoctorSeverity::Error,
                                code: "event_trace_level_invalid_diagnostics".to_owned(),
                                message: format!(
                                    "`event.trace ... level {level}` is not in the closed catalog. Allowed values: {}.",
                                    catalog_list(TRACE_LEVEL_CATALOG),
                                ),
                                category: None,
                                feature_name: None,
                                construct: None,
                                fix: None,
                                group: None,
                            });
                        }
                    } else {
                        diagnostics.push(DoctorDiagnostic {
                            path: file.path.clone(),
                            line: i + 1,
                            column: leading + 1,
                            severity: DoctorSeverity::Error,
                            code: "event_trace_level_on_domain_event_diagnostics".to_owned(),
                            message: "`level` is only valid on `event.trace`, not on a domain `event`. Move the slot to a `event.trace` block or remove the `level` line.".to_owned(),
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

    // Health probe paths from `app.runtime <unit>.{healthcheck,readiness}`.
    // We trust the parser (`parse_app_manifest`) to populate the IR;
    // doctor only validates shape ("/foo") here.
    if let Some(manifest) = app {
        for unit in &manifest.manifest.runtime {
            for (slot, value) in [
                ("healthcheck", unit.healthcheck.as_deref()),
                ("readiness", unit.readiness.as_deref()),
            ] {
                let Some(path) = value else {
                    continue;
                };
                if !path.starts_with('/') || path.contains(char::is_whitespace) {
                    diagnostics.push(DoctorDiagnostic {
                        path: manifest.path.clone(),
                        line: 1,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "health_probe_path_invalid_diagnostics".to_owned(),
                        message: format!(
                            "`app.runtime unit {unit_name} {slot} {path:?}` must be a path starting with `/` and containing no whitespace.",
                            unit_name = unit.name,
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

fn resource_policy_and_command_audit_hints(
    facts: &[Tier3FeatureFacts],
    feature_resources: &BTreeMap<String, BTreeMap<String, ResourceFact>>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen_commands: BTreeSet<(PathBuf, String, String)> = BTreeSet::new();
    let mut seen_resources: BTreeSet<(PathBuf, String, String)> = BTreeSet::new();

    for feature in facts {
        let mut referenced_write_resources = BTreeSet::new();
        for command in &feature.commands {
            if is_write_effect_command(command) {
                if command.audit.is_none()
                    && seen_commands.insert((
                        feature.path.clone(),
                        feature.feature.clone(),
                        command.name.clone(),
                    ))
                {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: feature
                            .command_lines
                            .get(&command.name)
                            .copied()
                            .unwrap_or(feature.feature_line),
                        column: 1,
                        severity: DoctorSeverity::Hint,
                        code: "command_without_audit_hint".to_owned(),
                        message: format!(
                            "command `{}.{}` is write-effect but has no `audit default` declared ÔÇö write actions without audit are invisible to compliance. Add `audit default` on the command or `audit_default` in feature defaults.",
                            feature.feature, command.name
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                }

                if let Some(resource) = write_effect_resource(command) {
                    let is_local_resource = match resource.feature.as_deref() {
                        Some(owner) => owner == feature.feature,
                        None => true,
                    };
                    if is_local_resource {
                        referenced_write_resources.insert(resource.name.clone());
                    }
                }
            }
        }

        if feature.policies_declared || referenced_write_resources.is_empty() {
            continue;
        }

        let Some(resources) = feature_resources.get(&feature.feature) else {
            continue;
        };
        for resource in referenced_write_resources {
            let Some(resource_fact) = resources.get(&resource) else {
                continue;
            };
            if !seen_resources.insert((
                resource_fact.path.clone(),
                feature.feature.clone(),
                resource.clone(),
            )) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                path: resource_fact.path.clone(),
                line: if resource_fact.line == 0 {
                    feature.feature_line
                } else {
                    resource_fact.line
                },
                column: 1,
                severity: DoctorSeverity::Hint,
                code: "resource_without_policy_hint".to_owned(),
                message: format!(
                    "feature `{}` declares resource `{}` with no `policies` block ÔÇö every write command implicitly gets the default policy. Add an explicit `policies` block to make access control auditable.",
                    feature.feature, resource
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

fn is_write_effect_command(command: &lazuli_ir::Command) -> bool {
    matches!(
        command.kind,
        lazuli_ir::CommandKind::Create
            | lazuli_ir::CommandKind::Update
            | lazuli_ir::CommandKind::Delete
    )
}

fn write_effect_resource(command: &lazuli_ir::Command) -> Option<&lazuli_ir::QualifiedName> {
    match &command.effect {
        lazuli_ir::CommandEffect::Creates(effect) => Some(&effect.resource),
        lazuli_ir::CommandEffect::Updates(effect) => Some(&effect.resource),
        lazuli_ir::CommandEffect::Deletes(effect) => Some(&effect.resource),
        lazuli_ir::CommandEffect::Returns(_) | lazuli_ir::CommandEffect::None => None,
    }
}


fn canonical_payload_event(name: &str, canonical: &BTreeSet<String>) -> bool {
    !canonical.is_empty() && ir::is_reserved_trace_event_name(name)
}

/// After spotting `trigger event.trace agent_run`, walk subsequent
/// lines at deeper indent and flag any `<field> = <expr>` reference
/// where `<field>` is not part of the canonical payload. The check is
/// scoped to the job block that owns the trigger — we stop at the
/// next sibling at the same or shallower indent.
fn scan_payload_field_drift(
    file: &DoctorFile,
    lines: &[&str],
    trigger_line: usize,
    event_name: &str,
    canonical: &BTreeSet<String>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let trigger_indent = leading_spaces(lines[trigger_line]);
    let mut i = trigger_line + 1;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        let leading = leading_spaces(line);
        // Stop at the next sibling-or-shallower line. Trigger's
        // subscriber body is everything deeper than the trigger.
        if leading <= trigger_indent {
            break;
        }
        // Lines like `tokens_input = payload.tokens_input` or
        // `<field>: <expr>` reference a payload field on the LHS.
        if let Some(field) = trimmed
            .split_once('=')
            .or_else(|| trimmed.split_once(':'))
            .map(|(lhs, _)| lhs.trim())
        {
            // The LHS may include a dotted prefix (e.g.
            // `payload.tokens_input`). Strip the leading segment if
            // it's `payload`.
            let candidate = field
                .strip_prefix("payload.")
                .unwrap_or(field)
                .split_whitespace()
                .next()
                .unwrap_or("");
            if !candidate.is_empty()
                && candidate
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !canonical.contains(candidate)
            {
                // Surface as drift only if the LHS resembles a
                // field reference (lowercase ident); avoid false
                // positives on full expressions.
                if candidate
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase())
                {
                    diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: i + 1,
                        column: leading + 1,
                        severity: DoctorSeverity::Error,
                        code: "agent_run_subscriber_payload_drift_diagnostics".to_owned(),
                        message: format!(
                            "subscriber references `{candidate}` but `agent_run`'s canonical payload does not declare it. Valid fields: {}.",
                            payload_field_list(canonical),
                        ),
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    });
                    let _ = event_name; // pin for future per-event errors
                }
            }
        }
        i += 1;
    }
    diagnostics
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

fn collect_file_capability_facts(
    file: &DoctorFile,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    if !is_lzi_path(&file.path) {
        return;
    }

    let mut current_feature: Option<String> = None;
    let mut current_resource: Option<(String, usize)> = None;
    let mut current_api: Option<(String, usize)> = None;

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        let line_number = index + 1;
        let column = indent + 1;

        // Top-level feature header anchors all enclosed sites.
        if indent == 0 && trimmed.starts_with("feature ") {
            current_feature = trimmed.split_whitespace().nth(1).map(str::to_owned);
            current_resource = None;
            current_api = None;
            continue;
        }

        // Resource and api headers; close on any line that retreats to
        // the header indent or shallower (matching `inspect_storage_projection`).
        if let Some(rest) = trimmed.strip_prefix("resource ") {
            current_resource = Some((
                rest.split_whitespace().next().unwrap_or("").to_owned(),
                indent,
            ));
            current_api = None;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("api ") {
            current_api = Some((
                rest.split_whitespace().next().unwrap_or("").to_owned(),
                indent,
            ));
            current_resource = None;
            continue;
        }
        if let Some((_, header_indent)) = &current_resource {
            if indent <= *header_indent {
                current_resource = None;
            }
        }
        if let Some((_, header_indent)) = &current_api {
            if indent <= *header_indent {
                current_api = None;
            }
        }

        let Some(feature) = current_feature.as_deref() else {
            continue;
        };

        // Resource field shape: `<field>: @cap.File(...)`.
        if let Some((resource, _)) = &current_resource {
            if let Some((field_name, cap_text)) = extract_cap_file_field_line(trimmed) {
                if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(file_cap)) =
                    lazuli_analyzer::type_ref_from_syntax_public(&cap_text)
                {
                    operational.file_capability_facts.push(FileCapabilityFact {
                        path: file.path.clone(),
                        line: line_number,
                        column,
                        feature: feature.to_owned(),
                        binding: FileCapabilityBinding::ResourceField {
                            resource: resource.clone(),
                            field: field_name,
                        },
                        capability: file_cap,
                    });
                }
            }
        }

        // Api output shape: `output @cap.File(...)`.
        if let Some((api, _)) = &current_api {
            if let Some(rest) = trimmed.strip_prefix("output ") {
                let rest = rest.trim();
                if rest.starts_with("@cap.File(") {
                    if let Some(close) = rest.find(')') {
                        let cap_text = &rest[..=close];
                        if let lazuli_ir::TypeRef::Capability(lazuli_ir::CapabilityRef::File(
                            file_cap,
                        )) = lazuli_analyzer::type_ref_from_syntax_public(cap_text)
                        {
                            operational.file_capability_facts.push(FileCapabilityFact {
                                path: file.path.clone(),
                                line: line_number,
                                column,
                                feature: feature.to_owned(),
                                binding: FileCapabilityBinding::ApiOutput { api: api.clone() },
                                capability: file_cap,
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Extract `(field_name, "@cap.File(...)")` from a resource-field line.
/// Mirrors `crates/lazuli_cli/src/main.rs:extract_cap_file_field` but is
/// re-implemented here to keep the doctor crate's dependency surface
/// unchanged (no new pub item needed).
fn extract_cap_file_field_line(trimmed: &str) -> Option<(String, String)> {
    let (name_part, type_part) = trimmed.split_once(':')?;
    let name = name_part.trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    let type_tokens = type_part.trim();
    let cap_start = type_tokens.find("@cap.File(")?;
    let from_cap = &type_tokens[cap_start..];
    let close = from_cap.find(')')?;
    let cap_text = &from_cap[..=close];
    Some((name.to_owned(), cap_text.to_owned()))
}

/// Run the 10 `REPORT-*` doctor rules per
/// `docs/proposals/report-vocab.md` v0.2 §Doctor / LSP, aggregating
/// findings into typed `DoctorDiagnostic` rows.
fn report_diagnostics(
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



fn cap_file_storage_diagnostics(operational: &OperationalFacts) -> Vec<DoctorDiagnostic> {
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
fn query_view_sql_file_diagnostics(
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

fn resolve_query_view_sql_path(project_root: &Path, sql_path: &str) -> PathBuf {
    let path = Path::new(sql_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_root.join(path)
    }
}

fn query_view_unsafe_sql_line(sql: &str) -> Option<(usize, &'static str)> {
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

fn plus_near_dollar_placeholder(line: &str) -> bool {
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
