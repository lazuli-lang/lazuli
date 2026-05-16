pub mod auth;
pub mod folder;
pub mod lzx;
pub mod rbac;

// Re-export file-local diagnostic sub-modules extracted to the `lazuli_doctor`
// crate on 2026-05-15 so the LSP can import them. Existing call sites inside
// this module continue to reference them as `correctness::`, `vocab::`, etc.
pub use lazuli_doctor::{
    correctness, design, domain, encryption, lifecycle, poller, report, vocab,
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
        return doctor_release_command(input);
    }

    let package = DoctorPackage::load(input, security_profile)?;
    let diagnostics = package.diagnostics();
    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DoctorSeverity::Error);

    for diagnostic in &diagnostics {
        diagnostic.print();
    }

    if has_error {
        bail!("{} failed Lazuli doctor checks", input.display());
    }

    println!("{} passed Lazuli doctor checks", input.display());
    Ok(())
}

fn doctor_release_command(input: &Path) -> Result<()> {
    let project_root = doctor_project_root(input);
    let mut diagnostics = Vec::new();
    diagnostics.extend(check_migration_recipe_001(&project_root, LZIR_SCHEMA));
    diagnostics.extend(check_migration_recipe_002(&project_root));
    let has_error = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DoctorSeverity::Error);

    for diagnostic in &diagnostics {
        diagnostic.print();
    }

    if has_error {
        bail!("{} failed Lazuli release checks", project_root.display());
    }

    println!("{} passed Lazuli release checks", project_root.display());
    Ok(())
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
#[derive(Debug, Clone)]
struct Tier3FeatureFacts {
    feature: String,
    path: PathBuf,
    feature_line: usize,
    /// Resolved tenancy axis (`org`, `team`, custom, none) inferred
    /// from the feature's `defaults` block. `None` if the source did
    /// not declare a default. Doctor's tenant_from / fanout checks
    /// use this to cross-check axis references.
    tenancy_axis: Option<String>,
    jobs: Vec<lazuli_ir::Job>,
    webhooks: Vec<lazuli_ir::Webhook>,
    notifications: Vec<lazuli_ir::Notification>,
    event_groups: Vec<lazuli_ir::EventGroup>,
    /// Migrations bucket cycle Route C — lifted `TenantMigration`
    /// declarations for this feature, paired with `tenant_migration_lines`
    /// for `TM-*` diagnostic line anchoring.
    tenant_migrations: Vec<lazuli_ir::TenantMigration>,
    /// Migrations bucket cycle Route C — `Resource.previous_names`
    /// captures plus current resource names per feature, for
    /// `PREVIOUSLY-*` cross-checks.
    resource_previous_names: Vec<ResourcePreviousFact>,
    /// Migrations bucket cycle Route C — `Field.previous_names`
    /// captures (resource + field + previous names + line).
    field_previous_names: Vec<FieldPreviousFact>,
    /// Migrations bucket cycle Route C — every current resource name
    /// in this feature (including resources without any `previously`
    /// declaration) so `PREVIOUSLY-FWD-001` can detect stale rename
    /// targets pointing at live symbols.
    all_resource_names_in_feature: BTreeSet<String>,
    /// Migrations bucket cycle Route C — `resource_name -> {field_names}`
    /// per feature for `PREVIOUSLY-FWD-001` on field-level rename hints.
    all_field_names_in_feature: BTreeMap<String, BTreeSet<String>>,
    /// `job_name -> source line` lookup.
    job_lines: BTreeMap<String, usize>,
    webhook_lines: BTreeMap<String, usize>,
    notification_lines: BTreeMap<String, usize>,
    tenant_migration_lines: BTreeMap<String, usize>,
    /// `event_group_pattern -> source line` lookup.
    event_group_lines: BTreeMap<String, usize>,
    /// OpenAPI/Cache bucket cycles — lifted `command` IR per feature.
    /// Doctor reads `Command.deprecated` and `Command.invalidates` from
    /// here for the openapi/cache cross-checks.
    commands: Vec<lazuli_ir::Command>,
    /// `command_name -> source line` lookup. Anchors `deprecated_*` and
    /// `cache_invalidates_*` diagnostics at the command header.
    command_lines: BTreeMap<String, usize>,
    /// Cache bucket cycle — lifted `query` IR per feature. Doctor reads
    /// `Query.cache` (when populated) for the cache cross-checks.
    queries: Vec<lazuli_ir::Query>,
    /// `query_name -> source line` lookup. Anchors `cache_*` diagnostics
    /// at the query header.
    query_lines: BTreeMap<String, usize>,
    /// Cache bucket cycle (CL.C.3) — feature-level `cache <name>`
    /// profile declarations lifted from the canonical-indent slice.
    /// Doctor uses this to (1) resolve query `cache <profile>`
    /// references for `cache-profile-unknown`, (2) build the package-
    /// wide tag index for `cache-tag-unknown`, and (3) cross-check TTL
    /// shape invariants for `cache-ttl-contract`.
    caches: Vec<lazuli_ir::CacheProfile>,
    /// `cache_profile_name -> source line` lookup. Anchors CL.C.3
    /// diagnostics at the profile header.
    cache_lines: BTreeMap<String, usize>,
    /// OpenAPI bucket cycle — every `api <name>` declaration in this
    /// feature (text-pattern era, before Tier 4 lift). Doctor uses this
    /// to surface `openapi_text_pattern_api_block`.
    api_names_text_pattern: Vec<String>,
    /// i18n bucket cycle — lifted typed `api` blocks (post Tier 4).
    /// Doctor reads `Api.locale_negotiate` from here for per-endpoint
    /// override validation.
    apis: Vec<lazuli_ir::Api>,
    /// Phase L Tier 4b — `api_name -> source line` lookup for the lifted
    /// `apis` slot. Anchors `agent_expose_*` cross-checks at each api
    /// header.
    api_lines: BTreeMap<String, usize>,
    /// Cut A.7 — lifted agents for report auto-mount route conflict checks.
    agents: Vec<lazuli_ir::Agent>,
    /// i18n bucket cycle — lifted `translation` block (when authored).
    translation: Option<lazuli_ir::Translation>,
    translation_line: usize,
    /// Phase L Tier 4 follow-up — lifted `record <Name>` declarations
    /// per feature. Replaces the text-scanned `FeatureSymbols.records`
    /// for the agent discriminator cross-checks.
    records: Vec<lazuli_ir::Record>,
    /// Phase L Tier 4 follow-up — lifted `enum <Name>` declarations per
    /// feature. Closes out the canonical-indent slice for `domain`:
    /// `agent_discriminator_target_invalid` and
    /// `check_record_discriminator` both read from here. The retired
    /// `FeatureSymbols.enums` text walker is gone.
    enums: Vec<lazuli_ir::EnumDecl>,
    /// Notifications expanded bucket cycle — lifted `event` /
    /// `event.trace` declarations for this feature. `NOTIF-DIGEST-001`
    /// resolves `notification.digest.group_by` against the trigger
    /// event's payload schema; cross-feature lookup walks `facts`
    /// keyed by `<feature>.<event>`. Tracking the full payload at the
    /// fact level keeps the diagnostic shape-aware without adding a
    /// new fact family.
    events: Vec<lazuli_ir::Event>,
    /// Whether the feature authored a top-level `policies` block.
    /// `Feature.policies` has a default value, so doctor reads the
    /// lowered `span_ref` to distinguish "absent" from "declared".
    policies_declared: bool,
    /// Report vocab — lifted `report` declarations per feature. See
    /// `docs/proposals/report-vocab.md`.
    reports: Vec<lazuli_ir::Report>,
    /// `report_name -> source line` lookup. Anchors `REPORT-*`
    /// diagnostics at the report header.
    report_lines: BTreeMap<String, usize>,
    /// Resources captured (full `Resource`) per feature — used by
    /// `REPORT-COLUMN-MISMATCH-001` to resolve `row.<field>` against
    /// the source query's projection.
    resources: Vec<lazuli_ir::Resource>,
    /// Raw `ReportDecl` AST per feature — used by rules that need the
    /// original (pre-lowering) form (e.g. `REPORT-FORMAT-UNKNOWN-001`
    /// scans the AST formats list since lowering drops unknown tokens).
    report_decls: Vec<lazuli_syntax::ReportDecl>,
    /// CL.C.4 — lifted `aggregate <Name>` declarations per feature.
    /// Powers the four domain-model diagnostics:
    /// `AGGREGATE-ROOT-UNKNOWN`, `AGGREGATE-CONTAINS-UNKNOWN`,
    /// `INVARIANT-PREDICATE-INVALID`, `SLUG-UNIQUENESS-IMPLICIT`.
    /// Empty vec when the feature authored no aggregate blocks.
    aggregates: Vec<lazuli_ir::Aggregate>,
    /// CL.C.4 — `aggregate_name -> source line` lookup. Anchors the
    /// `AGGREGATE-*` and aggregate-scoped `INVARIANT-*` diagnostics
    /// at the aggregate header.
    aggregate_lines: BTreeMap<String, usize>,
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
        // `.lzi` fixtures was scanned for @plugin/* refs from sibling files.
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
                                                .map(|s| line_col_for_offset(&file.source, s.start).0)
                                                .unwrap_or(header_line);
                                            aggregate_lines.insert(agg.name.clone(), agg_line);
                                        }
                                        tier3_facts.push(Tier3FeatureFacts {
                                            feature: feature.name.clone(),
                                            path: file.path.clone(),
                                            feature_line: header_line,
                                            tenancy_axis: tenancy_axis_for(&feature),
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
                                            reports: feature.reports.clone(),
                                            report_lines,
                                            resources: feature.resources.clone(),
                                            report_decls: skeleton.reports.clone(),
                                            aggregates: feature.aggregates.clone(),
                                            aggregate_lines,
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
                        file.lzx = Some(document);
                    }
                    Err(error) => file.local_diagnostics.push(DoctorDiagnostic {
                        path: file.path.clone(),
                        line: line_col_for_offset(&file.source, error.span().start).0,
                        column: line_col_for_offset(&file.source, error.span().start).1,
                        severity: DoctorSeverity::Error,
                        code: "LZX-PARSE".to_owned(),
                        message: error.to_string(),
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
        let mut feature_gates_raw: Vec<(String, lazuli_syntax::FeatureGatesAst)> =
            Vec::new();
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
                    let feature_name = derive_feature_name(&file.source)
                        .unwrap_or_else(|| {
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
        let plan_gate_facts = if plan_blocks_raw.is_empty()
            && feature_gates_raw.is_empty()
            && anchor.is_none()
        {
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

    fn diagnostics(&self) -> Vec<DoctorDiagnostic> {
        let mut diagnostics = Vec::new();

        diagnostics.extend(manifest_required_diagnostics(
            &self.project_root,
            self.single_file_input,
        ));
        diagnostics.extend(lazurite_manifest_diagnostics(self));

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
                });
            }
        }

        for file in &self.files {
            diagnostics.extend(dedupe_env_contract_diagnostics(&file.local_diagnostics));
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
        // probe path shape.
        diagnostics.extend(audit_event_health_diagnostics(
            &self.files,
            self.app.as_ref(),
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
        diagnostics.extend(approval_missing_children_diagnostics(
            &self.approval_presences,
        ));

        diagnostics.extend(app_urls_missing_diagnostics(self.app.as_ref()));

        // Cut A.11 — `cors` block cross-checks against the app's
        // declared environments + urls.
        diagnostics.extend(cors_diagnostics(self.app.as_ref()));

        // Roadmap §1.2 — HTTP hygiene contracts: cookie / proxy /
        // limits. Each block's typed lift is doctor-validated against
        // the closed catalog (same_site, parseable CIDR/size/duration).
        diagnostics.extend(app_cookie_contract_diagnostics(self.app.as_ref()));
        diagnostics.extend(app_proxy_contract_diagnostics(self.app.as_ref()));
        diagnostics.extend(app_limits_contract_diagnostics(self.app.as_ref()));

        // Roadmap §1.10 — `app.headers` production-completeness +
        // closed-catalog gate.
        diagnostics.extend(app_headers_diagnostics(
            self.app.as_ref(),
            self.security_profile,
        ));
        // Roadmap §1.10 — `secret_rotation` overlap + binding
        // cross-check.
        diagnostics.extend(secret_rotation_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));

        // Observability bucket cycle row 36 — `app.logging` and
        // `app.tracing` closed-catalog + range + exporter binding
        // checks.
        diagnostics.extend(app_logging_tracing_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));
        diagnostics.extend(app_observability_diagnostics(self.app.as_ref()));

        // Phase L — auth block cross-feature diagnostics.
        diagnostics.extend(auth_diagnostics(
            &self.auth_facts,
            &self.feature_resources,
            &self.feature_adapters,
            &self.feature_uses,
            self.registry.as_ref(),
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
        diagnostics.extend(webhook_event_registry_diagnostics(
            self.registry.as_ref(),
        ));

        // Row 34 — `event_group` pattern-prefix rule promoted from LSP
        // to doctor now that `EventGroup` IR exists.
        diagnostics.extend(event_group_pattern_prefix_diagnostics(&self.tier3_facts));

        // Rows 41-43 — Migrations bucket cycle Route C: eight new
        // IR-driven diagnostics covering rename hints, the
        // `tenant_migration` kind, and the deploy block expansion. See
        // `docs/proposals/bucket-migrations-cycle.md` §Doctor.
        diagnostics.extend(migrations_diagnostics(&self.tier3_facts, self.app.as_ref()));

        // Row 48 — OpenAPI bucket cycle: five `deprecated_*` + text-pattern
        // api detection. See `docs/proposals/bucket-openapi-cycle.md`
        // §Doctor/LSP.
        diagnostics.extend(openapi_deprecated_diagnostics(&self.tier3_facts));

        // Row 51 — Cache bucket cycle: five `cache_*` diagnostics. See
        // `docs/proposals/bucket-cache-cycle.md` §Doctor/LSP.
        diagnostics.extend(cache_diagnostics(
            &self.tier3_facts,
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));

        // Row 54 — i18n bucket cycle: 15 locale/translation diagnostics.
        // See `docs/proposals/bucket-i18n-cycle.md` §Doctor/LSP.
        diagnostics.extend(i18n_diagnostics(
            &self.tier3_facts,
            self.app.as_ref(),
            &self.files,
        ));
        diagnostics.extend(check_codegen_wrap_001(&self.project_root));
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
        diagnostics.extend(domain_diagnostics(&self.tier3_facts));

        diagnostics.extend(folder_layout_diagnostics(
            &self.project_root,
            self.security_profile,
        ));
        diagnostics.extend(design_token_diagnostics(
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
        diagnostics
    }
}

fn doctor_rule_severity(security_profile: SecurityProfile) -> DoctorSeverity {
    match security_profile {
        SecurityProfile::Production => DoctorSeverity::Error,
        SecurityProfile::Prototype | SecurityProfile::Strict => DoctorSeverity::Warning,
    }
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
                    }
                })
        })
        .collect()
}

fn folder_layout_diagnostics(
    project_root: &Path,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_rule_severity(security_profile);
    let mut diagnostics = Vec::new();

    diagnostics.extend(
        folder::feature_orphan::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.path),
                line: 1,
                column: 1,
                severity,
                code: folder::feature_orphan::Finding::CODE.to_owned(),
                message: finding.message,
            }),
    );
    diagnostics.extend(
        folder::pages_bypass::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.path),
                line: 1,
                column: 1,
                severity,
                code: folder::pages_bypass::Finding::CODE.to_owned(),
                message: finding.message,
            }),
    );
    diagnostics.extend(
        folder::type_duplicate::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.user_file),
                line: 1,
                column: 1,
                severity,
                code: folder::type_duplicate::Finding::CODE.to_owned(),
                message: finding.message,
            }),
    );
    diagnostics.extend(
        folder::cross_feature_import::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.source_file),
                line: 1,
                column: 1,
                severity,
                code: folder::cross_feature_import::Finding::CODE.to_owned(),
                message: finding.message,
            }),
    );

    diagnostics
}

fn design_token_diagnostics(
    project_root: &Path,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let Some(allowlist) = design::read_allowlist(project_root) else {
        return Vec::new();
    };

    let severity = doctor_rule_severity(security_profile);
    let mut diagnostics = Vec::new();

    diagnostics.extend(
        design::token_undefined::check(project_root, &allowlist)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::token_undefined::Finding::CODE.to_owned(),
                    message,
                }
            }),
    );
    diagnostics.extend(
        design::hex_leak::check(project_root)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::hex_leak::Finding::CODE.to_owned(),
                    message,
                }
            }),
    );
    diagnostics.extend(
        design::px_leak::check(project_root)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::px_leak::Finding::CODE.to_owned(),
                    message,
                }
            }),
    );
    diagnostics.extend(
        design::fontfamily_leak::check(project_root, &allowlist)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::fontfamily_leak::Finding::CODE.to_owned(),
                    message,
                }
            }),
    );
    diagnostics.extend(
        design::shadow_leak::check(project_root)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::shadow_leak::Finding::CODE.to_owned(),
                    message,
                }
            }),
    );

    diagnostics
}

fn doctor_rule_path(project_root: &Path, path: PathBuf) -> PathBuf {
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
struct DoctorAppManifest {
    path: PathBuf,
    source: String,
    manifest: AppManifest,
}

#[derive(Debug)]
struct DoctorAppRegistry {
    path: PathBuf,
    manifest: AppRegistry,
}

#[derive(Debug)]
struct DoctorAppProfile {
    path: PathBuf,
    profile: AppProfile,
}

#[derive(Debug)]
struct DoctorFile {
    path: PathBuf,
    source: String,
    local_diagnostics: Vec<DoctorDiagnostic>,
    lzx: Option<LzxDocument>,
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
enum DoctorSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Debug, Clone)]
struct DoctorDiagnostic {
    path: PathBuf,
    line: usize,
    column: usize,
    severity: DoctorSeverity,
    code: String,
    message: String,
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

fn doctor_project_root(input: &Path) -> PathBuf {
    if input.is_dir() {
        return input.to_path_buf();
    }

    input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn project_has_lazurite_manifest(project_root: &Path) -> bool {
    project_root.join("Lazurite.toml").is_file()
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
            !(d.code == "app-env-contract"
                && env_schema_lines.contains(&(d.path.clone(), d.line)))
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
    // unrelated sibling fixtures' `@plugin/*` refs, and pointed the user
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
        message: "project uses @plugin/* references but is missing Lazurite.toml.".to_owned(),
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
    diagnostics.extend(check_submodule_drift(manifest, package));
    diagnostics.extend(check_migration_strategy_conflict(manifest, package));
    diagnostics.extend(check_frontend_audience_unknown(manifest, package));
    diagnostics.extend(check_audience_no_frontend(manifest, package));
    diagnostics.extend(check_frontend_out_collision(manifest, package));
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
    }]
}

fn major_minor(version: &str) -> String {
    let mut parts = version.split('.');
    let Some(major) = parts.next() else {
        return version.to_owned();
    };
    let Some(minor) = parts.next() else {
        return version.to_owned();
    };
    format!("{major}.{minor}")
}

fn is_one_dot_zero_plus(version: &str) -> bool {
    version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u64>().ok())
        .is_some_and(|major| major >= 1)
}

fn lazuli_version_line(source: &str) -> Option<usize> {
    source
        .lines()
        .position(|line| {
            leading_spaces(line) == 2 && line.trim_start().starts_with("lazuli_version ")
        })
        .map(|line| line + 1)
}

fn check_migration_recipe_001(project_root: &Path, lzir_schema: &str) -> Vec<DoctorDiagnostic> {
    let Ok(output) = std::process::Command::new("git")
        .args(["show", "HEAD~1:crates/lazuli_ir/src/lib.rs"])
        .current_dir(project_root)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let previous_source = String::from_utf8_lossy(&output.stdout);
    let Some(previous_schema) = extract_lzir_schema(&previous_source) else {
        return Vec::new();
    };
    if previous_schema == lzir_schema {
        return Vec::new();
    }

    let previous_major_minor = major_minor(&previous_schema);
    let current_major_minor = major_minor(lzir_schema);
    let transition_dir = project_root.join("migrations/recipes").join(format!(
        "{}-to-{}",
        previous_major_minor, current_major_minor
    ));
    let recipe_count = fs::read_dir(&transition_dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .count()
        })
        .unwrap_or(0);

    if recipe_count > 0 {
        return Vec::new();
    }

    vec![DoctorDiagnostic {
        path: transition_dir,
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "MIGRATION-RECIPE-001".to_owned(),
        message: format!(
            "LZIR_SCHEMA changed from {} to {}, but no recipe directory exists under migrations/recipes/{}-to-{}/.",
            previous_schema, lzir_schema, previous_major_minor, current_major_minor
        ),
    }]
}

fn check_migration_recipe_002(project_root: &Path) -> Vec<DoctorDiagnostic> {
    let recipe_root = project_root.join("migrations/recipes");
    let mut recipe_dirs = Vec::new();
    collect_recipe_dirs(&recipe_root, &mut recipe_dirs);

    let mut diagnostics = Vec::new();
    for recipe_dir in recipe_dirs {
        let input = recipe_dir.join("input.lzi");
        let output = recipe_dir.join("output.lzi");
        if !input.exists() && !output.exists() {
            continue;
        }
        if let Err(error) = crate::upgrade::smoke_recipe(&recipe_dir) {
            diagnostics.push(DoctorDiagnostic {
                path: recipe_dir,
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "MIGRATION-RECIPE-002".to_owned(),
                message: format!("migration recipe smoke failed: {error}"),
            });
        }
    }
    diagnostics
}

fn collect_recipe_dirs(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.join("recipe.toml").is_file() {
            out.push(path);
        } else {
            collect_recipe_dirs(&path, out);
        }
    }
}

fn extract_lzir_schema(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let trimmed = line.trim();
        if !trimmed.starts_with("pub const LZIR_SCHEMA") {
            return None;
        }
        let (_, rest) = trimmed.split_once('"')?;
        let (value, _) = rest.split_once('"')?;
        Some(value.to_owned())
    })
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
        .filter_map(|key| key.strip_prefix("@plugin/").map(|name| name.to_owned()))
        .collect();

    for key in manifest.plugins.keys() {
        if let Some(namespace) = reference_namespace(key) {
            if namespace != "plugin" {
                diagnostics.push(DoctorDiagnostic {
                    path: package.project_root.join("Lazurite.toml"),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                    message: format!(
                        "manifest plugin key `{key}` uses namespace `@{namespace}`, but plugins must use `@plugin/<name>`."
                    ),
                });
            }
        }
    }

    for file in &package.files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        for reference in collect_at_references_in_source(&file.path, &file.source) {
            if reference.namespace == "adapter" && declared_short.contains(&reference.name) {
                diagnostics.push(DoctorDiagnostic {
                    path: reference.path,
                    line: reference.line,
                    column: reference.column,
                    severity: DoctorSeverity::Error,
                    code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                    message: format!(
                        "`{}` uses the local adapter namespace, but Lazurite.toml declares `@plugin/{}`; use the plugin reference.",
                        reference.reference, reference.name
                    ),
                });
            } else if reference.namespace != "plugin"
                && !is_allowed_reference_namespace_for_doctor(&reference.namespace)
                && declared_short.contains(&reference.name)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: reference.path,
                    line: reference.line,
                    column: reference.column,
                    severity: DoctorSeverity::Error,
                    code: "PLUGIN-NAMESPACE-MISMATCH-001".to_owned(),
                    message: format!(
                        "`{}` uses unknown namespace `@{}`, but Lazurite.toml declares `@plugin/{}`; use the plugin reference.",
                        reference.reference, reference.namespace, reference.name
                    ),
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
            });
        }
    }
    diagnostics
}

fn collect_lazuli_paths_recursive(root: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root).with_context(|| format!("failed to list {}", root.display()))? {
        let entry = entry.with_context(|| format!("failed to read {}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_lazuli_paths_recursive(&path, paths)?;
        } else if path.is_file() && (is_lzi_path(&path) || is_lzx_path(&path)) {
            paths.push(path);
        }
    }
    Ok(())
}

fn package_stem(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    if let Some(stem) = file_name.strip_suffix(".lzi") {
        return Some(stem.to_owned());
    }

    let stem = file_name.strip_suffix(".lzx")?;
    Some(stem.split('.').next().unwrap_or(stem).to_owned())
}

fn is_lzi_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("lzi")
}

fn is_lzx_path(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("lzx")
}

/// PG.B — read the first `feature <name>` header from a `.lzi` source.
/// Returns `None` for app.lzi / registry.lzi / contracts that don't
/// declare a feature.
fn derive_feature_name(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("feature ") {
            return rest.split_whitespace().next().map(|s| s.to_owned());
        }
    }
    None
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

fn error_page_catalog_display() -> String {
    ir::ERROR_PAGE_STATUS_CATALOG
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ")
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
                });
            }
        }
    }

    diagnostics
}

fn webhook_event_registry_diagnostics(
    registry: Option<&DoctorAppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let Some(registry) = registry else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();

    for event in &registry.manifest.webhook_events {
        if let Some(previous_version) = event.previous_version {
            if previous_version > event.version {
                diagnostics.push(DoctorDiagnostic {
                    path: registry.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "webhook-event-version-decreasing".to_owned(),
                    message: format!(
                        "`webhook_event {}` declares `previous_version {}` greater than current `version {}`.",
                        event.name, previous_version, event.version
                    ),
                });
            }
        }

        if event.payload.is_empty() {
            diagnostics.push(DoctorDiagnostic {
                path: registry.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "webhook-event-payload-empty".to_owned(),
                message: format!(
                    "`webhook_event {}` declares no payload fields; outbound event schemas must be explicit.",
                    event.name
                ),
            });
        }

        if event.deprecated && event.previous_version.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: registry.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "webhook-event-deprecated-no-replacement".to_owned(),
                message: format!(
                    "`webhook_event {}` is deprecated but declares no replacement trail; add `previous_version <n>` or document the successor inline.",
                    event.name
                ),
            });
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
fn is_valid_notification_duration(raw: &str) -> bool {
    parse_notification_duration_seconds(raw).is_some()
}

fn parse_notification_duration_seconds(raw: &str) -> Option<u64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (num_part, unit_part) = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .map(|idx| trimmed.split_at(idx))
        .unwrap_or(("", ""));
    if num_part.is_empty() {
        return None;
    }
    let n = num_part.parse::<u64>().ok()?;
    let unit = unit_part.trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        "d" | "day" | "days" => 24 * 60 * 60,
        _ => return None,
    };
    n.checked_mul(multiplier)
}

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
fn event_group_pattern_prefix_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for feature in facts {
        for group in &feature.event_groups {
            let line = feature
                .event_group_lines
                .get(&group.pattern)
                .copied()
                .unwrap_or(feature.feature_line);

            // EVENTGROUP-NESTING-001: parse_event_group records nested
            // `event_group` headers as raw child lines today, so we
            // scan `raw_payload` + adjacent groups; for now we surface
            // the case where two groups share the same parent feature
            // *and* one pattern fully contains another's prefix.
            for other in &feature.event_groups {
                if other.pattern == group.pattern {
                    continue;
                }
                if let (Some(group_prefix), Some(other_prefix)) = (
                    group.pattern.strip_suffix('*'),
                    other.pattern.strip_suffix('*'),
                ) {
                    if other_prefix.starts_with(group_prefix) && other_prefix != group_prefix {
                        diagnostics.push(DoctorDiagnostic {
                            path: feature.path.clone(),
                            line,
                            column: 1,
                            severity: DoctorSeverity::Warning,
                            code: "EVENTGROUP-NESTING-001".to_owned(),
                            message: format!(
                                "event_group `{}` in feature `{}` is a prefix of `{}` — nest in the more specific group or rename one pattern.",
                                group.pattern, feature.feature, other.pattern
                            ),
                        });
                    }
                }
            }

            // Pattern-prefix rule (row 34). Strip trailing `*` to get
            // the group prefix. Short event names are *promoted* by
            // the group's prefix at lowering time — `event created`
            // under `customer_*` becomes the qualified event
            // `customer_created`. Authored event names are short names
            // by default in canonical Lazuli; the rule only fires
            // when the same feature declares *another* group whose
            // prefix matches the event — then the author probably
            // wrote the event under the wrong group.
            if let Some(prefix) = group.pattern.strip_suffix('*') {
                if !prefix.is_empty() {
                    for event_name in &group.events {
                        if event_name.starts_with(prefix) {
                            continue;
                        }
                        // Look for another group whose prefix the
                        // event matches; only then is misrouting likely.
                        let other_owner = feature.event_groups.iter().find(|other| {
                            if other.pattern == group.pattern {
                                return false;
                            }
                            let Some(other_prefix) = other.pattern.strip_suffix('*') else {
                                return false;
                            };
                            !other_prefix.is_empty() && event_name.starts_with(other_prefix)
                        });
                        if let Some(other) = other_owner {
                            diagnostics.push(DoctorDiagnostic {
                                path: feature.path.clone(),
                                line,
                                column: 1,
                                severity: DoctorSeverity::Warning,
                                code: "EVENTGROUP-PREFIX-001".to_owned(),
                                message: format!(
                                    "event `{}` authored under group `{}` matches group `{}`'s prefix — move it to the matching group or rename.",
                                    event_name, group.pattern, other.pattern
                                ),
                            });
                        }
                    }
                }
            }
        }
    }
    diagnostics
}

/// Migrations bucket cycle Route C — eight IR-driven cross-checks.
///
/// `PREVIOUSLY-FWD-001` — `Resource.previous_names` / `Field.previous_names`
/// reference a name that exists nowhere in the package. Warning.
///
/// `PREVIOUSLY-CYCLE-001` — `A previously B`, `B previously A` cycle.
/// Error (silent-data-loss footgun).
///
/// `PREVIOUSLY-DUP-001` — two current names claim the same `previously`
/// source. Error.
///
/// `TM-AXIS-001` — `tenant_migration target tenants <axis>` references
/// an axis not declared in any `defaults.tenancy` for the same feature.
/// Error.
///
/// `TM-IDEMP-001` — `tenant_migration` lacks `idempotency by`. Error.
///
/// `DEPLOY-CHECKPOINT-001` — `deploy.checkpoint` path does not resolve
/// to a file relative to `app.lzi`. Error.
///
/// `DEPLOY-CHECKPOINT-002` — `deploy.checkpoint` file exists but its
/// snapshot's `lazuli_version` lags the analyzer's expectation. Warning.
///
/// `DEPLOY-STRATEGY-001` — `deploy.strategy` not in closed catalog
/// `{rolling, blue_green, canary}`. Error.
fn migrations_diagnostics(
    tier3_facts: &[Tier3FeatureFacts],
    app: Option<&DoctorAppManifest>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut queries_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut commands_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for feature in tier3_facts {
        queries_by_feature.insert(
            feature.feature.as_str(),
            feature.queries.iter().map(query_name).collect(),
        );
        commands_by_feature.insert(
            feature.feature.as_str(),
            feature.commands.iter().map(|c| c.name.as_str()).collect(),
        );
    }

    for feature in tier3_facts {
        previously_diagnostics(feature, &mut diagnostics);
        tenant_migration_diagnostics(
            feature,
            &queries_by_feature,
            &commands_by_feature,
            &mut diagnostics,
        );
    }

    if let Some(app) = app {
        deploy_strategy_diagnostics(app, &mut diagnostics);
        deploy_checkpoint_diagnostics(app, &mut diagnostics);
    }

    diagnostics
}

fn query_name(query: &lazuli_ir::Query) -> &str {
    match query {
        lazuli_ir::Query::List(q) => &q.name,
        lazuli_ir::Query::Lookup(q) => &q.name,
        lazuli_ir::Query::Sql(q) => &q.name,
    }
}

fn previously_diagnostics(feature: &Tier3FeatureFacts, diagnostics: &mut Vec<DoctorDiagnostic>) {
    let all_resource_names: &BTreeSet<String> = &feature.all_resource_names_in_feature;
    let all_field_names: &BTreeMap<String, BTreeSet<String>> = &feature.all_field_names_in_feature;
    // PREVIOUSLY-DUP-001 — two current names claim the same previous
    // source. Build a `previous -> Vec<current>` map per feature.
    let mut resource_previous_claims: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for fact in &feature.resource_previous_names {
        for prev in &fact.previous_names {
            resource_previous_claims
                .entry(prev.as_str())
                .or_default()
                .push(fact.current_name.as_str());
        }
    }
    for (prev, currents) in &resource_previous_claims {
        if currents.len() > 1 {
            // Anchor on the first claiming resource line.
            let first_current = currents[0];
            let line = feature
                .resource_previous_names
                .iter()
                .find(|f| f.current_name == first_current)
                .map(|f| f.line)
                .unwrap_or(feature.feature_line);
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "PREVIOUSLY-DUP-001".to_owned(),
                message: format!(
                    "resource rename target `{}` is claimed by multiple current resources ({}) in feature `{}` — only one current name may inherit a previous identity.",
                    prev,
                    currents.join(", "),
                    feature.feature
                ),
            });
        }
    }

    // PREVIOUSLY-FWD-001 + PREVIOUSLY-CYCLE-001 on resources.
    for fact in &feature.resource_previous_names {
        for prev in &fact.previous_names {
            // FWD-001 — the previous name must NOT exist as a current
            // resource name (it has been renamed away). If it does, the
            // author may have copy-pasted a stale identifier.
            if all_resource_names.contains(prev.as_str()) {
                // Check for a rename cycle: does the resource `prev`
                // claim `fact.current_name` as one of its previous names?
                let cycle = tier3_iter_resource_previously_pairs(feature, prev.as_str())
                    .any(|other_prev| other_prev == fact.current_name);
                if cycle {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "PREVIOUSLY-CYCLE-001".to_owned(),
                        message: format!(
                            "resource rename cycle between `{}` and `{}` in feature `{}` — only one direction may carry `previously migrated`.",
                            fact.current_name, prev, feature.feature
                        ),
                    });
                } else {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: fact.line,
                        column: 1,
                        severity: DoctorSeverity::Warning,
                        code: "PREVIOUSLY-FWD-001".to_owned(),
                        message: format!(
                            "resource `{}` declares `previously migrated {}` but `{}` is also a current resource — the rename hint will be ignored or misrouted.",
                            fact.current_name, prev, prev
                        ),
                    });
                }
            }
        }
    }

    // PREVIOUSLY-FWD-001 on fields — the previous field name must NOT
    // shadow a current field on the same resource.
    for fact in &feature.field_previous_names {
        let current_fields = all_field_names
            .get(fact.resource_name.as_str())
            .cloned()
            .unwrap_or_default();
        for prev in &fact.previous_names {
            if current_fields.contains(prev.as_str()) && prev != &fact.current_name {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: fact.line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "PREVIOUSLY-FWD-001".to_owned(),
                    message: format!(
                        "field `{}.{}` declares `previously migrated {}` but `{}` is also a current field on the same resource.",
                        fact.resource_name, fact.current_name, prev, prev
                    ),
                });
            }
        }
    }
}

/// Helper for cycle detection: yield every `previous_name` declared by
/// any resource whose current name matches `current`. Iterator avoids
/// cloning the entire fact list.
fn tier3_iter_resource_previously_pairs<'a>(
    feature: &'a Tier3FeatureFacts,
    current: &'a str,
) -> impl Iterator<Item = &'a str> {
    feature
        .resource_previous_names
        .iter()
        .filter(move |f| f.current_name == current)
        .flat_map(|f| f.previous_names.iter().map(String::as_str))
}

fn tenant_migration_diagnostics(
    feature: &Tier3FeatureFacts,
    queries_by_feature: &BTreeMap<&str, BTreeSet<&str>>,
    commands_by_feature: &BTreeMap<&str, BTreeSet<&str>>,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    for tm in &feature.tenant_migrations {
        let line = feature
            .tenant_migration_lines
            .get(&tm.name)
            .copied()
            .unwrap_or(feature.feature_line);

        if let Some(operation) = &tm.target.operation {
            let (kind, target_feature, name, index) = match operation {
                lazuli_ir::TenantMigrationTargetOperation::Query { feature: target_feature, name } => {
                    (
                        "query",
                        target_feature.as_deref().unwrap_or(feature.feature.as_str()),
                        name.as_str(),
                        queries_by_feature,
                    )
                }
                lazuli_ir::TenantMigrationTargetOperation::Command { feature: target_feature, name } => {
                    (
                        "command",
                        target_feature.as_deref().unwrap_or(feature.feature.as_str()),
                        name.as_str(),
                        commands_by_feature,
                    )
                }
            };
            if !index
                .get(target_feature)
                .map(|names| names.contains(name))
                .unwrap_or(false)
            {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "tenant-migration-target-unknown".to_owned(),
                    message: format!(
                        "tenant_migration `{}` targets unknown {} `{}.{}`.",
                        tm.name, kind, target_feature, name
                    ),
                });
            }
        }

        let handler_path = feature
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&tm.handler.path);
        if !handler_path.is_file() {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "tenant-migration-handler-missing".to_owned(),
                message: format!(
                    "tenant_migration `{}` handler `{}` does not exist on disk.",
                    tm.name, tm.handler.path
                ),
            });
        }

        // Target axis must match the feature's tenancy axis.
        if let Some(axis) = &feature.tenancy_axis {
            if &tm.target.axis != axis {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "tenant-migration-axis-mismatch".to_owned(),
                    message: format!(
                        "tenant_migration `{}` declares `axis {}` but feature `{}` uses tenancy axis `{}`.",
                        tm.name, tm.target.axis, feature.feature, axis
                    ),
                });
            }
        } else {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "tenant-migration-axis-mismatch".to_owned(),
                message: format!(
                    "tenant_migration `{}` declares `axis {}` but feature `{}` did not declare a `defaults.tenancy` axis.",
                    tm.name, tm.target.axis, feature.feature
                ),
            });
        }

        // `idempotency <path>` is mandatory; absence
        // surfaces as an empty `IdempotencyKey.by` Path.
        if tm.idempotency.by.segments.is_empty() {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "tenant-migration-idempotency-required".to_owned(),
                message: format!(
                    "tenant_migration `{}` does not declare `idempotency <path>` — tenant migrations are not safely re-runnable without an idempotency key.",
                    tm.name
                ),
            });
        }
    }
}

const DEPLOY_STRATEGY_CATALOG: &[&str] = &["rolling", "blue_green", "canary"];

fn deploy_strategy_diagnostics(app: &DoctorAppManifest, diagnostics: &mut Vec<DoctorDiagnostic>) {
    let Some(deploy) = app.manifest.deploy.as_ref() else {
        return;
    };
    let Some(strategy) = deploy.strategy.as_ref() else {
        return;
    };
    if !DEPLOY_STRATEGY_CATALOG.contains(&strategy.as_str()) {
        diagnostics.push(DoctorDiagnostic {
            path: app.path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "DEPLOY-STRATEGY-001".to_owned(),
            message: format!(
                "app `deploy.strategy {}` is not in the closed catalog ({}).",
                strategy,
                DEPLOY_STRATEGY_CATALOG.join(", ")
            ),
        });
    }
}

fn deploy_checkpoint_diagnostics(app: &DoctorAppManifest, diagnostics: &mut Vec<DoctorDiagnostic>) {
    let Some(deploy) = app.manifest.deploy.as_ref() else {
        return;
    };
    let Some(checkpoint) = deploy.checkpoint.as_ref() else {
        return;
    };
    // DEPLOY-CHECKPOINT-001 — path must resolve to a file relative
    // to the directory containing `app.lzi`.
    let app_dir = app
        .path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    let candidate = app_dir.join(&checkpoint.path);
    if !candidate.exists() {
        diagnostics.push(DoctorDiagnostic {
            path: app.path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "DEPLOY-CHECKPOINT-001".to_owned(),
            message: format!(
                "deploy checkpoint `{}` references path `{}` that does not exist relative to app.lzi.",
                checkpoint.name, checkpoint.path
            ),
        });
        return;
    }

    // DEPLOY-CHECKPOINT-002 — load snapshot and verify `lazuli_version`
    // (a top-level JSON field). Stale = warning, not error: the snapshot
    // file existed but is older than the analyzer's expected schema.
    if let Ok(text) = std::fs::read_to_string(&candidate) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            let snapshot_version = value
                .get("lazuli_version")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let expected = env!("CARGO_PKG_VERSION");
            if !snapshot_version.is_empty() && snapshot_version != expected {
                diagnostics.push(DoctorDiagnostic {
                    path: app.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "DEPLOY-CHECKPOINT-002".to_owned(),
                    message: format!(
                        "deploy checkpoint `{}` snapshot `lazuli_version` ({}) lags analyzer ({}); regenerate to silence this warning.",
                        checkpoint.name, snapshot_version, expected
                    ),
                });
            }
        }
    }
}

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
            "integration `{integration_name}` uses adapter `{adapter}`, but adapter sources must declare provenance with `@runtime/...`, `@plugin/publisher/name`, `@adapter.<local>`, or a local path."
        ),
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

fn leading_spaces(line: &str) -> usize {
    line.bytes().take_while(|byte| *byte == b' ').count()
}

fn is_identifier(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_type_name(source: &str) -> bool {
    let mut chars = source.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_uppercase())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
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
    while let Some(relative_start) = source[offset..].find("@plugin/") {
        let start = offset + relative_start;
        let after_prefix = &source[start + "@plugin/".len()..];
        let name_len = plugin_reference_name_len(after_prefix);
        if name_len > 0 {
            let (line, column) = line_col_for_offset(source, start);
            references.push(PluginReferenceFact {
                path: path.to_path_buf(),
                line,
                column,
                reference: source[start..start + "@plugin/".len() + name_len].to_owned(),
            });
        }
        offset = start + "@plugin/".len() + name_len.max(1);
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

fn span_line(
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

fn tool_kind_word(kind: ir::ToolKind) -> &'static str {
    match kind {
        ir::ToolKind::QueryList => "query.list",
        ir::ToolKind::QueryLookup => "query.lookup",
        ir::ToolKind::QuerySql => "query.sql",
        ir::ToolKind::QueryUnspecified => "query",
        ir::ToolKind::Command => "command",
        ir::ToolKind::Api => "api",
    }
}

fn format_agent_policy(agent: &Agent) -> String {
    match agent.policy.as_ref() {
        Some(ir::PolicyRef::Atom(name)) => format!("@{name}"),
        Some(ir::PolicyRef::Local(name)) => format!("@policy.{name}"),
        Some(ir::PolicyRef::External { feature, name }) => format!("{feature}.{name}"),
        Some(ir::PolicyRef::Unresolved(text)) => text.clone(),
        Some(ir::PolicyRef::None) | None => "<none>".to_owned(),
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
        });
    }

    diagnostics
}

/// Phase L Tier 4 follow-up — project a typed `TypeRef` back to its
/// short name for cross-type lookups (used by
/// `check_record_discriminator` to find the matching enum). Many
/// variants don't yield a usable name; callers fall back to the empty
/// string and the enum lookup fails as expected.
fn type_ref_name(t: &lazuli_ir::TypeRef) -> String {
    use lazuli_ir::TypeRef;
    match t {
        TypeRef::UserDefined(qn) | TypeRef::EnumRef(qn) => qn.name.clone(),
        TypeRef::Unresolved(name) => name.clone(),
        TypeRef::Many(inner) => type_ref_name(inner),
        _ => String::new(),
    }
}

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
fn normalise_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for segment in path.split('/') {
        if !out.is_empty() {
            out.push('/');
        } else if path.starts_with('/') {
            // preserve leading `/`
        }
        if let Some(_name) = segment.strip_prefix(':') {
            out.push_str(":_");
        } else {
            out.push_str(segment);
        }
    }
    if path.starts_with('/') && !out.starts_with('/') {
        format!("/{out}")
    } else {
        out
    }
}

fn http_method_word(method: ir::HttpMethod) -> &'static str {
    match method {
        ir::HttpMethod::Get => "GET",
        ir::HttpMethod::Post => "POST",
        ir::HttpMethod::Put => "PUT",
        ir::HttpMethod::Patch => "PATCH",
        ir::HttpMethod::Delete => "DELETE",
    }
}

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
    }]
}

const APP_URLS_MISSING_MESSAGE: &str = "app declares no `urls` block — auth callbacks, CORS allowlist, and frontend redirect targets cannot be configured. Add a `urls` block to app.lzi with at least one environment URL (e.g., `urls\n  dev: \"http://localhost:3000\"`).";

// -----------------------------------------------------------------------------
// Cut A.11 — `cors` block cross-feature checks
//
// CORS lives in `app.lzi` (language-light tier) alongside `urls`.
// Doctor validates origins against the declared environments + urls
// and catches the CORS-spec violation of `allow_origins "*"` with
// `allow_credentials true`.
// -----------------------------------------------------------------------------

fn cors_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(cors) = app_manifest.manifest.cors.as_ref() else {
        return diagnostics;
    };

    let environments: BTreeSet<&str> = app_manifest
        .manifest
        .environments
        .iter()
        .map(String::as_str)
        .collect();
    let declared_urls: Vec<&lazuli_ir::AppUrl> = app_manifest.manifest.urls.iter().collect();

    let mut has_wildcard = false;
    for rule in &cors.allow_origins {
        // Environment must be declared in `app.environments`.
        if !environments.contains(rule.environment.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: app_manifest.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "cors_unknown_environment_diagnostics".to_owned(),
                message: format!(
                    "`cors allow_origins {} ...` references an environment that is not in `app.environments` ({}).",
                    rule.environment,
                    environments_summary(&environments),
                ),
            });
        }

        for origin in &rule.origins {
            if origin == "*" {
                has_wildcard = true;
                continue;
            }
            // Wildcards in subdomain (`https://*.example.com`) skip
            // url-match — they're intentionally broader than any
            // single URL declaration.
            if origin.contains("*") {
                continue;
            }
            // Compare against declared urls in the same environment.
            // The match is prefix-based: a declared URL
            // `https://app.example.com` allows the origin
            // `https://app.example.com` (exact) — query string and
            // path differences are tolerated by the CORS layer, so
            // we compare scheme+host.
            let documented = declared_urls
                .iter()
                .filter(|u| u.environment == rule.environment)
                .any(|u| same_origin(&u.url, origin));
            if !documented {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "cors_origin_undocumented_diagnostics".to_owned(),
                    message: format!(
                        "`cors allow_origins {env} \"{origin}\"` does not match any `url <target> {env} ...` declaration. If the origin is a third-party caller, ignore; otherwise update `urls` so the source-of-truth stays consistent.",
                        env = rule.environment,
                    ),
                });
            }
        }
    }

    // CORS spec forbids `allow_origins "*"` with `allow_credentials true`.
    if has_wildcard && cors.allow_credentials {
        diagnostics.push(DoctorDiagnostic {
            path: app_manifest.path.clone(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "cors_credentials_wildcard_conflict_diagnostics".to_owned(),
            message: "`cors allow_origins ... \"*\"` cannot be combined with `allow_credentials true`. Per CORS spec, browsers reject the response. Either narrow the origin list or set `allow_credentials false`.".to_owned(),
        });
    }

    diagnostics
}

fn environments_summary(environments: &BTreeSet<&str>) -> String {
    if environments.is_empty() {
        "none declared".to_owned()
    } else {
        environments
            .iter()
            .map(|e| format!("`{e}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Compare two URLs by scheme + host (ignoring path, query, port
/// where absent). A declared `url` is the canonical reference; the
/// origin must match its scheme + authority for the CORS layer to
/// recognise it as the same browser origin.
fn same_origin(declared_url: &str, origin: &str) -> bool {
    let canon = |raw: &str| {
        let raw = raw.trim();
        // Strip path / query — keep scheme + authority only.
        let cut = raw
            .find("://")
            .and_then(|idx| {
                let after = &raw[idx + 3..];
                let tail_start = after.find('/').map(|p| idx + 3 + p);
                tail_start.map(|p| raw[..p].to_owned())
            })
            .unwrap_or_else(|| raw.to_owned());
        cut.trim_end_matches('/').to_owned()
    };
    canon(declared_url) == canon(origin)
}

// =============================================================================
// Observability bucket cycle row 36 — `app.logging` + `app.tracing`
//
// Six diagnostics check the closed catalogs and the sample-rate range:
//   - `app_logging_level_invalid_diagnostics`        error
//   - `app_logging_format_invalid_diagnostics`       error
//   - `app_logging_redact_unknown_diagnostics`       error
//   - `app_logging_sample_rate_range_diagnostics`    error
//   - `app_tracing_sample_rate_range_diagnostics`    error
//   - `app_tracing_exporter_unbound_diagnostics`     error
//
// The closed catalogs are deliberately small (4 levels, 2 formats, 2
// redact strategies). New catalog entries require a language cut.
//
// See `docs/proposals/bucket-observability-cycle.md` §3.1 §3.2.
// =============================================================================

/// Closed catalog shared with `event.trace <name> level <level>` in
/// row 37. Mirrors `log/slog` level discipline.
const LOG_LEVEL_CATALOG: &[&str] = &["debug", "info", "warn", "error"];

/// Closed catalog for `app.logging.format`. JSON for production
/// pipelines, text for local development.
const LOG_FORMAT_CATALOG: &[&str] = &["json", "text"];

/// Closed catalog for `app.logging.redact`. `pii` consumes `@pii.*`
/// tags; `none` opts out entirely.
const LOG_REDACT_CATALOG: &[&str] = &["pii", "none"];
const OBSERVABILITY_ERROR_SOURCE_CATALOG: &[&str] = &["dev", "staging", "prod"];

fn app_logging_tracing_diagnostics(
    app: Option<&DoctorAppManifest>,
    registry: Option<&ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let manifest_path = app_manifest.path.clone();

    if let Some(logging) = app_manifest.manifest.logging.as_ref() {
        if let Some(level) = logging.level.as_deref() {
            if !LOG_LEVEL_CATALOG.contains(&level) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_level_invalid_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.level {level}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_LEVEL_CATALOG),
                    ),
                });
            }
        }
        if let Some(format) = logging.format.as_deref() {
            if !LOG_FORMAT_CATALOG.contains(&format) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_format_invalid_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.format {format}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_FORMAT_CATALOG),
                    ),
                });
            }
        }
        if let Some(redact) = logging.redact.as_deref() {
            if !LOG_REDACT_CATALOG.contains(&redact) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_redact_unknown_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.redact {redact}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(LOG_REDACT_CATALOG),
                    ),
                });
            }
        }
        if let Some(rate) = logging.sample_rate {
            if !(0.0..=1.0).contains(&rate) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_logging_sample_rate_range_diagnostics".to_owned(),
                    message: format!(
                        "`app.logging.sample_rate {rate}` must be a float in `[0.0, 1.0]`. Use `1.0` for full capture and `0.0` to disable."
                    ),
                });
            }
        }
    }

    if let Some(tracing) = app_manifest.manifest.tracing.as_ref() {
        if let Some(rate) = tracing.sample_rate {
            if !(0.0..=1.0).contains(&rate) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_tracing_sample_rate_range_diagnostics".to_owned(),
                    message: format!(
                        "`app.tracing.sample_rate {rate}` must be a float in `[0.0, 1.0]`. Use `1.0` for full capture and `0.0` to disable."
                    ),
                });
            }
        }
        if let Some(exporter) = tracing.exporter.as_deref() {
            // The exporter slot must resolve to a `registry.capabilities
            // <name> tracing` entry (declared as the `name`, valued as
            // `tracing`) or to an integration. We accept any
            // `AppCapability` whose value is `tracing` *or* whose name
            // matches the exporter literal.
            let resolved = registry
                .map(|reg| {
                    reg.capabilities
                        .iter()
                        .any(|cap| cap.name == exporter && cap.value == "tracing")
                })
                .unwrap_or(false);
            if !resolved {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_tracing_exporter_unbound_diagnostics".to_owned(),
                    message: format!(
                        "`app.tracing.exporter {exporter}` does not resolve to a `registry.capabilities` entry of kind `tracing`. Declare it in `registry.capabilities`, or remove the line to let the runtime pick a default.",
                    ),
                });
            }
        }
    }

    diagnostics
}

fn catalog_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|i| format!("`{i}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

// =============================================================================
// Roadmap §1.2 — HTTP hygiene at the app level
//
// Three blocks lift to typed `AppCookie` / `AppProxy` / `AppLimits` and
// surface as one doctor diagnostic each:
//
//   - `app_cookie_contract_diagnostics`  — bad `same_site` token or
//     unparseable `max_age`.
//   - `app_proxy_contract_diagnostics`   — unparseable CIDR in
//     `trusted` or missing header name.
//   - `app_limits_contract_diagnostics`  — unparseable size / duration.
//
// All three are deliberately small contracts: the runtime owns the
// real validation (Go `net/url`, `time.ParseDuration`, `netip.Prefix`).
// Doctor catches the obvious typos at compile time so an LLM cold-
// reading the manifest sees the bar.
// =============================================================================

/// Closed catalog for `same_site`. CSRF policy per RFC 6265bis.
const COOKIE_SAME_SITE_CATALOG: &[&str] = &["lax", "strict", "none"];

fn app_cookie_contract_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(cookie) = app_manifest.manifest.cookie.as_ref() else {
        return diagnostics;
    };

    for profile in &cookie.profiles {
        if let Some(token) = profile.same_site.as_deref() {
            if !COOKIE_SAME_SITE_CATALOG.contains(&token) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_cookie_contract_diagnostics".to_owned(),
                    message: format!(
                        "`app.cookie.{name}.same_site {token}` is not in the closed catalog. Allowed values: {}.",
                        catalog_list(COOKIE_SAME_SITE_CATALOG),
                        name = profile.name,
                    ),
                });
            }
        }
        if let Some(raw) = profile.max_age.as_deref() {
            if !is_parseable_duration(raw) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_cookie_contract_diagnostics".to_owned(),
                    message: format!(
                        "`app.cookie.{name}.max_age \"{raw}\"` is not a parseable duration. Use forms like `\"7d\"`, `\"12h\"`, `\"30m\"`, `\"45s\"`.",
                        name = profile.name,
                    ),
                });
            }
        }
    }

    diagnostics
}

fn app_proxy_contract_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(proxy) = app_manifest.manifest.proxy.as_ref() else {
        return diagnostics;
    };

    for cidr in &proxy.trusted {
        if !is_parseable_cidr(cidr) {
            diagnostics.push(DoctorDiagnostic {
                path: app_manifest.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_proxy_contract_diagnostics".to_owned(),
                message: format!(
                    "`app.proxy.trusted \"{cidr}\"` is not a parseable CIDR. Use forms like `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `2001:db8::/32`.",
                ),
            });
        }
    }

    // A missing header name on any of the three slots is a contract
    // error: the runtime needs a token to look up. Empty strings reach
    // here when the author wrote `real_ip_header ""` or
    // `real_ip_header` (with no value).
    let header_slots: [(&str, Option<&String>); 3] = [
        ("real_ip_header", proxy.real_ip_header.as_ref()),
        ("forwarded_proto_header", proxy.forwarded_proto_header.as_ref()),
        ("forwarded_host_header", proxy.forwarded_host_header.as_ref()),
    ];
    for (slot, value) in header_slots {
        if let Some(name) = value {
            if name.trim().is_empty() {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_proxy_contract_diagnostics".to_owned(),
                    message: format!(
                        "`app.proxy.{slot}` requires a non-empty header name (e.g. `X-Forwarded-For`). Remove the line to let the runtime fall back to its default.",
                    ),
                });
            }
        }
    }

    diagnostics
}

fn app_limits_contract_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    let Some(limits) = app_manifest.manifest.limits.as_ref() else {
        return diagnostics;
    };

    let size_slots: [(&str, Option<&String>); 3] = [
        ("body_size", limits.body_size.as_ref()),
        ("header_size", limits.header_size.as_ref()),
        ("upload_size", limits.upload_size.as_ref()),
    ];
    for (slot, value) in size_slots {
        if let Some(raw) = value {
            if !is_parseable_size(raw) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_limits_contract_diagnostics".to_owned(),
                    message: format!(
                        "`app.limits.{slot} \"{raw}\"` is not a parseable size. Use forms like `\"512b\"`, `\"16kb\"`, `\"10mb\"`, `\"2gb\"`.",
                    ),
                });
            }
        }
    }

    if let Some(raw) = limits.timeout.as_ref() {
        if !is_parseable_duration(raw) {
            diagnostics.push(DoctorDiagnostic {
                path: app_manifest.path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_limits_contract_diagnostics".to_owned(),
                message: format!(
                    "`app.limits.timeout \"{raw}\"` is not a parseable duration. Use forms like `\"30s\"`, `\"5m\"`, `\"2h\"`.",
                ),
            });
        }
    }

    diagnostics
}

/// Liberal duration parser. Matches Go `time.ParseDuration` idioms
/// (`30s`, `5m`, `2h`) plus the day shorthand `7d` that the cookie /
/// session vocabulary already uses. The numeric prefix must be a
/// positive integer; the suffix is one of `ms | s | m | h | d`. This
/// stays in sync with the runtime parser at
/// `runtime/go/lazuli/http.go`.
fn is_parseable_duration(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    let suffixes = ["ms", "s", "m", "h", "d"];
    for suffix in suffixes {
        if let Some(head) = trimmed.strip_suffix(suffix) {
            if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Liberal size parser. Matches the common Go idiom (`512b`, `16kb`,
/// `10mb`, `2gb`). The numeric prefix must be a positive integer; the
/// suffix is one of `b | kb | mb | gb | tb`.
fn is_parseable_size(raw: &str) -> bool {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return false;
    }
    let suffixes = ["tb", "gb", "mb", "kb", "b"];
    for suffix in suffixes {
        if let Some(head) = trimmed.strip_suffix(suffix) {
            if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Liberal CIDR parser. Accepts IPv4 (`a.b.c.d/n`, `0 ≤ n ≤ 32`) and
/// IPv6 (`prefix::/n`, `0 ≤ n ≤ 128`). We don't need full RFC 4632
/// canonicalization at this layer — the Go runtime parses via
/// `netip.ParsePrefix` at wire time and surfaces real errors there.
/// This check just catches the obvious typo (missing slash, garbage
/// prefix length).
fn is_parseable_cidr(raw: &str) -> bool {
    let Some((addr, mask)) = raw.split_once('/') else {
        return false;
    };
    if addr.is_empty() || mask.is_empty() {
        return false;
    }
    let Ok(prefix_len) = mask.parse::<u32>() else {
        return false;
    };
    if addr.contains(':') {
        prefix_len <= 128
    } else {
        let octets: Vec<&str> = addr.split('.').collect();
        if octets.len() != 4 {
            return false;
        }
        for octet in &octets {
            let Ok(value) = octet.parse::<u32>() else {
                return false;
            };
            if value > 255 {
                return false;
            }
        }
        prefix_len <= 32
    }
}

// -----------------------------------------------------------------------------
// Roadmap §1.10 — `app.headers` + `secret_rotation` diagnostics
//
// Three new doctor codes lift production-grade security defaults from
// user-territory into the closed catalog:
//   - `headers-contract`: under the `production` security profile the
//     app must declare CSP, HSTS, X-Frame-Options, and
//     X-Content-Type-Options. Closed-catalog values also gate here.
//   - `secret-rotation-overlap-contract`: overlap > cadence makes the
//     profile contradictory — the runtime cannot finish a rollover
//     before it has begun.
//   - `secret-rotation-binding-unknown`: `app.encryption.key
//     @key.<scope> rotation_profile <name>` must reference a profile
//     declared on `registry.secret_rotations`.
//
// Runtime wire-of-X (actual header emission, secret rollover scheduling)
// stays for the follow-up cycle — these diagnostics validate the
// declarative shape today.
// -----------------------------------------------------------------------------

/// Required headers under the `production` security profile. Other
/// profiles emit a warning instead of an error; the catalog itself
/// stays the same so the message reads consistently across profiles.
const HEADERS_REQUIRED_IN_PRODUCTION: &[&str] = &[
    "csp",
    "hsts",
    "x_frame_options",
    "x_content_type_options",
];

fn app_headers_diagnostics(
    app: Option<&DoctorAppManifest>,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };

    let headers = app_manifest.manifest.headers.as_ref();
    let manifest_path = app_manifest.path.clone();

    // Production-profile completeness gate. Two distinct behaviours
    // keyed off whether the author has opted in by declaring ANY
    // `app.headers` content:
    //
    // 1. Author opted in (headers block present): every profile flags
    //    missing required slots (Strict/Prototype as warning, Production
    //    as error). The intent signal is unambiguous.
    //
    // 2. No headers block at all: only Production fires. Strict and
    //    Prototype defer — existing fixtures + feature-port flows must
    //    keep passing on the default Strict profile.
    let severity = match security_profile {
        SecurityProfile::Production => DoctorSeverity::Error,
        SecurityProfile::Strict | SecurityProfile::Prototype => DoctorSeverity::Warning,
    };
    let author_opted_in = headers.is_some();
    let production_gate = security_profile == SecurityProfile::Production;
    let mut missing: Vec<&'static str> = Vec::new();
    for required in HEADERS_REQUIRED_IN_PRODUCTION {
        let present = headers
            .map(|h| match *required {
                "csp" => h.csp.is_some(),
                "hsts" => h.hsts.is_some(),
                "x_frame_options" => h.x_frame_options.is_some(),
                "x_content_type_options" => h.x_content_type_options.is_some(),
                _ => true,
            })
            .unwrap_or(false);
        if !present {
            missing.push(*required);
        }
    }
    if !missing.is_empty() && (author_opted_in || production_gate) {
        diagnostics.push(DoctorDiagnostic {
            path: manifest_path.clone(),
            line: 1,
            column: 1,
            severity,
            code: "headers-contract".to_owned(),
            message: format!(
                "`app.headers` is missing the production-grade slots [{}]. Declare them under `app.lzi headers` so the runtime can emit the headers on every response.",
                missing.join(", "),
            ),
        });
    }

    // Closed-catalog value checks. These are independent of the
    // profile — `nosniff` is the only legal X-Content-Type-Options
    // token anywhere, etc.
    if let Some(headers) = headers {
        if let Some(value) = headers.x_content_type_options.as_deref() {
            if !ir::AppHeaders::is_x_content_type_options_known(value) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "headers-contract".to_owned(),
                    message: format!(
                        "`app.headers x_content_type_options {value}` is invalid — the only legal token is `nosniff`.",
                    ),
                });
            }
        }
        if let Some(value) = headers.x_frame_options.as_deref() {
            if !ir::AppHeaders::is_x_frame_options_known(value) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "headers-contract".to_owned(),
                    message: format!(
                        "`app.headers x_frame_options {value}` is invalid — closed catalog is `DENY`, `SAMEORIGIN`, or `ALLOW-FROM <uri>`.",
                    ),
                });
            }
        }
        if let Some(value) = headers.referrer_policy.as_deref() {
            if !ir::AppHeaders::is_referrer_policy_known(value) {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "headers-contract".to_owned(),
                    message: format!(
                        "`app.headers referrer_policy {value}` is invalid — closed catalog is [{}].",
                        ir::AppHeaders::REFERRER_POLICY_CATALOG.join(", "),
                    ),
                });
            }
        }
        if let Some(hsts) = headers.hsts.as_ref() {
            if hsts.max_age == 0 {
                diagnostics.push(DoctorDiagnostic {
                    path: manifest_path.clone(),
                    line: 1,
                    column: 1,
                    severity,
                    code: "headers-contract".to_owned(),
                    message: "`app.headers hsts max_age 0` disables HSTS — set a positive seconds value (typically 31536000 or higher) so the runtime can opt the browser into HTTPS-only.".to_owned(),
                });
            }
        }
    }

    diagnostics
}

fn secret_rotation_diagnostics(
    app: Option<&DoctorAppManifest>,
    registry: Option<&ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // overlap > cadence — invalid profile. The path is the registry
    // file because the profile is authored there.
    if let Some(registry) = registry {
        for rotation in &registry.secret_rotations {
            let cadence_secs = ir::security_duration::duration_seconds(&rotation.cadence);
            let overlap_secs = ir::security_duration::duration_seconds(&rotation.overlap);
            if let (Some(cadence), Some(overlap)) = (cadence_secs, overlap_secs) {
                if overlap > cadence {
                    // We don't have a registry path on AppRegistry; use
                    // the app path as a fallback because both files
                    // live next to each other and the message names the
                    // profile explicitly. If the app manifest is also
                    // missing, fall through to a synthesized
                    // `registry.lzi` path.
                    let path = app
                        .map(|a| a.path.clone())
                        .unwrap_or_else(|| PathBuf::from("registry.lzi"));
                    diagnostics.push(DoctorDiagnostic {
                        path,
                        line: 1,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "secret-rotation-overlap-contract".to_owned(),
                        message: format!(
                            "`secret_rotation {name}` declares overlap `{overlap_lit}` longer than cadence `{cadence_lit}`. Overlap is the grace window during which old + new secrets both pass; it must be strictly shorter than the cadence between rolls.",
                            name = rotation.name,
                            overlap_lit = rotation.overlap,
                            cadence_lit = rotation.cadence,
                        ),
                    });
                }
            }
        }
    }

    // `app.encryption.key @key.<scope> rotation_profile <name>`
    // referencing an undeclared profile.
    if let Some(app_manifest) = app {
        let declared: BTreeSet<&str> = registry
            .map(|r| {
                r.secret_rotations
                    .iter()
                    .map(|s| s.name.as_str())
                    .collect()
            })
            .unwrap_or_default();
        for binding in &app_manifest.manifest.encryption_bindings {
            let Some(profile) = binding.rotation_profile.as_deref() else {
                continue;
            };
            if !declared.contains(profile) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_manifest.path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "secret-rotation-binding-unknown".to_owned(),
                    message: format!(
                        "`encryption.key {scope} rotation_profile {profile}` references no `secret_rotation {profile}` entry in `registry.lzi`. Declare the profile or remove the reference.",
                        scope = binding.scope,
                    ),
                });
            }
        }
    }

    diagnostics
}

fn app_observability_diagnostics(app: Option<&DoctorAppManifest>) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    let Some(app_manifest) = app else {
        return diagnostics;
    };
    diagnostics.extend(
        check_observability_source_tokens(&app_manifest.manifest)
            .into_iter()
            .map(|mut diagnostic| {
                diagnostic.path = app_manifest.path.clone();
                diagnostic
            }),
    );
    diagnostics.extend(
        check_observability_panic_recover(&app_manifest.manifest)
            .into_iter()
            .map(|mut diagnostic| {
                diagnostic.path = app_manifest.path.clone();
                diagnostic
            }),
    );
    diagnostics
}

/// OBSERVABILITY-SOURCE-001 — error_source token outside closed catalog.
/// Allowed values: "dev", "staging", "prod".
fn check_observability_source_tokens(app: &AppManifest) -> Vec<DoctorDiagnostic> {
    let Some(observability) = app.observability.as_ref() else {
        return Vec::new();
    };
    observability
        .error_source
        .iter()
        .filter(|token| !OBSERVABILITY_ERROR_SOURCE_CATALOG.contains(&token.as_str()))
        .map(|token| DoctorDiagnostic {
            path: PathBuf::new(),
            line: 1,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "OBSERVABILITY-SOURCE-001".to_owned(),
            message: format!(
                "`app.observability.error_source {token}` is not in the closed catalog. Allowed values: {}.",
                catalog_list(OBSERVABILITY_ERROR_SOURCE_CATALOG),
            ),
        })
        .collect()
}

/// OBSERVABILITY-PANIC-001 — panic_recover=false outside `dev` environment.
/// Loud opt-out for prod; require explicit override.
fn check_observability_panic_recover(app: &AppManifest) -> Vec<DoctorDiagnostic> {
    let Some(observability) = app.observability.as_ref() else {
        return Vec::new();
    };
    if observability.panic_recover {
        return Vec::new();
    }
    let has_non_dev = app.environments.is_empty()
        || app
            .environments
            .iter()
            .any(|environment| environment != "dev");
    if !has_non_dev {
        return Vec::new();
    }
    vec![DoctorDiagnostic {
        path: PathBuf::new(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Warning,
        code: "OBSERVABILITY-PANIC-001".to_owned(),
        message: "`app.observability.panic_recover false` disables the runtime panic guard outside `dev`. Keep recovery enabled for staging/prod unless this is an explicit debug override.".to_owned(),
    }]
}

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
                });
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
    use lazuli_syntax::{
        PackageSkeleton, PermissionDeclAst, RoleDeclAst, parse_package_skeleton,
    };

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
        .find(|f| is_lzi_path(&f.path) && f.source.contains("\nrole ") || f.source.starts_with("role "))
        .or_else(|| files.iter().find(|f| is_lzi_path(&f.path)))
        .map(|f| f.path.clone())
        .unwrap_or_default();
    let issues_with_path: Vec<(PathBuf, _)> =
        issues.into_iter().map(|i| (representative.clone(), i)).collect();
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
                    || trimmed.starts_with("api "))
            {
                let name = trimmed
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_owned();
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
/// `query.lookup`, `query.sql` headers. Mirrors `collect_construct_lines`
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
            .or_else(|| trimmed.strip_prefix("query.sql "));
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
/// Phase L Tier 4a (commit `__TIER_4A__`) — `parse_feature_skeletons`
/// now lifts `defaults.tenancy`; this is a typed read of
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
                });
            }
        }
    }
    diagnostics
}

fn auth_session_ttl_seconds(raw: &str) -> Option<u64> {
    let trimmed = raw.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return None;
    }
    let digit_end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    if digit_end == 0 {
        return None;
    }
    let value = trimmed[..digit_end].parse::<u64>().ok()?;
    let unit = trimmed[digit_end..].trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 60 * 60,
        "d" | "day" | "days" => 24 * 60 * 60,
        _ => return None,
    };
    value.checked_mul(multiplier)
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

const TRACE_LEVEL_CATALOG: &[&str] = LOG_LEVEL_CATALOG;
const RESERVED_AUDIT_STREAMS: &[&str] = &["audit_log", "audit_stream"];

fn audit_event_health_diagnostics(
    files: &[DoctorFile],
    app: Option<&DoctorAppManifest>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // `event_group` names per feature (text-walk; the canonical-indent
    // slice does not yet cover commands/event_groups — see Phase L
    // row 24). Each entry is `(feature_name, event_group_name)`.
    let mut feature_event_groups: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
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

    // `audit ... emit_to <X>` checks. The slot lives at indent +2
    // below the `audit <fields>` line inside a command/job/webhook
    // body.
    for file in files {
        if !is_lzi_path(&file.path) {
            continue;
        }
        let lines: Vec<&str> = file.source.lines().collect();
        let mut current_feature: Option<String> = None;
        let mut audit_pending: Option<(usize, usize)> = None; // (line_index, indent of audit)
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
                continue;
            }
            // Track audit headers as `audit <fields...>` or bare `audit`
            // at indent 4 or 6 (command/job/webhook bodies).
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
                        if !resolved {
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

/// Render a `{name1, name2, ...}`-style list for diagnostic messages.
/// Empty sets render as `<none>` so the message stays unambiguous.
fn format_name_list(names: &BTreeSet<String>) -> String {
    if names.is_empty() {
        "<none>".to_owned()
    } else {
        names
            .iter()
            .map(|n| format!("`{n}`"))
            .collect::<Vec<_>>()
            .join(", ")
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
                    });
                    let _ = event_name; // pin for future per-event errors
                }
            }
        }
        i += 1;
    }
    diagnostics
}

fn payload_field_list(canonical: &BTreeSet<String>) -> String {
    let mut fields: Vec<&String> = canonical.iter().collect();
    fields.sort();
    fields
        .iter()
        .map(|f| format!("`{f}`"))
        .collect::<Vec<_>>()
        .join(", ")
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
        for finding in
            report::report_columns_empty_001::check(&feature_for_rules, &fact.path)
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
                code: report::report_columns_empty_001::Finding::CODE.to_owned(),
            });
        }
        for finding in
            report::report_signed_ttl_missing_001::check(&feature_for_rules, &fact.path)
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
            });
        }
        for finding in
            report::report_source_kind_001::check(&feature_for_rules, &fact.path)
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
                code: report::report_source_kind_001::Finding::CODE.to_owned(),
            });
        }
        for finding in report::report_policy_public_no_rate_limit_001::check(
            &feature_for_rules,
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
                code: report::report_policy_public_no_rate_limit_001::Finding::CODE.to_owned(),
            });
        }
        for finding in
            report::report_column_mismatch_001::check(&feature_for_rules, &fact.path)
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
                code: report::report_column_mismatch_001::Finding::CODE.to_owned(),
            });
        }
        for finding in
            report::report_path_collision_001::check(&feature_for_rules, &fact.path)
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
                code: report::report_path_collision_001::Finding::CODE.to_owned(),
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
            });
        }

        // AST-based rule (REPORT-FORMAT-UNKNOWN-001) reads the raw
        // ReportDecl text because lowering drops unknown format tokens.
        for finding in report::report_format_unknown_001::check(
            &fact.feature,
            &fact.report_decls,
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
                code: report::report_format_unknown_001::Finding::CODE.to_owned(),
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
fn make_synthetic_feature_for_reports(fact: &Tier3FeatureFacts) -> lazuli_ir::Feature {
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
        commands: Vec::new(),
        apis: fact.apis.clone(),
        records: fact.records.clone(),
        queries: fact.queries.clone(),
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
        previous_names: Vec::new(),
        span_ref: None,
    }
}

/// CL.C.4 — dispatch the four domain-model diagnostics over every
/// feature fact. Each rule consumes a synthesized `Feature` view (the
/// rules take `&Feature` to stay independent of the doctor scaffolding).
/// Line anchoring uses `aggregate_lines` for aggregate-scoped findings
/// and `feature_line` otherwise.
fn domain_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for fact in facts {
        if fact.aggregates.is_empty()
            && fact.resources.iter().all(|r| r.invariants.is_empty())
            && fact.resources.iter().all(|r| r.fields.iter().all(|f| !f.slug))
        {
            continue;
        }
        let feature = make_synthetic_feature_for_reports(fact);

        // AGGREGATE-ROOT-UNKNOWN
        for finding in domain::aggregate_root_unknown::check(&feature, &fact.path) {
            let line = fact
                .aggregate_lines
                .get(&finding.aggregate)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::aggregate_root_unknown::Finding::CODE.to_owned(),
            });
        }
        // AGGREGATE-CONTAINS-UNKNOWN
        for finding in domain::aggregate_contains_unknown::check(&feature, &fact.path) {
            let line = fact
                .aggregate_lines
                .get(&finding.aggregate)
                .copied()
                .unwrap_or(fact.feature_line);
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::aggregate_contains_unknown::Finding::CODE.to_owned(),
            });
        }
        // INVARIANT-PREDICATE-INVALID — covers both resource-scoped
        // and aggregate-scoped invariants.
        for finding in domain::invariant_predicate_invalid::check(&feature, &fact.path) {
            let line = match &finding.scope {
                domain::invariant_predicate_invalid::InvariantScope::Aggregate(a) => fact
                    .aggregate_lines
                    .get(a)
                    .copied()
                    .unwrap_or(fact.feature_line),
                domain::invariant_predicate_invalid::InvariantScope::Resource(_) => {
                    fact.feature_line
                }
            };
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: domain::invariant_predicate_invalid::Finding::CODE.to_owned(),
            });
        }
        // SLUG-UNIQUENESS-IMPLICIT — warning, not error.
        for finding in domain::slug_uniqueness_implicit::check(&feature, &fact.path) {
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: domain::slug_uniqueness_implicit::Finding::CODE.to_owned(),
            });
        }
    }
    diagnostics
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
                    });
                }
            }
        }
    }

    diagnostics
}

fn mime_sets_intersect(left: &[lazuli_ir::MimeType], right: &[lazuli_ir::MimeType]) -> bool {
    for l in left {
        for r in right {
            if mime_matches(l, r) {
                return true;
            }
        }
    }
    false
}

fn mime_matches(left: &lazuli_ir::MimeType, right: &lazuli_ir::MimeType) -> bool {
    let family_ok = left.family == right.family || left.family == "*" || right.family == "*";
    let subtype_ok = left.subtype == right.subtype || left.subtype == "*" || right.subtype == "*";
    family_ok && subtype_ok
}

fn format_visibility(v: lazuli_ir::FileVisibility) -> &'static str {
    match v {
        lazuli_ir::FileVisibility::Public => "public",
        lazuli_ir::FileVisibility::Private => "private",
        lazuli_ir::FileVisibility::Signed => "signed",
    }
}

fn format_accept_list(accept: &[lazuli_ir::MimeType]) -> String {
    accept
        .iter()
        .map(|m| format!("{}/{}", m.family, m.subtype))
        .collect::<Vec<_>>()
        .join("|")
}

// =============================================================================
// OpenAPI bucket cycle (row 48) — `deprecated_*` diagnostics.
// =============================================================================

/// Row 48 — emits OpenAPI-related diagnostics:
/// `deprecated-replacement-unknown`, `deprecated_sunset_date_invalid`,
/// `deprecated-sunset-past`, `deprecated-no-replacement`,
/// `openapi_text_pattern_api_block`,
/// `api_changelog_breaking_change` (the last only when invoked from the
/// changelog pipeline; doctor surfaces a guard noop). See
/// `docs/proposals/bucket-openapi-cycle.md` §Doctor/LSP.
fn openapi_deprecated_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let mut commands_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut apis_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for feature in facts {
        let command_set = commands_by_feature
            .entry(feature.feature.as_str())
            .or_default();
        for c in &feature.commands {
            command_set.insert(c.name.as_str());
        }
        let api_set = apis_by_feature.entry(feature.feature.as_str()).or_default();
        for api in &feature.apis {
            api_set.insert(api.name.as_str());
        }
    }

    let today_pivot = openapi_today_pivot();

    for feature in facts {
        for command in &feature.commands {
            let Some(dep) = &command.deprecated else {
                continue;
            };
            let line = feature
                .command_lines
                .get(&command.name)
                .copied()
                .unwrap_or(feature.feature_line);
            deprecated_callable_diagnostics(
                &mut diagnostics,
                feature,
                "command",
                &command.name,
                line,
                dep,
                &commands_by_feature,
                &apis_by_feature,
                today_pivot,
            );
        }
        for api in &feature.apis {
            let Some(dep) = &api.deprecated else {
                continue;
            };
            let line = feature
                .api_lines
                .get(&api.name)
                .copied()
                .unwrap_or(feature.feature_line);
            deprecated_callable_diagnostics(
                &mut diagnostics,
                feature,
                "api",
                &api.name,
                line,
                dep,
                &commands_by_feature,
                &apis_by_feature,
                today_pivot,
            );
        }
    }

    // 4) `openapi_text_pattern_api_block` — surface once per unique
    // text-pattern api name across the package. The IR-lifted `Api`s
    // shadow text-pattern entries; subtract them so the warning only
    // fires for genuinely un-lifted authoring sites.
    let typed_api_names: BTreeSet<&str> = facts
        .iter()
        .flat_map(|f| f.apis.iter().map(|a| a.name.as_str()))
        .collect();
    let mut surfaced: BTreeSet<String> = BTreeSet::new();
    for feature in facts {
        for name in &feature.api_names_text_pattern {
            if typed_api_names.contains(name.as_str()) {
                continue;
            }
            if !surfaced.insert(name.clone()) {
                continue;
            }
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line: feature.feature_line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "openapi_text_pattern_api_block".to_owned(),
                message: format!(
                    "api `{}` is text-pattern; OpenAPI emission falls back to a stub with `x-lazuli-text-pattern-skip: true`. Lift to typed IR via Phase L Tier 4.",
                    name
                ),
            });
        }
    }

    diagnostics
}

#[allow(clippy::too_many_arguments)]
fn deprecated_callable_diagnostics(
    diagnostics: &mut Vec<DoctorDiagnostic>,
    feature: &Tier3FeatureFacts,
    kind: &str,
    name: &str,
    line: usize,
    dep: &lazuli_ir::Deprecation,
    commands_by_feature: &BTreeMap<&str, BTreeSet<&str>>,
    apis_by_feature: &BTreeMap<&str, BTreeSet<&str>>,
    today_pivot: (u16, u8, u8),
) {
    if dep.replacement.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "deprecated-no-replacement".to_owned(),
            message: format!(
                "{kind} `{name}` is deprecated without a replacement; declare `replacement {kind}.<name>` when a successor exists."
            ),
        });
    } else if let Some(replacement) = &dep.replacement {
        match replacement {
            lazuli_ir::DeprecationReplacement::LocalCommand(target) => {
                push_unknown_replacement_if_missing(
                    diagnostics,
                    feature,
                    kind,
                    name,
                    line,
                    "command",
                    feature.feature.as_str(),
                    target,
                    commands_by_feature,
                );
            }
            lazuli_ir::DeprecationReplacement::LocalApi(target) => {
                push_unknown_replacement_if_missing(
                    diagnostics,
                    feature,
                    kind,
                    name,
                    line,
                    "api",
                    feature.feature.as_str(),
                    target,
                    apis_by_feature,
                );
            }
            lazuli_ir::DeprecationReplacement::Qualified(q) => {
                push_unknown_replacement_if_missing(
                    diagnostics,
                    feature,
                    kind,
                    name,
                    line,
                    "command",
                    q.feature.as_deref().unwrap_or(feature.feature.as_str()),
                    &q.name,
                    commands_by_feature,
                );
            }
            lazuli_ir::DeprecationReplacement::QualifiedApi(q) => {
                push_unknown_replacement_if_missing(
                    diagnostics,
                    feature,
                    kind,
                    name,
                    line,
                    "api",
                    q.feature.as_deref().unwrap_or(feature.feature.as_str()),
                    &q.name,
                    apis_by_feature,
                );
            }
            lazuli_ir::DeprecationReplacement::Url(url) => {
                let cleaned = url.trim();
                if !(cleaned.starts_with("http://") || cleaned.starts_with("https://"))
                    || cleaned.len() < "https://x".len()
                {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "deprecated-replacement-unknown".to_owned(),
                        message: format!(
                            "{kind} `{name}`.deprecated.replacement `{url}` does not resolve: url malformed."
                        ),
                    });
                }
            }
        }
    }

    if let Some(sunset) = &dep.sunset {
        match parse_iso_date(sunset) {
            None => diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "deprecated_sunset_date_invalid".to_owned(),
                message: format!(
                    "{kind} `{name}`.deprecated.sunset `{sunset}` is not a valid ISO-8601 date (`YYYY-MM-DD`)."
                ),
            }),
            Some(date) if date < today_pivot => diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Info,
                code: "deprecated-sunset-past".to_owned(),
                message: format!(
                    "{kind} `{name}`.deprecated.sunset `{sunset}` is in the past; consumers should expect this endpoint to be removed soon."
                ),
            }),
            Some(_) => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push_unknown_replacement_if_missing(
    diagnostics: &mut Vec<DoctorDiagnostic>,
    feature: &Tier3FeatureFacts,
    kind: &str,
    name: &str,
    line: usize,
    target_kind: &str,
    target_feature: &str,
    target_name: &str,
    index: &BTreeMap<&str, BTreeSet<&str>>,
) {
    let resolves = index
        .get(target_feature)
        .map(|set| set.contains(target_name))
        .unwrap_or(false);
    if resolves {
        return;
    }
    diagnostics.push(DoctorDiagnostic {
        path: feature.path.clone(),
        line,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "deprecated-replacement-unknown".to_owned(),
        message: format!(
            "{kind} `{name}`.deprecated.replacement `{target_feature}.{target_kind}.{target_name}` does not resolve."
        ),
    });
}

/// Parse `YYYY-MM-DD` into a `(year, month, day)` triple. Returns `None`
/// if the format is wrong or numbers are out of plausible range. Doctor
/// uses this for `deprecated_sunset_*` checks; the comparison against
/// `today_pivot` is lexical (the tuple sorts as if it were a real date
/// because each component is fixed-width).
fn parse_iso_date(s: &str) -> Option<(u16, u8, u8)> {
    let trimmed = s.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: u16 = trimmed[0..4].parse().ok()?;
    let month: u8 = trimmed[5..7].parse().ok()?;
    let day: u8 = trimmed[8..10].parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    if !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

/// Calendar pivot the OpenAPI `sunset_in_past` rule compares against.
/// The runtime context exposes no `chrono` dependency; we anchor the
/// pivot at the current Lazuli development date so the diagnostic is
/// deterministic across runs. Bump alongside the canonical fixture
/// each cycle; in practice the day-of-month precision is sufficient.
fn openapi_today_pivot() -> (u16, u8, u8) {
    (2026, 5, 11)
}

// =============================================================================
// Cache bucket cycle (row 51 + CL.C.3) — `cache_*` diagnostics.
// =============================================================================

/// Row 51 + CL.C.3 — emits Cache-related diagnostics:
/// `cache_ttl_unit_invalid`, `cache_invalidates_target_unresolved`,
/// `cache_tags_referenced_but_undeclared`, `cache_namespace_collision`,
/// `cache_capability_undeclared` (legacy bucket cycle), plus the
/// CL.C.3 trio:
///  * `cache-profile-unknown` — query authored `cache <name>` where no
///    matching `cache <name>` profile exists in the feature.
///  * `cache-tag-unknown` — an `invalidates tag:<label>` references a
///    tag not declared by any cache (inline or profile) anywhere.
///  * `cache-ttl-contract` — invalid TTL literal, `stale_while_revalidate`
///    larger than `ttl`, or `sliding true` without `ttl`.
///
/// See `docs/proposals/bucket-cache-cycle.md` §Doctor/LSP +
/// `docs/proposals/bucket-cache-scope.md` (CL.C.3 row).
fn cache_diagnostics(
    facts: &[Tier3FeatureFacts],
    registry: Option<&lazuli_ir::AppRegistry>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Build cross-feature indices.
    let mut queries_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut all_tags: BTreeSet<String> = BTreeSet::new();
    let mut namespace_owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut any_query_has_cache = false;
    for feature in facts {
        let set = queries_by_feature
            .entry(feature.feature.as_str())
            .or_default();
        for q in &feature.queries {
            let (name, cache) = query_name_and_cache(q);
            set.insert(name);
            if let Some(cache) = cache {
                any_query_has_cache = true;
                for tag in &cache.tags {
                    all_tags.insert(tag.clone());
                }
                if let Some(ns) = &cache.namespace {
                    namespace_owners
                        .entry(ns.clone())
                        .or_default()
                        .insert(feature.feature.clone());
                }
            }
        }
        // CL.C.3 — feature-level cache profiles also contribute to the
        // tag and namespace indexes. A profile referenced by a query is
        // a cached query; queue the capability check too.
        for profile in &feature.caches {
            for tag in &profile.tags {
                all_tags.insert(tag.clone());
            }
            if let Some(ns) = &profile.namespace {
                namespace_owners
                    .entry(ns.clone())
                    .or_default()
                    .insert(feature.feature.clone());
            }
        }
    }

    // `cache_capability_undeclared`: if any query carries `cache` but
    // the registry has no `cache <name>` capability, surface once at
    // the registry file (or, if no registry parsed, at the first
    // offending query).
    if any_query_has_cache {
        let has_cache_cap = registry
            .map(|r| {
                r.capabilities
                    .iter()
                    .any(|cap| cap.name.eq_ignore_ascii_case("cache"))
            })
            .unwrap_or(false);
        if !has_cache_cap {
            // Anchor at the first feature with a cache; the LSP rule on
            // `registry.lzi` is the file-local surface.
            if let Some(feature) = facts.iter().find(|f| {
                f.queries
                    .iter()
                    .any(|q| query_name_and_cache(q).1.is_some())
            }) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "cache_capability_undeclared".to_owned(),
                    message: "`cache` block requires a `cache <name>` capability in `registry.lzi` but none is declared. Add `cache <name>` to `registry.capabilities`.".to_owned(),
                });
            }
        }
    }

    // `cache_namespace_collision`: a namespace label declared by two
    // distinct features. One feature owning the namespace is fine.
    for (ns, owners) in &namespace_owners {
        if owners.len() >= 2 {
            // Emit one warning per owning feature.
            let owners_list: Vec<&str> = owners.iter().map(String::as_str).collect();
            for feature in facts {
                if !owners.contains(&feature.feature) {
                    continue;
                }
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature.feature_line,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "cache_namespace_collision".to_owned(),
                    message: format!(
                        "`cache namespace {}` is declared by queries in {}. Cross-feature namespace aliasing is unusual; rename one to avoid accidental cache-key collisions.",
                        ns,
                        owners_list.join(" and ")
                    ),
                });
            }
        }
    }

    for feature in facts {
        // `cache_ttl_unit_invalid` is a typed promotion: when the parser
        // produced `CacheTtl::Quoted` with a payload that does *not* look
        // like a typed duration (closed catalog: `s|m|h|d`), emit. Empty
        // payload is also rejected. The literal form is type-safe by
        // construction so it never trips.
        for query in &feature.queries {
            let (name, cache) = query_name_and_cache(query);
            let Some(cache) = cache else { continue };
            let line = feature
                .query_lines
                .get(name)
                .copied()
                .unwrap_or(feature.feature_line);

            if let lazuli_ir::CacheTtl::Quoted(prose) = &cache.ttl {
                let cleaned = prose.trim();
                if cleaned.is_empty() {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "cache_ttl_unit_invalid".to_owned(),
                        message: format!(
                            "`cache ttl` on query `{}` is empty. Use a typed duration (`<int>s|m|h|d`) or non-empty quoted prose.",
                            name
                        ),
                    });
                }
            }

            // `cache_invalidates_target_unresolved` — defer to command
            // walk below.

            // Tag-side `cache_tags_*` is handled at the invalidates
            // walk below.
        }

        for command in &feature.commands {
            for invalidate in &command.invalidates {
                let line = feature
                    .command_lines
                    .get(&command.name)
                    .copied()
                    .unwrap_or(feature.feature_line);

                // The parser captures `invalidates query.<name>` as a
                // qualified pair where the literal prefix `query` lands
                // in `query.feature`. That's a shorthand for "same
                // feature, kind=query, name=<X>". When the qualifier is
                // literally `query`, resolve `<name>` against the
                // current feature's queries. Any other qualifier is a
                // genuine cross-feature reference (`<feature>.<query>`).
                let raw_feature = invalidate.query.feature.as_deref();
                let raw_name = invalidate.query.name.as_str();
                let (target_feature, target_name) = match raw_feature {
                    Some("query") => (feature.feature.as_str(), raw_name),
                    Some(other) => (other, raw_name),
                    None => (feature.feature.as_str(), raw_name),
                };

                let resolves = queries_by_feature
                    .get(target_feature)
                    .map(|set| set.contains(target_name))
                    .unwrap_or(false);
                if !resolves {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "cache_invalidates_target_unresolved".to_owned(),
                        message: format!(
                            "`invalidates query.{}` in command `{}` does not resolve: query `{}` not found in feature `{}`.",
                            target_name, command.name, target_name, target_feature
                        ),
                    });
                }
            }
        }

        // `cache_tags_referenced_but_undeclared` — placeholder: today
        // the parser does not lift `invalidates tag:<label>` into a
        // typed `InvalidationTarget::Tag` variant (it captures only
        // `query.<name>(...)`). When the parser surfaces tag targets,
        // this branch tests `all_tags.contains(label)`. The rule stays
        // wired so the LSP fallback continues to cover the surface.

        // CL.C.3 — `cache-profile-unknown`: a query referenced
        // `cache <name>` but no feature-level `cache <name>` profile
        // matches.
        let profile_names: BTreeSet<&str> =
            feature.caches.iter().map(|c| c.name.as_str()).collect();
        for query in &feature.queries {
            let (qname, cache) = query_name_and_cache(query);
            let Some(cache) = cache else { continue };
            let Some(profile_name) = cache.profile_ref.as_deref() else {
                continue;
            };
            if profile_names.contains(profile_name) {
                continue;
            }
            let line = feature
                .query_lines
                .get(qname)
                .copied()
                .unwrap_or(feature.feature_line);
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "cache-profile-unknown".to_owned(),
                message: format!(
                    "`cache {profile_name}` on query `{qname}` does not resolve: no feature-level `cache {profile_name}` profile declared in feature `{}`.",
                    feature.feature
                ),
            });
        }

        // CL.C.3 — `cache-ttl-contract`: closed-catalog TTL shape
        // invariants on feature-level profiles.
        for profile in &feature.caches {
            let line = feature
                .cache_lines
                .get(&profile.name)
                .copied()
                .unwrap_or(feature.feature_line);

            // (1) Empty quoted prose TTL.
            if let lazuli_ir::CacheTtl::Quoted(prose) = &profile.ttl {
                if prose.trim().is_empty() {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "cache-ttl-contract".to_owned(),
                        message: format!(
                            "`cache {}` has an empty `ttl`. Use a typed duration (`<int>s|m|h|d`) or non-empty quoted prose.",
                            profile.name
                        ),
                    });
                }
            }

            // (2) SWR > TTL.
            if let Some(swr) = &profile.stale_while_revalidate {
                if let (Some(ttl_secs), Some(swr_secs)) = (
                    cache_ttl_as_seconds(&profile.ttl),
                    cache_ttl_as_seconds(swr),
                ) {
                    if swr_secs > ttl_secs {
                        diagnostics.push(DoctorDiagnostic {
                            path: feature.path.clone(),
                            line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "cache-ttl-contract".to_owned(),
                            message: format!(
                                "`cache {}` has `stale_while_revalidate` ({swr_secs}s) larger than `ttl` ({ttl_secs}s). SWR must extend the freshness window, not invert it.",
                                profile.name
                            ),
                        });
                    }
                }
            }

            // (3) `sliding true` without a typed TTL literal.
            if profile.sliding == Some(true)
                && !matches!(profile.ttl, lazuli_ir::CacheTtl::Literal(_))
            {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "cache-ttl-contract".to_owned(),
                    message: format!(
                        "`cache {}` declares `sliding true` but its `ttl` is not a typed duration literal. Use `<int>s|m|h|d` so the runtime can slide the window deterministically.",
                        profile.name
                    ),
                });
            }
        }

        // CL.C.3 — `cache-tag-unknown`: a query carrying inline tags
        // names a tag that no other site declares.
        for query in &feature.queries {
            let (qname, cache) = query_name_and_cache(query);
            let Some(cache) = cache else { continue };
            if cache.tags.is_empty() {
                continue;
            }
            let line = feature
                .query_lines
                .get(qname)
                .copied()
                .unwrap_or(feature.feature_line);
            for tag in &cache.tags {
                let declarers = facts.iter().fold(0usize, |acc, f| {
                    let from_queries = f
                        .queries
                        .iter()
                        .filter(|q| {
                            query_name_and_cache(q)
                                .1
                                .map(|c| c.tags.iter().any(|t| t == tag))
                                .unwrap_or(false)
                        })
                        .count();
                    let from_profiles = f
                        .caches
                        .iter()
                        .filter(|p| p.tags.iter().any(|t| t == tag))
                        .count();
                    acc + from_queries + from_profiles
                });
                if declarers <= 1 {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Warning,
                        code: "cache-tag-unknown".to_owned(),
                        message: format!(
                            "`cache tags {tag}` on query `{qname}` is the only declarer of `{tag}`. Either declare it on another query/profile or remove it — tags without a second declarer cannot fan out invalidation.",
                        ),
                    });
                }
            }
        }
    }

    diagnostics
}

/// Helper — destructure an `ir::Query` into `(name, Option<&QueryCache>)`.
/// Both `ListQuery` and `SqlQuery` carry `cache`; `LookupQuery` does not.
fn query_name_and_cache(q: &lazuli_ir::Query) -> (&str, Option<&lazuli_ir::QueryCache>) {
    match q {
        lazuli_ir::Query::List(l) => (l.name.as_str(), l.cache.as_ref()),
        lazuli_ir::Query::Lookup(l) => (l.name.as_str(), None),
        lazuli_ir::Query::Sql(s) => (s.name.as_str(), s.cache.as_ref()),
    }
}

/// CL.C.3 — convert a `CacheTtl` to seconds for ordering comparisons
/// (`stale_while_revalidate` <= `ttl`). Returns `None` for quoted prose
/// (adapter-parsed; we don't second-guess the runtime there).
fn cache_ttl_as_seconds(ttl: &lazuli_ir::CacheTtl) -> Option<u64> {
    match ttl {
        lazuli_ir::CacheTtl::Literal(lit) => Some(match lit {
            lazuli_ir::CacheTtlLiteral::Seconds(n) => *n as u64,
            lazuli_ir::CacheTtlLiteral::Minutes(n) => *n as u64 * 60,
            lazuli_ir::CacheTtlLiteral::Hours(n) => *n as u64 * 60 * 60,
            lazuli_ir::CacheTtlLiteral::Days(n) => *n as u64 * 60 * 60 * 24,
        }),
        lazuli_ir::CacheTtl::Quoted(_) => None,
    }
}

// =============================================================================
// i18n bucket cycle (row 54) — 15 locale/translation diagnostics.
// =============================================================================

const LOCALE_NEGOTIATE_SOURCES: &[&str] = &[
    "accept_language",
    "query_param",
    "cookie",
    "user_profile",
    "subdomain",
];

const LOCALE_NEGOTIATE_STRATEGIES: &[&str] = &["best_match", "prefix_match", "exact_match"];

const CLDR_PLURAL_ARMS: &[&str] = &["zero", "one", "two", "few", "many", "other"];

/// Row 54 — emits up to 15 i18n diagnostics. See
/// `docs/proposals/bucket-i18n-cycle.md` §Doctor/LSP.
fn i18n_diagnostics(
    facts: &[Tier3FeatureFacts],
    app: Option<&DoctorAppManifest>,
    files: &[DoctorFile],
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    let Some(app_facts) = app else {
        return diagnostics;
    };
    let app_locale = app_facts.manifest.locale.as_ref();
    let supported: BTreeSet<String> = app_locale
        .map(|l| l.supported.iter().cloned().collect())
        .unwrap_or_default();
    let default_locale = app_locale.map(|l| l.default.as_str()).unwrap_or("");
    let app_path = app_facts.path.clone();

    // ---- App-level: locale default / supported / fallback ----
    if let Some(locale) = app_locale {
        // `app_locale_default_unsupported`.
        if !locale.default.is_empty() && !supported.contains(&locale.default) {
            diagnostics.push(DoctorDiagnostic {
                path: app_path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_locale_default_unsupported".to_owned(),
                message: format!(
                    "`app.locale.default` `{}` must appear in `supported`.",
                    locale.default
                ),
            });
        }

        // Build adjacency for cycle + unknown-tag checks.
        let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for fb in &locale.fallbacks {
            if !supported.contains(&fb.from) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_locale_fallback_unknown_source".to_owned(),
                    message: format!(
                        "fallback `{} -> {}` source `{}` is not in `app.locale.supported`.",
                        fb.from, fb.to, fb.from
                    ),
                });
            }
            if !supported.contains(&fb.to) {
                diagnostics.push(DoctorDiagnostic {
                    path: app_path.clone(),
                    line: 1,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "app_locale_fallback_unknown_dest".to_owned(),
                    message: format!(
                        "fallback `{} -> {}` destination `{}` is not in `app.locale.supported`.",
                        fb.from, fb.to, fb.to
                    ),
                });
            }
            graph
                .entry(fb.from.clone())
                .or_default()
                .push(fb.to.clone());
        }

        // Cycle detection via DFS.
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut on_stack: BTreeSet<String> = BTreeSet::new();
        let mut found_cycle: Option<String> = None;
        let nodes: Vec<String> = graph.keys().cloned().collect();
        for start in nodes {
            if found_cycle.is_some() {
                break;
            }
            if visited.contains(&start) {
                continue;
            }
            // Iterative DFS with path stack.
            let mut stack: Vec<(String, usize)> = vec![(start.clone(), 0)];
            on_stack.insert(start.clone());
            visited.insert(start.clone());
            while let Some((node, idx)) = stack.last().cloned() {
                let nbrs = graph.get(&node).cloned().unwrap_or_default();
                if idx >= nbrs.len() {
                    on_stack.remove(&node);
                    stack.pop();
                    continue;
                }
                stack.last_mut().unwrap().1 = idx + 1;
                let next = nbrs[idx].clone();
                if on_stack.contains(&next) {
                    found_cycle = Some(format!(
                        "{} -> {} -> ... -> {}",
                        stack
                            .iter()
                            .map(|(n, _)| n.as_str())
                            .collect::<Vec<_>>()
                            .join(" -> "),
                        next,
                        next
                    ));
                    break;
                }
                if !visited.contains(&next) {
                    visited.insert(next.clone());
                    on_stack.insert(next.clone());
                    stack.push((next, 0));
                }
            }
        }
        if let Some(cycle) = found_cycle {
            diagnostics.push(DoctorDiagnostic {
                path: app_path.clone(),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_locale_fallback_cycle".to_owned(),
                message: format!("fallback chain creates a cycle: `{}`.", cycle),
            });
        }
    }

    // ---- Per-runtime-unit / per-api `locale_negotiate` ----
    for unit in &app_facts.manifest.runtime {
        if let Some(ln) = &unit.locale_negotiate {
            check_locale_negotiate(ln, &supported, &app_path, 1, &mut diagnostics);
        }
    }

    // ---- Per-feature translation ----
    for feature in facts {
        if let Some(api) = feature.apis.first() {
            // No-op placeholder: api-level locale_negotiate doctor rules
            // would attach here once the api facts surface line maps.
            // Today the rules use the api block itself; we walk
            // `feature.apis` instead.
            let _ = api;
        }
        for api in &feature.apis {
            if let Some(ln) = &api.locale_negotiate {
                check_locale_negotiate(
                    ln,
                    &supported,
                    &feature.path,
                    feature.feature_line,
                    &mut diagnostics,
                );
            }
        }

        let Some(translation) = &feature.translation else {
            continue;
        };
        let tline = feature.translation_line;

        if !translation.catalog.contains("<locale>") {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line: tline,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "translation_catalog_path_missing".to_owned(),
                message: format!(
                    "translation catalog path `{}` should contain a `<locale>` placeholder so the runtime can load per-locale files.",
                    translation.catalog
                ),
            });
        }

        // Build set of declared key names + referenced key set is below.
        let declared: BTreeSet<&str> = translation.keys.iter().map(|k| k.name.as_str()).collect();

        for key in &translation.keys {
            let mut variant_locales: BTreeSet<&str> = BTreeSet::new();
            for variant in &key.variants {
                variant_locales.insert(variant.locale.as_str());
                if !supported.contains(&variant.locale) && !supported.is_empty() {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: tline,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "translation_locale_unsupported".to_owned(),
                        message: format!(
                            "translation key `{}.{}` declares variant `{}` outside `app.locale.supported`.",
                            feature.feature, key.name, variant.locale
                        ),
                    });
                }
            }
            if !default_locale.is_empty() && !variant_locales.contains(default_locale) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: tline,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "translation_locale_missing_for_default".to_owned(),
                    message: format!(
                        "translation key `{}.{}` is missing a variant for default locale `{}`.",
                        feature.feature, key.name, default_locale
                    ),
                });
            }
            for tag in &supported {
                if tag == default_locale {
                    continue;
                }
                if !variant_locales.contains(tag.as_str()) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: tline,
                        column: 1,
                        severity: DoctorSeverity::Warning,
                        code: "translation_locale_missing_for_supported".to_owned(),
                        message: format!(
                            "translation key `{}.{}` is missing a variant for supported locale `{}`.",
                            feature.feature, key.name, tag
                        ),
                    });
                }
            }
            for plural in &key.plurals {
                if !CLDR_PLURAL_ARMS.contains(&plural.arm.as_str()) {
                    diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line: tline,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "cldr_plural_arm_invalid".to_owned(),
                        message: format!(
                            "translation key `{}.{}` plural arm `{}` is not a CLDR category: zero|one|two|few|many|other.",
                            feature.feature, key.name, plural.arm
                        ),
                    });
                }
            }
        }

        // `translation_key_unresolved`: any `@translation.<key>` in a
        // rule message that does not resolve to a declared key.
        // `translation_key_unused`: any declared key never referenced
        // anywhere in the feature.
        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for command in &feature.commands {
            // commands today carry no rule-message slot; skip.
            let _ = command;
        }
        // Walk file source for `message @translation.<key>` references
        // since `Rule.message_ref` is exposed via the analyzer's lifted
        // Tier 4d resource rules; the legacy rule walker still owns the
        // file-local lift. Doctor uses text-pattern here to bridge the
        // gap until the rule lift lands. We read from the in-memory
        // package files first (so tests work without filesystem
        // round-trips) and fall back to the filesystem.
        let source = files
            .iter()
            .find(|f| f.path == feature.path)
            .map(|f| f.source.clone())
            .or_else(|| std::fs::read_to_string(&feature.path).ok());
        if let Some(text) = source {
            for line in text.lines() {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed.strip_prefix("message @translation.") {
                    let key = rest.split_whitespace().next().unwrap_or("");
                    if !key.is_empty() {
                        referenced.insert(key.to_owned());
                        if !declared.contains(key) {
                            diagnostics.push(DoctorDiagnostic {
                                path: feature.path.clone(),
                                line: tline,
                                column: 1,
                                severity: DoctorSeverity::Error,
                                code: "rule_message_ref_unresolved".to_owned(),
                                message: format!(
                                    "`@translation.{}` in feature `{}` does not resolve. Declared keys: {}.",
                                    key,
                                    feature.feature,
                                    declared
                                        .iter()
                                        .copied()
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ),
                            });
                        }
                    }
                }
            }
        }
        for key in &translation.keys {
            if !referenced.contains(&key.name) {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: tline,
                    column: 1,
                    severity: DoctorSeverity::Warning,
                    code: "translation_key_unused".to_owned(),
                    message: format!(
                        "translation key `{}.{}` is declared but never referenced via `@translation.<key>`.",
                        feature.feature, key.name
                    ),
                });
            }
        }

        // `notification_template_placeholder_unknown`: a notification
        // template path containing `<locale>` requires a mounted
        // `locale_negotiate`. The fixture authors templates via
        // `notification template "./outreach/...mjml"` in the IR
        // notifications slot; doctor checks each.
        let mount_count = app_facts
            .manifest
            .runtime
            .iter()
            .filter(|u| u.locale_negotiate.is_some())
            .count();
        for notification in &feature.notifications {
            if notification.template.contains("<locale>") && mount_count == 0 {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line: feature
                        .notification_lines
                        .get(&notification.name)
                        .copied()
                        .unwrap_or(feature.feature_line),
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "notification_template_placeholder_unknown".to_owned(),
                    message: format!(
                        "notification `{}` template path contains `<locale>` but no `locale_negotiate` is mounted in `app.runtime`.",
                        notification.name
                    ),
                });
            }
        }

        // `translation_catalog_path_missing` (filesystem check) is
        // deferred to `lazuli translate extract --check`. Doctor does
        // not touch the filesystem.
    }

    diagnostics
}

/// i18n bucket cycle helper — validate a `LocaleNegotiate` block.
/// Emits `locale_negotiate_source_invalid`,
/// `locale_negotiate_strategy_invalid`, and reuses
/// `app_locale_fallback_unknown_dest` when the fallback tag is missing.
fn check_locale_negotiate(
    ln: &lazuli_ir::LocaleNegotiate,
    supported: &BTreeSet<String>,
    path: &Path,
    line: usize,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    if let Some(source) = &ln.source {
        if !LOCALE_NEGOTIATE_SOURCES.contains(&source.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: path.to_path_buf(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "locale_negotiate_source_invalid".to_owned(),
                message: format!(
                    "`locale_negotiate.source` `{}` must be one of: {}.",
                    source,
                    LOCALE_NEGOTIATE_SOURCES.join(", ")
                ),
            });
        }
    }
    if let Some(strategy) = &ln.strategy {
        if !LOCALE_NEGOTIATE_STRATEGIES.contains(&strategy.as_str()) {
            diagnostics.push(DoctorDiagnostic {
                path: path.to_path_buf(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "locale_negotiate_strategy_invalid".to_owned(),
                message: format!(
                    "`locale_negotiate.strategy` `{}` must be one of: {}.",
                    strategy,
                    LOCALE_NEGOTIATE_STRATEGIES.join(", ")
                ),
            });
        }
    }
    if let Some(fallback) = &ln.fallback {
        if !supported.is_empty() && !supported.contains(fallback) {
            diagnostics.push(DoctorDiagnostic {
                path: path.to_path_buf(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "app_locale_fallback_unknown_dest".to_owned(),
                message: format!(
                    "`locale_negotiate.fallback` `{}` is not in `app.locale.supported`.",
                    fallback
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("lazuli-{name}-{unique}"));
        fs::create_dir_all(&root).expect("create temp project root");
        root
    }

    fn write_file(path: &Path, source: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(path, source).expect("write test file");
    }

    #[test]
    fn codegen_wrap_001_fires_on_field_error_literal_in_bucket() {
        let root = temp_project_root("codegen-wrap-fires");
        write_file(
            &root.join("runtime/go/lazuli/auth/password.go"),
            "package auth\n\nfunc f() error { return &lazuli.FieldError{} }\n",
        );

        let diagnostics = check_codegen_wrap_001(&root);
        fs::remove_dir_all(&root).ok();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "CODEGEN-WRAP-001");
        assert_eq!(diagnostics[0].line, 3);
    }

    #[test]
    fn codegen_wrap_001_ignores_top_level_runtime_files() {
        let root = temp_project_root("codegen-wrap-top-level");
        write_file(
            &root.join("runtime/go/lazuli/error_field.go"),
            "package lazuli\n\nvar _ = lazuli.FieldError{}\n",
        );

        let diagnostics = check_codegen_wrap_001(&root);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn codegen_wrap_001_ignores_gen_files() {
        let root = temp_project_root("codegen-wrap-gen");
        write_file(
            &root.join("runtime/go/lazuli/auth/password.gen.go"),
            "package auth\n\nvar _ = lazuli.FieldError{}\n",
        );

        let diagnostics = check_codegen_wrap_001(&root);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn codegen_wrap_001_ignores_test_files() {
        let root = temp_project_root("codegen-wrap-test");
        write_file(
            &root.join("runtime/go/lazuli/auth/password_test.go"),
            "package auth\n\nvar _ = lazuli.FieldError{}\n",
        );

        let diagnostics = check_codegen_wrap_001(&root);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn pattern_draft_stale_001_skips_when_no_drafts() {
        let root = temp_project_root("pattern-draft-no-drafts");
        write_file(
            &root.join("crates/lazuli_codegen_go/src/emitter/patterns.rs"),
            "pub const PATTERN_COMMAND: (&str, &str) = (\"command\", \"v1\");\n",
        );

        let diagnostics = check_pattern_draft_stale_001_at(&root, 1_800_000_000);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    #[test]
    fn pattern_draft_stale_001_skips_when_recent() {
        let root = temp_project_root("pattern-draft-recent");
        let pattern_file = root.join("crates/lazuli_codegen_go/src/emitter/patterns.rs");
        write_file(
            &pattern_file,
            "pub const PATTERN_COMMAND: (&str, &str) = (\"command\", \"draft\");\n",
        );
        let recent = 1_800_000_000_u64;
        if !init_git_repo_with_commit(&root, recent) {
            fs::remove_dir_all(&root).ok();
            return;
        }

        let diagnostics = check_pattern_draft_stale_001_at(&root, recent + 60);
        fs::remove_dir_all(&root).ok();

        assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    }

    fn init_git_repo_with_commit(root: &Path, timestamp: u64) -> bool {
        let init = std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output();
        if !init.map(|output| output.status.success()).unwrap_or(false) {
            return false;
        }

        for args in [
            ["config", "user.email", "test@example.com"],
            ["config", "user.name", "Lazuli Test"],
        ] {
            let _ = std::process::Command::new("git")
                .args(args)
                .current_dir(root)
                .output();
        }

        let add = std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .output();
        if !add.map(|output| output.status.success()).unwrap_or(false) {
            return false;
        }

        std::process::Command::new("git")
            .args(["commit", "-m", "fixture"])
            .env("GIT_AUTHOR_DATE", format!("@{timestamp} +0000"))
            .env("GIT_COMMITTER_DATE", format!("@{timestamp} +0000"))
            .current_dir(root)
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn package_from_sources(sources: Vec<(&str, &str)>) -> DoctorPackage {
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

        for (path, source) in sources {
            let mut file = DoctorFile {
                path: PathBuf::from(path),
                source: source.to_owned(),
                local_diagnostics: Vec::new(),
                lzx: None,
            };

            if path.ends_with(".lzi") {
                contracts.extend(
                    parse_app_contracts(&file.source)
                        .into_iter()
                        .map(|manifest| DoctorAppContract {
                            path: file.path.clone(),
                            manifest,
                        }),
                );
                if let Some(manifest) = parse_app_workspace(&file.source) {
                    workspace = Some(DoctorAppWorkspace {
                        path: file.path.clone(),
                        manifest,
                    });
                }
                if let Some(manifest) = parse_app_manifest(&file.source) {
                    app = Some(DoctorAppManifest {
                        path: file.path.clone(),
                        source: file.source.clone(),
                        manifest,
                    });
                }
                let RegistryParseOutput {
                    registry: parsed_registry,
                    tool_defects,
                } = parse_app_registry_with_defects(&file.source);
                if let Some(manifest) = parsed_registry {
                    registry = Some(DoctorAppRegistry {
                        path: file.path.clone(),
                        manifest,
                    });
                }
                registry_tool_defects.extend(tool_defects.into_iter().map(|defect| {
                    RegistryToolDefect {
                        path: file.path.clone(),
                        line: defect.line,
                        name: defect.name,
                        reason: defect.reason,
                    }
                }));
                profiles.extend(parse_app_profiles(&file.source).into_iter().map(|profile| {
                    DoctorAppProfile {
                        path: file.path.clone(),
                        profile,
                    }
                }));
                collect_canonical_facts(&file, &mut operational);

                // Cut A — typed agent + feature-symbol collection.
                if let Ok(features) = parse_feature_skeletons(&file.source) {
                    for skeleton in &features {
                        if let Ok(feature) = lower_feature_skeleton(skeleton) {
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
                            // Webhooks expanded cycle — populate the
                            // Tier 3 facts so the new doctor rules
                            // (`WEBHOOK-PAYLOAD-001/002`, ...) can run
                            // in unit tests too. The helper mirrors
                            // the production `load` path.
                            let has_text_pattern_api = file
                                .source
                                .lines()
                                .any(|line| line.trim_start().starts_with("api "));
                            if !feature.jobs.is_empty()
                                || !feature.webhooks.is_empty()
                                || !feature.notifications.is_empty()
                                || !feature.event_groups.is_empty()
                                || !feature.commands.is_empty()
                                || !feature.queries.is_empty()
                                || !feature.apis.is_empty()
                                || !feature.records.is_empty()
                                || !feature.enums.is_empty()
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
                                    feature.webhooks.iter().map(|w| w.name.as_str()).collect(),
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
                                    feature.commands.iter().map(|c| c.name.as_str()).collect(),
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
                                    feature.reports.iter().map(|r| r.name.as_str()).collect(),
                                );
                                let cache_lines = collect_construct_lines(
                                    &file.source,
                                    "cache ",
                                    feature.caches.iter().map(|c| c.name.as_str()).collect(),
                                );
                                let translation_line = feature
                                    .translation
                                    .as_ref()
                                    .map(|_| {
                                        find_keyword_line(&file.source, "translation")
                                            .unwrap_or(header_line)
                                    })
                                    .unwrap_or(header_line);
                                tier3_facts.push(Tier3FeatureFacts {
                                    feature: feature.name.clone(),
                                    path: file.path.clone(),
                                    feature_line: header_line,
                                    tenancy_axis: tenancy_axis_for(&feature),
                                    jobs: feature.jobs.clone(),
                                    webhooks: feature.webhooks.clone(),
                                    notifications: feature.notifications.clone(),
                                    event_groups: feature.event_groups.clone(),
                                    tenant_migrations: feature.tenant_migrations.clone(),
                                    resource_previous_names: Vec::new(),
                                    field_previous_names: Vec::new(),
                                    all_resource_names_in_feature: BTreeSet::new(),
                                    all_field_names_in_feature: BTreeMap::new(),
                                    job_lines,
                                    webhook_lines,
                                    notification_lines,
                                    tenant_migration_lines: BTreeMap::new(),
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
                                    reports: feature.reports.clone(),
                                    report_lines,
                                    resources: feature.resources.clone(),
                                    report_decls: skeleton.reports.clone(),
                                    aggregates: feature.aggregates.clone(),
                                    aggregate_lines: BTreeMap::new(),
                                });
                            }
                            // Phase L Tier 4 follow-up — mirror the IR-driven
                            // command map population from the live loader so
                            // the test harness exercises the same code path
                            // as `policy_reachability_diagnostics` /
                            // `command_route_binding_diagnostics`.
                            populate_commands_from_ir(&feature, &mut commands);
                            populate_feature_resources_from_ir(
                                &file.path,
                                &file.source,
                                &feature,
                                &mut feature_resources,
                            );
                            populate_command_external_calls_from_ir(
                                &file,
                                &feature,
                                &mut operational,
                            );
                            populate_job_external_calls_from_ir(&file, &feature, &mut operational);
                            for agent in feature.agents.clone() {
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
                            if let Some(auth) = feature.auth.clone() {
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
                                    password_algorithm_line: anchors.password_algorithm_line,
                                    sessions_line: anchors.sessions_line,
                                    sessions_resource_line: anchors.sessions_resource_line,
                                    mfa_line: anchors.mfa_line,
                                    oauth_lines: anchors.oauth_lines,
                                });
                            }

                            // Migrations bucket cycle Route C — harvest
                            // tenant migrations + resource/field rename
                            // facts so the test helper exercises the
                            // PREVIOUSLY-*/TM-* rules.
                            let mut resource_previous_names: Vec<ResourcePreviousFact> = Vec::new();
                            let mut field_previous_names: Vec<FieldPreviousFact> = Vec::new();
                            let mut all_resource_names_in_feature: BTreeSet<String> =
                                BTreeSet::new();
                            let mut all_field_names_in_feature: BTreeMap<String, BTreeSet<String>> =
                                BTreeMap::new();
                            let resource_header_lines = collect_construct_lines(
                                &file.source,
                                "resource ",
                                feature.resources.iter().map(|r| r.name.as_str()).collect(),
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
                                    resource_previous_names.push(ResourcePreviousFact {
                                        current_name: res.name.clone(),
                                        previous_names: res.previous_names.clone(),
                                        line: res_line,
                                    });
                                }
                                for fld in &res.fields {
                                    if !fld.previous_names.is_empty() {
                                        field_previous_names.push(FieldPreviousFact {
                                            resource_name: res.name.clone(),
                                            current_name: fld.name.clone(),
                                            previous_names: fld.previous_names.clone(),
                                            line: res_line,
                                        });
                                    }
                                }
                            }
                            if !feature.tenant_migrations.is_empty()
                                || !resource_previous_names.is_empty()
                                || !field_previous_names.is_empty()
                            {
                                let tenant_migration_lines = collect_construct_lines(
                                    &file.source,
                                    "tenant_migration ",
                                    feature
                                        .tenant_migrations
                                        .iter()
                                        .map(|t| t.name.as_str())
                                        .collect(),
                                );
                                tier3_facts.push(Tier3FeatureFacts {
                                    feature: feature.name.clone(),
                                    path: file.path.clone(),
                                    feature_line: header_line,
                                    tenancy_axis: tenancy_axis_for(&feature),
                                    jobs: feature.jobs.clone(),
                                    webhooks: feature.webhooks.clone(),
                                    notifications: feature.notifications.clone(),
                                    event_groups: feature.event_groups.clone(),
                                    tenant_migrations: feature.tenant_migrations.clone(),
                                    resource_previous_names,
                                    field_previous_names,
                                    all_resource_names_in_feature,
                                    all_field_names_in_feature,
                                    job_lines: BTreeMap::new(),
                                    webhook_lines: BTreeMap::new(),
                                    notification_lines: BTreeMap::new(),
                                    tenant_migration_lines,
                                    event_group_lines: BTreeMap::new(),
                                    commands: feature.commands.clone(),
                                    command_lines: BTreeMap::new(),
                                    queries: feature.queries.clone(),
                                    query_lines: BTreeMap::new(),
                                    caches: feature.caches.clone(),
                                    cache_lines: BTreeMap::new(),
                                    api_names_text_pattern: Vec::new(),
                                    apis: feature.apis.clone(),
                                    api_lines: BTreeMap::new(),
                                    agents: feature.agents.clone(),
                                    translation: feature.translation.clone(),
                                    translation_line: header_line,
                                    records: feature.records.clone(),
                                    enums: feature.enums.clone(),
                                    events: feature.events.clone(),
                                    policies_declared: feature.policies.span_ref.is_some(),
                                    reports: feature.reports.clone(),
                                    report_lines: BTreeMap::new(),
                                    resources: feature.resources.clone(),
                                    report_decls: skeleton.reports.clone(),
                                    aggregates: feature.aggregates.clone(),
                                    aggregate_lines: BTreeMap::new(),
                                });
                            }
                        }
                    }
                }
                collect_approval_block_presence(&file, &mut approval_presences);
                collect_feature_adapters(&file, &mut feature_adapters);
                collect_feature_uses(&file, &mut feature_uses);
            } else {
                let document = lazuli_syntax::parse_lzx_document(&file.source).unwrap();
                collect_lzx_experience_facts(&document, &mut experiences);
                collect_lzx_operational_facts(&file, &document, &mut operational);
                file.lzx = Some(document);
            }

            files.push(file);
        }

        // Tier 4 follow-up — matches the live `load()` ordering. IR-driven
        // command policy hints fill `feature_symbols.commands` after every
        // file's `tier3_facts` slice has been collected.
        populate_feature_symbols_from_ir(&tier3_facts, &mut feature_symbols);

        DoctorPackage {
            project_root: PathBuf::from("."),
            security_profile: SecurityProfile::Strict,
            single_file_input: false,
            lazurite_manifest: None,
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
            plan_gate_facts: None,
        }
    }

    fn package_from_sources_with_manifest(
        sources: Vec<(&str, &str)>,
        manifest_source: &str,
    ) -> DoctorPackage {
        let mut package = package_from_sources(sources);
        let root = std::env::temp_dir().join(format!(
            "lazuli-doctor-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp manifest project");
        fs::write(root.join("Lazurite.toml"), manifest_source).expect("write Lazurite.toml");
        package.project_root = root;
        package.lazurite_manifest = Some(toml::from_str(manifest_source).unwrap());
        package
    }

    fn minimal_manifest(extra: &str) -> String {
        format!(
            r#"
[project]
name = "demo"
module = "example.com/demo"
schema = 1

[lazuli]
runtime = "v0.1.0"

{extra}
"#
        )
    }

    #[test]
    fn doctor_manifest_required_skipped_when_no_plugin_refs() {
        let package = package_from_sources(vec![("app.lzi", "app NoManifest\n")]);
        let diagnostics = package.diagnostics();

        assert!(
            !codes(&diagnostics).contains("MANIFEST-REQUIRED-001"),
            "MANIFEST-REQUIRED-001 should not fire without @plugin/* refs"
        );
    }

    #[test]
    fn doctor_runs_on_project_without_manifest() {
        let root =
            std::env::temp_dir().join(format!("lazuli-doctor-no-manifest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp doctor project");
        fs::write(root.join("app.lzi"), "app NoManifest\n").expect("write app.lzi");

        let result = doctor_command(&root, SecurityProfile::Strict, false, false);
        let _ = fs::remove_dir_all(&root);

        result.expect("doctor should pass without Lazurite.toml when no @plugin/* refs");
    }

    #[test]
    fn doctor_passes_full_capsule_without_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/full-capsule");
        doctor_command(&root, SecurityProfile::Strict, false, false)
            .expect("full-capsule should pass without Lazurite.toml");
    }

    #[test]
    fn doctor_passes_auth_roundtrip_without_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/auth-roundtrip");
        doctor_command(&root, SecurityProfile::Strict, false, false)
            .expect("auth-roundtrip should pass without Lazurite.toml");
    }

    #[test]
    fn doctor_emits_manifest_required_when_lzi_refs_plugin_without_manifest() {
        let root = std::env::temp_dir().join(format!(
            "lazuli-doctor-manifest-required-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp doctor project");
        fs::write(
            root.join("app.lzi"),
            r#"
feature billing
  command charge
    policy @plugin/payments
"#,
        )
        .expect("write app.lzi");

        let result = doctor_command(&root, SecurityProfile::Strict, false, false);
        let _ = fs::remove_dir_all(&root);

        let error = result.expect_err("doctor should fail when @plugin refs lack Lazurite.toml");
        assert!(
            error.to_string().contains("failed Lazuli doctor checks"),
            "unexpected error: {error:?}"
        );
    }

    /// Regression for the R1.C real-world sweep — `lazuli doctor file.lzi`
    /// (single-file invocation) must not emit `MANIFEST-REQUIRED-001`.
    /// The previous behavior treated the file's parent directory as the
    /// project root, scanned every sibling `.lzi`, and reported a phantom
    /// `Lazurite.toml` even when the target file itself had no plugin refs.
    #[test]
    fn doctor_skips_manifest_required_on_single_file_invocation() {
        let root = std::env::temp_dir().join(format!(
            "lazuli-doctor-single-file-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp doctor project");

        // Sibling file in the parent dir uses @plugin/payments, but we
        // are NOT going to invoke the doctor on it.
        fs::write(
            root.join("sibling.lzi"),
            r#"
feature billing
  command charge
    policy @plugin/payments
"#,
        )
        .expect("write sibling.lzi");

        // The file we DO invoke the doctor on has no plugin refs.
        let target = root.join("clean.lzi");
        fs::write(
            &target,
            r#"
feature greetings
  query.list hello
    title "Hello"
"#,
        )
        .expect("write clean.lzi");

        let package = DoctorPackage::load(&target, SecurityProfile::Strict)
            .expect("load single-file package");
        let diagnostics = package.diagnostics();
        let _ = fs::remove_dir_all(&root);

        assert!(
            !codes(&diagnostics).contains("MANIFEST-REQUIRED-001"),
            "MANIFEST-REQUIRED-001 should not fire on single-file invocations; got: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    /// Regression for the R1.C real-world sweep — the LSP fires both
    /// `app-env-contract` and `env-schema-contract` on the same line of
    /// a `registry.env` block when the env declaration shape is invalid.
    /// The doctor layer dedupes them in favor of `env-schema-contract`.
    #[test]
    fn doctor_dedupes_env_contract_on_same_line() {
        // Invalid env declaration (missing `required|optional`) under
        // `registry.env.group <name>` triggers both `app-env-contract`
        // (via `validate_app_env_line`) and `env-schema-contract` in
        // the LSP. The doctor must collapse them into one diagnostic.
        // Sanity-check the upstream double-emission first via the LSP
        // directly so this test fails loudly if the LSP wiring changes.
        let source = r#"registry
  env
    group storage
      server S3_ENDPOINT: Text
"#;
        let lsp_diagnostics =
            lazuli_lsp::diagnostics_for_source_with_profile(source, SecurityProfile::Strict);
        let lsp_codes: Vec<String> = lsp_diagnostics
            .iter()
            .map(|d| {
                DoctorDiagnostic::from_lsp(PathBuf::from("registry.lzi"), d).code
            })
            .collect();
        // The LSP layer is intentionally left noisy; doctor owns the dedupe.
        assert!(
            lsp_codes.iter().any(|c| c == "app-env-contract")
                && lsp_codes.iter().any(|c| c == "env-schema-contract"),
            "LSP should still emit both codes (dedupe lives in doctor); got: {lsp_codes:?}"
        );

        let root = std::env::temp_dir().join(format!(
            "lazuli-doctor-env-dedupe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp project");
        let target = root.join("registry.lzi");
        fs::write(&target, source).expect("write registry.lzi");

        let package = DoctorPackage::load(&target, SecurityProfile::Strict)
            .expect("load registry.lzi package");
        let diagnostics = package.diagnostics();
        let _ = fs::remove_dir_all(&root);

        let env_line_codes: Vec<&str> = diagnostics
            .iter()
            .filter(|d| {
                d.line == 4
                    && (d.code == "app-env-contract" || d.code == "env-schema-contract")
            })
            .map(|d| d.code.as_str())
            .collect();

        assert_eq!(
            env_line_codes.len(),
            1,
            "exactly one of app-env-contract / env-schema-contract should survive dedupe; got: {env_line_codes:?}"
        );
        assert_eq!(
            env_line_codes[0], "env-schema-contract",
            "env-schema-contract should win the dedupe (registry-scoped owner)"
        );
    }

    #[test]
    fn doctor_emits_plugin_not_declared_when_lzi_refs_undeclared_plugin() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.lzi",
                r#"
feature billing
  command charge
    policy @plugin/payments
"#,
            )],
            &minimal_manifest(""),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("PLUGIN-NOT-DECLARED-001"));
    }

    #[test]
    fn doctor_emits_plugin_unused_when_manifest_declares_unreferenced_plugin() {
        let package = package_from_sources_with_manifest(
            vec![("app.lzi", "app Demo\n")],
            &minimal_manifest(
                r#"
[plugins]
"@plugin/payments" = { module = "example.com/payments", version = "v0.1.0" }
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("PLUGIN-UNUSED-001"));
    }

    #[test]
    fn doctor_emits_plugin_namespace_mismatch_for_known_plugin_adapter_ref() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.lzi",
                r#"
app Demo
  integrations
    payments adapter @adapter.payments
"#,
            )],
            &minimal_manifest(
                r#"
[plugins]
"@plugin/payments" = { module = "example.com/payments", version = "v0.1.0" }
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("PLUGIN-NAMESPACE-MISMATCH-001"));
    }

    #[test]
    fn doctor_emits_submodule_drift_when_generated_go_runtime_differs() {
        let package = package_from_sources_with_manifest(
            vec![("app.lzi", "app Demo\n")],
            &minimal_manifest(
                r#"
[generate.go]
submodule = true
"#,
            ),
        );
        fs::write(
            package.project_root.join("go.mod"),
            "module example.com/demo\n\nrequire lazuli.dev/runtime v0.1.0\n",
        )
        .unwrap();
        fs::create_dir_all(package.project_root.join("dist/go")).unwrap();
        fs::write(
            package.project_root.join("dist/go/go.mod"),
            "module example.com/demo/dist\n\nrequire lazuli.dev/runtime v0.2.0\n",
        )
        .unwrap();

        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("SUBMODULE-DRIFT-001"));
    }

    #[test]
    fn doctor_emits_migration_strategy_conflict_for_manual_before_deploy() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.lzi",
                r#"
app Demo
  deploy
    migrations before_deploy
"#,
            )],
            &minimal_manifest(
                r#"
[migrations]
generated = "migrations/generated"
manual = "migrations/manual"
strategy = "manual"
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("MIGRATION-STRATEGY-CONFLICT-001"));
    }

    #[test]
    fn doctor_emits_frontend_audience_unknown_for_manifest_only_audience() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.web.lzx",
                r#"
surface demo web
  audience admin
    view dashboard Page
"#,
            )],
            &minimal_manifest(
                r#"
[frontends.web]
target = "tanstack-vite"
out = "dist/ts-web"
audiences = ["unknown"]
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("FRONTEND-AUDIENCE-UNKNOWN-001"));
    }

    #[test]
    fn doctor_emits_audience_no_frontend_for_unshipped_lzx_audience() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.web.lzx",
                r#"
surface demo web
  audience admin
    view dashboard Page
"#,
            )],
            &minimal_manifest(
                r#"
[frontends.web]
target = "tanstack-vite"
out = "dist/ts-web"
audiences = []
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("AUDIENCE-NO-FRONTEND-001"));
    }

    #[test]
    fn doctor_emits_frontend_out_collision_defensively() {
        let package = package_from_sources_with_manifest(
            vec![(
                "app.web.lzx",
                r#"
surface demo web
  audience admin
    view dashboard Page
"#,
            )],
            &minimal_manifest(
                r#"
[frontends.admin]
target = "tanstack-vite"
out = "dist/ts"
audiences = ["admin"]

[frontends.web]
target = "tanstack-vite"
out = "dist/ts"
audiences = ["admin"]
"#,
            ),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("FRONTEND-OUT-COLLISION-001"));
    }

    #[test]
    fn doctor_reports_public_surface_reaching_staff_command() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    create: @role.admin, @role.sales

  command create
    policy @policy.create
"#,
            ),
            (
                "customer.public.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience public
    view lead_capture Form
      submit customer.command.create
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "LZX-POL-001"
                && diagnostic.message.contains("audience `public`")
                && diagnostic.message.contains("customer.command.create")
        }));
    }

    #[test]
    fn doctor_allows_public_surface_reaching_public_command() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    capture_lead: @scope.public

  command capture_lead
    policy @policy.capture_lead
"#,
            ),
            (
                "customer.public.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience public
    view lead_capture Form
      submit customer.command.capture_lead
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }

    #[test]
    fn doctor_resolves_platform_action_through_abstract_experience() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    create: @role.admin

  command create
    policy @policy.create
"#,
            ),
            (
                "customer.lzx",
                r#"
experience customer
  imports customer

  view list
    action create -> customer.command.create
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
      actions create
"#,
            ),
        ]);

        // Assert no BLOCKING diagnostics (Error/Warning). Info-level
        // advisories (e.g. `RBAC-CATALOG-MISSING-001` suggesting
        // migration to the top-level RBAC catalog) are non-blocking
        // suggestions and not part of this test's contract — the
        // assertion is "the platform action resolves through the
        // abstract experience without breaking validation".
        let diagnostics = package.diagnostics();
        let blocking: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.severity,
                    DoctorSeverity::Error | DoctorSeverity::Warning
                )
            })
            .collect();
        assert!(
            blocking.is_empty(),
            "expected no blocking diagnostics, got: {:#?}",
            blocking
        );
    }

    #[test]
    fn doctor_reports_command_route_not_bound_by_surface_target() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  policies
    update: @role.admin

  command reassign
    route id: ID
    policy @policy.update
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
    view detail Form
      submit customer.command.reassign
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "LZX-ROUTE-001"
                && diagnostic
                    .message
                    .contains("required command route slot(s) id")
        }));
    }

    #[test]
    fn doctor_allows_command_route_bound_from_context() {
        let package = package_from_sources(vec![
            (
                "customer_auth.lzi",
                r#"
feature customer_auth
  policies
    update: @scope.same_org

  command enable_mfa
    route customer_id: ID from ctx.customer.id
    policy @policy.update
"#,
            ),
            (
                "customer_auth.web.lzx",
                r#"
surface customer_auth web
  uses experience customer_auth

  audience account
    view enable_mfa Form
      submit customer_auth.command.enable_mfa
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }

    #[test]
    fn doctor_reports_app_manifest_operational_gaps() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    customer
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves queries, commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  job import
    trigger schedule "0 2 * * *"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
      header "X-Inbound-Signature"
    tenant_from payload.org_id
    idempotency by payload.id
    handler "./webhooks/inbound.go"
"#,
            ),
            (
                "customer.web.lzx",
                r#"
route customer_list
  path "/customers"
  to customer.view.list
  surface customer web
  audience admin
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-ENV-001"));
        assert!(codes.contains("APP-CAP-001"));
        assert!(codes.contains("APP-RUNTIME-001"));
        assert!(codes.contains("APP-RUNTIME-002"));
        assert!(codes.contains("APP-RUNTIME-003"));
        assert!(codes.contains("APP-TARGET-001"));
        assert!(codes.contains("APP-URL-001"));
        assert!(codes.contains("APP-URL-002"));
    }

    #[test]
    fn doctor_accepts_app_manifest_operational_contract() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    customer
  targets
    backend go
    web react
  environments
    production
  urls
    web production "https://app.acme.example"
    api production "https://api.acme.example"
  env
    group webhooks
      server INBOUND_SECRET: Secret required in production
  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.INBOUND_SECRET
  capabilities
    object_storage files
    integration crm
  architecture
    mode modular_monolith
    service_ready true
    enforce_service_boundaries true
  services
    service crm
      owns customer
      exposes
        query customer.query.list
      publishes customer.*
  communication
    internal sync rpc
    external http
    async event_bus
    propagate actor, tenant, trace_id, request_id
  runtime
    unit api
      serves queries, commands, webhooks, apis
      healthcheck "/healthz"
    unit worker
      runs jobs *
    unit scheduler
      runs schedules *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  api export
    method GET
    path "/api/export"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @scope.public
    handler "./api/export.go"

  job import
    trigger schedule "0 2 * * *"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
      header "X-Inbound-Signature"
    tenant_from payload.org_id
    idempotency by payload.id
    handler "./webhooks/inbound.go"
"#,
            ),
            (
                "customer.web.lzx",
                r#"
route customer_list
  path "/customers"
  to customer.view.list
  surface customer web
  audience admin
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }

    #[test]
    fn doctor_uses_registry_for_env_and_capabilities() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    customer
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit api
      serves webhooks
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  env
    group webhooks
      server INBOUND_SECRET: Secret required in production
  capabilities
    object_storage files
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
      header "X-Inbound-Signature"
    tenant_from payload.org_id
    idempotency by payload.id
    handler "./webhooks/inbound.go"
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(
            diagnostics.is_empty(),
            "expected registry to satisfy app contract, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn doctor_rejects_unknown_auth_failed_redirect_route() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  auth_failed_redirect public_login
  not_found public_not_found
  uses
    customer
  targets
    web react
  environments
    production
  urls
    web production "https://app.acme.example"
  runtime
    unit web
      serves surfaces web
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
"#,
            ),
            (
                "app.lzx",
                r#"
route public_login
  path "/login"
  to customer.view.login
  surface customer web
  audience public
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(
            !codes.contains("APP-ROUTE-001"),
            "did not expect APP-ROUTE-001 for declared route, got: {diagnostics:#?}",
        );
        assert!(
            codes.contains("APP-ROUTE-002"),
            "expected APP-ROUTE-002 for missing not_found route, got: {diagnostics:#?}",
        );
    }

    #[test]
    fn doctor_rejects_error_page_status_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  error_page 418
    template "./views/teapot.tmpl"
"#,
        )]);

        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("error-page-contract"),
            "expected error-page-contract, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn doctor_warns_when_error_page_template_is_missing() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  error_page 404
    template "./views/missing-404.tmpl"
"#,
        )]);

        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("error-page-template-missing"),
            "expected error-page-template-missing, got: {diagnostics:#?}"
        );
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "error-page-template-missing"
                    && diagnostic.severity == DoctorSeverity::Warning
            }),
            "template-missing should be a warning, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn doctor_rejects_duplicate_error_page_status() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  error_page 500
    template "./views/500.tmpl"
  error_page 500
    template "./views/other-500.tmpl"
"#,
        )]);

        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("error-page-duplicate"),
            "expected error-page-duplicate, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn doctor_validates_feature_integration_bindings() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    payments
  bindings
    payments.gateway = integrations.mercadopago
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    mercadopago: PaymentGateway
      adapter @adapter.mercadopago
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(package.diagnostics().is_empty());
    }

    #[test]
    fn doctor_resolves_features_and_requirements_from_enabled_packs() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    payments
  packs
    payments from registry.packs.payments
  bindings
    payments.gateway = registry.integrations.mercadopago
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    mercadopago: PaymentGateway
      adapter @adapter.mercadopago
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            package.diagnostics().is_empty(),
            "expected enabled pack to satisfy uses and binding contracts"
        );
    }

    #[test]
    fn doctor_reports_unknown_enabled_pack() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    payments
  packs
    payments from registry.packs.payments
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
        )]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-PACK-002"));
        assert!(codes.contains("APP-USES-002"));
    }

    #[test]
    fn doctor_reports_unknown_adapter_provenance() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    customer
  targets
    backend go
  environments
    local
  integrations
    crm: CRMProvider
      adapter @unknown.crm
  runtime
    unit api
      serves commands
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    serasa: CreditBureau
      adapter @unknown.serasa
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  integrations
    crm adapter @unknown.fake_crm
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-ADAPTER-001"));
        assert!(codes.contains("REG-ADAPTER-001"));
        assert!(codes.contains("PROFILE-ADAPTER-001"));
    }

    #[test]
    fn doctor_reports_missing_and_mismatched_feature_integration_bindings() {
        let missing = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    payments
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            missing
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "APP-BIND-001")
        );

        let mismatched = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    payments
  bindings
    payments.gateway = integrations.serasa
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    serasa: CreditBureau
      adapter @adapter.serasa
"#,
            ),
            (
                "payments.lzi",
                r#"
feature payments
  requires integration gateway: PaymentGateway
"#,
            ),
        ]);

        assert!(
            mismatched
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == "APP-BIND-004")
        );
    }

    #[test]
    fn doctor_validates_external_calls_against_feature_requirements() {
        let valid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    imports
  bindings
    imports.crm = integrations.crm
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit worker
      runs jobs *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    crm: CRMProvider
      adapter @adapter.crm
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider

  job process_import
    trigger event import_uploaded
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
    timeout "30s"
    handler "./jobs/process_import.go"
"#,
            ),
        ]);

        assert!(
            valid.diagnostics().is_empty(),
            "expected external call contract to pass doctor"
        );

        let invalid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    imports
  targets
    backend go
  environments
    production
  runtime
    unit worker
      runs jobs *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  job process_import
    trigger event import_uploaded
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
    handler "./jobs/process_import.go"
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("INT-CALL-001"));
        assert!(codes.contains("INT-CALL-002"));
        assert!(codes.contains("INT-CALL-003"));
        assert!(codes.contains("INT-CALL-004"));
    }

    #[test]
    fn doctor_validates_profiles_against_app_and_registry_contracts() {
        let valid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    imports
  bindings
    imports.crm = integrations.crm
  targets
    backend go
    web react
  environments
    local
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit worker
      runs jobs *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments sandbox, production
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  bindings
    imports.crm = integrations.crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
"#,
            ),
        ]);

        assert!(
            valid.diagnostics().is_empty(),
            "expected profile contract to pass doctor"
        );

        let invalid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    imports
  targets
    backend go
  environments
    production
  runtime
    unit worker
      runs jobs *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  integrations
    serasa: CreditBureau
      adapter @adapter.serasa
      environments production
"#,
            ),
            (
                "imports.lzi",
                r#"
feature imports
  requires integration crm: CRMProvider
"#,
            ),
            (
                "profiles.lzi",
                r#"
profile local
  urls
    web "http://localhost:3000"
  bindings
    imports.crm = integrations.serasa
  integrations
    crm environment sandbox
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-BIND-001"));
        assert!(codes.contains("PROFILE-001"));
        assert!(codes.contains("PROFILE-INT-001"));
        assert!(codes.contains("PROFILE-BIND-004"));
    }

    #[test]
    fn doctor_reports_app_service_ownership_gaps() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.14"
  uses
    customer
    billing
  targets
    backend go
  services
    service crm
      owns customer
      exposes
        query billing.query.invoice_by_id

    service finance
      owns customer, billing
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
"#,
            ),
            (
                "billing.lzi",
                r#"
feature billing
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "APP-SVC-001"
                && diagnostic
                    .message
                    .contains("feature `customer` is owned by multiple app services")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "APP-SVC-003"
                && diagnostic
                    .message
                    .contains("service `crm` exposes `billing.query.invoice_by_id`")
        }));
    }

    #[test]
    fn doctor_validates_workspace_contract_edges() {
        let valid = package_from_sources(vec![(
            "workspace.lzi",
            r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"
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
"#,
        )]);

        assert!(
            valid.diagnostics().is_empty(),
            "expected valid workspace contract to pass doctor"
        );

        let invalid = package_from_sources(vec![(
            "workspace.lzi",
            r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
  boundaries
    ai consumes ai.*
  communication
    propagate actor
  gateway public_api
    route "/api/ai/*" to app ai
"#,
        )]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("WS-BOUNDARY-001"));
        assert!(codes.contains("WS-EVENT-001"));
        assert!(codes.contains("WS-GW-002"));
        assert!(codes.contains("WS-GW-003"));
        assert!(codes.contains("WS-GW-004"));
        assert!(codes.contains("WS-COMM-001"));
    }

    #[test]
    fn doctor_validates_external_contracts() {
        let valid = package_from_sources(vec![
            (
                "workspace.lzi",
                r#"
workspace AcmeERP
  apps
    ai external contract "acme.ai.v1"
"#,
            ),
            (
                "contracts/ai.lzi",
                r#"
contract acme.ai.v1
  import openapi "./contracts/ai.openapi.json"
  record CustomerSummaryRequest
    customer_id: ID required
  record CustomerSummaryResult
    summary: Text required
  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    timeout "10s"
  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
"#,
            ),
        ]);

        assert!(
            valid.diagnostics().is_empty(),
            "expected external contract to pass doctor"
        );

        let invalid = package_from_sources(vec![
            (
                "workspace.lzi",
                r#"
workspace AcmeERP
  apps
    ai external contract "acme.ai.v2"
"#,
            ),
            (
                "contracts/ai.lzi",
                r#"
contract acme.ai.v1
  operation summarize_customer
    transport http
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("CONTRACT-OP-002"));
        assert!(codes.contains("CONTRACT-OP-003"));
        assert!(codes.contains("CONTRACT-OP-004"));
        assert!(codes.contains("WS-CONTRACT-001"));
    }

    // -------------------------------------------------------------------------
    // Cut A — cross-feature diagnostics (§5.3 snapshot pattern)
    // -------------------------------------------------------------------------

    fn codes(diagnostics: &[DoctorDiagnostic]) -> BTreeSet<&str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    fn temp_project(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "lazuli-doctor-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn doctor_pipeline_invokes_folder_and_design_rules() {
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        write_file(
            &root.join("features/slug/web/views/admin/list.tsx"),
            "export function List() { return null; }\n",
        );
        // Orphan must live in a Lazuli-owned root (app/ | features/ | frontends/)
        // for the feature-orphan-component rule to see it; commit f4185a9
        // narrowed the rule's scope so `src/components/` is no longer walked.
        write_file(
            &root.join("app/components/Foo.tsx"),
            "export function Foo() { return null; }\n",
        );
        write_file(
            &root.join("dist/ts-web/design/allowlist.json"),
            r#"{"bg":["primary"],"text":["foreground"],"font":["sans"]}"#,
        );
        write_file(
            &root.join("features/slug/web/views/admin/styled.tsx"),
            r##"export function Styled() {
  return <div style={{ color: "#7c3aed" }} />;
}
"##,
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);

        assert!(
            surfaced.contains("feature-orphan-component"),
            "expected folder rule to fire; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            surfaced.contains("design-token-hex-leak"),
            "expected design rule to fire; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn lazuli_version_001_warns_when_missing_in_0_x() {
        let package = package_from_sources(vec![("app.lzi", "app Acme\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "0.12.0");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "LAZULI-VERSION-001");
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Warning);
        assert!(
            diagnostics[0]
                .message
                .contains("Expected: lazuli_version \"0.12\""),
            "user-facing prose should advertise the expected pin: {}",
            diagnostics[0].message
        );
    }

    /// Regression for the R1.C real-world sweep — the user-facing message
    /// must not leak the internal debug suffix `expected_value = "..."`.
    #[test]
    fn lazuli_version_001_message_has_no_debug_leakage() {
        let package = package_from_sources(vec![("app.lzi", "app Acme\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "0.14.0");
        assert_eq!(diagnostics.len(), 1);
        assert!(
            !diagnostics[0].message.contains("expected_value ="),
            "LAZULI-VERSION-001 message should not contain debug leakage: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn lazuli_version_001_errors_when_missing_in_1_0() {
        let package = package_from_sources(vec![("app.lzi", "app Acme\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "1.0.0");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn lazuli_version_001_errors_when_mismatched_with_recipe_path() {
        let package =
            package_from_sources(vec![("app.lzi", "app Acme\n  lazuli_version \"0.11\"\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "0.12.0");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Error);
        assert!(
            diagnostics[0]
                .message
                .contains("migrations/recipes/0.11-to-0.12")
        );
        assert_eq!(diagnostics[0].line, 2);
    }

    #[test]
    fn lazuli_version_001_no_diagnostic_when_pin_matches() {
        let package =
            package_from_sources(vec![("app.lzi", "app Acme\n  lazuli_version \"0.12\"\n")]);
        let diagnostics = lazuli_version_001_diagnostics(package.app.as_ref(), "0.12.0");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn lazuli_version_002_errors_when_no_recipe_dir() {
        let mut package =
            package_from_sources(vec![("app.lzi", "app Acme\n  lazuli_version \"0.5\"\n")]);
        package.project_root = temp_project("version-no-recipe");
        let diagnostics =
            lazuli_version_002_diagnostics(package.app.as_ref(), "0.12.0", &package.project_root);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "LAZULI-VERSION-002");
        assert_eq!(diagnostics[0].severity, DoctorSeverity::Error);
    }

    #[test]
    fn lazuli_version_002_silent_when_recipe_exists() {
        let mut package =
            package_from_sources(vec![("app.lzi", "app Acme\n  lazuli_version \"0.11\"\n")]);
        package.project_root = temp_project("version-recipe");
        fs::create_dir_all(
            package
                .project_root
                .join("migrations/recipes/0.11-to-0.12/sample"),
        )
        .unwrap();
        let diagnostics =
            lazuli_version_002_diagnostics(package.app.as_ref(), "0.12.0", &package.project_root);
        assert!(diagnostics.is_empty());
    }

    const APP_URLS_MISSING_FIXTURE: &str = "app MyApp\n";

    const SEMANTIC_UNKNOWN_FIXTURE: &str = include_str!("../tests/fixtures/semantic_unknown.lzi");

    const DOCTOR_HINTS_WRITE_WITHOUT_GUARDS_FIXTURE: &str = r#"
feature customer
  domain
    resource Customer
      id: ID required
      name: Text required

  command create
    input
      name: Text required
    creates Customer from input
"#;

    const DOCTOR_HINTS_GUARDED_WRITE_FIXTURE: &str = r#"
feature customer
  policies
    create: @role.admin

  domain
    resource Customer
      id: ID required
      name: Text required

  command create
    policy @policy.create
    audit default
    input
      name: Text required
    creates Customer from input
"#;

    const DOCTOR_HINTS_UNWRITTEN_RESOURCE_FIXTURE: &str = r#"
feature customer
  domain
    resource Customer
      id: ID required

  command preview
    returns Customer
"#;

    #[test]
    fn doctor_hints_resource_without_policy_for_written_resource() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            DOCTOR_HINTS_WRITE_WITHOUT_GUARDS_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "resource_without_policy_hint")
            .collect();

        assert_eq!(
            hits.len(),
            1,
            "expected one resource hint, got {diagnostics:?}"
        );
        let diagnostic = hits[0];
        assert_eq!(diagnostic.severity, DoctorSeverity::Hint);
        assert_eq!(diagnostic.line, 4);
        assert_eq!(
            diagnostic.message,
            "feature `customer` declares resource `Customer` with no `policies` block ÔÇö every write command implicitly gets the default policy. Add an explicit `policies` block to make access control auditable."
        );
    }

    #[test]
    fn doctor_hints_command_without_audit_for_write_command() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            DOCTOR_HINTS_WRITE_WITHOUT_GUARDS_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "command_without_audit_hint")
            .collect();

        assert_eq!(
            hits.len(),
            1,
            "expected one command hint, got {diagnostics:?}"
        );
        let diagnostic = hits[0];
        assert_eq!(diagnostic.severity, DoctorSeverity::Hint);
        assert_eq!(diagnostic.line, 8);
        assert_eq!(
            diagnostic.message,
            "command `customer.create` is write-effect but has no `audit default` declared ÔÇö write actions without audit are invisible to compliance. Add `audit default` on the command or `audit_default` in feature defaults."
        );
    }

    #[test]
    fn doctor_hints_suppressed_when_policy_block_and_audit_declared() {
        let package =
            package_from_sources(vec![("customer.lzi", DOCTOR_HINTS_GUARDED_WRITE_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);

        assert!(!codes.contains("resource_without_policy_hint"));
        assert!(!codes.contains("command_without_audit_hint"));
    }

    #[test]
    fn doctor_hints_skip_unwritten_resource_and_returns_command() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            DOCTOR_HINTS_UNWRITTEN_RESOURCE_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);

        assert!(!codes.contains("resource_without_policy_hint"));
        assert!(!codes.contains("command_without_audit_hint"));
    }

    #[test]
    fn doctor_emits_semantic_type_unknown_for_unknown_semantic_fields() {
        let package =
            package_from_sources(vec![("semantic_unknown.lzi", SEMANTIC_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == SEMANTIC_TYPE_UNKNOWN_CODE)
            .collect();

        assert!(
            hits.len() >= 2,
            "expected at least two semantic_type_unknown diagnostics, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(hits.iter().any(|diagnostic| {
            diagnostic.line == 8
                && diagnostic.message
                    == "unknown @semantic type \"@semantic.Distance\"; the closed catalog is {EMAIL, PHONE, URL, UUID, DATE, CURRENCY, MONEY, JSON, GEOPOINT}."
        }));
        assert!(hits.iter().any(|diagnostic| {
            diagnostic.line == 15
                && diagnostic.message
                    == "unknown @semantic type \"@semantic.Range\"; the closed catalog is {EMAIL, PHONE, URL, UUID, DATE, CURRENCY, MONEY, JSON, GEOPOINT}."
        }));
    }

    const CROSS_FEATURE_TYPE_UNRESOLVED_FIXTURE: &str = r#"
feature customer
  domain
    resource Customer
      id: ID required
      owner: User required

    record InviteDraft
      reviewer: Reviewer required
"#;

    #[test]
    fn doctor_reports_unresolved_bare_type_refs_on_resource_and_record_fields() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            CROSS_FEATURE_TYPE_UNRESOLVED_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        let messages: BTreeSet<&str> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "cross_feature_type_unresolved")
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.contains(
            "type `User` referenced by `customer.Customer.owner` is not declared in any feature. Add a `resource`/`record`/`enum User` block, or check for a typo."
        ));
        assert!(messages.contains(
            "type `Reviewer` referenced by `customer.InviteDraft.reviewer` is not declared in any feature. Add a `resource`/`record`/`enum Reviewer` block, or check for a typo."
        ));
    }

    #[test]
    fn doctor_reports_unresolved_bare_type_refs_on_command_input_slots() {
        let mut package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command create
    input
      email: Text required
    returns Text
"#,
        )]);
        let command = package
            .tier3_facts
            .iter_mut()
            .flat_map(|fact| fact.commands.iter_mut())
            .find(|command| command.name == "create")
            .expect("expected create command fact");
        let lazuli_ir::CommandInput::Typed(slots) = &mut command.input else {
            panic!("expected typed command input");
        };
        slots[0].type_ref = lazuli_ir::TypeRef::UserDefined(lazuli_ir::QualifiedName {
            feature: None,
            name: "EmailAddress".to_owned(),
        });

        let diagnostics = package.diagnostics();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "cross_feature_type_unresolved"
                && diagnostic.message
                    == "type `EmailAddress` referenced by `customer.create.input.email` is not declared in any feature. Add a `resource`/`record`/`enum EmailAddress` block, or check for a typo."
        }));
    }

    #[test]
    fn doctor_allows_bare_type_refs_declared_in_any_feature() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    resource Customer
      id: ID required
      owner: User required

feature identity
  domain
    resource User
      id: ID required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("cross_feature_type_unresolved"),
            "declared cross-feature type should resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    const FEATURE_USES_MISSING_FIXTURE: &str = r#"
feature customer
  domain
    resource Customer
      id: ID required

    record CustomerView
      id: ID required

feature identity
  domain
    record UserProfile
      id: ID required

feature catalog
  domain
    record ProductFilter
      sku: Text required

feature orders
  domain
    resource Order
      customer: Customer required

    query.list by_product
      params
        filter: ProductFilter required

  command assign_user
    input
      assignee: UserProfile required
    returns CustomerView
"#;

    #[test]
    fn doctor_warns_when_cross_feature_type_refs_omit_uses() {
        let package = package_from_sources(vec![("orders.lzi", FEATURE_USES_MISSING_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "feature_uses_missing")
            .collect();
        let messages: BTreeSet<&str> = hits
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert_eq!(
            hits.len(),
            3,
            "expected one missing-uses warning per referenced feature, got {hits:#?}; all diagnostics: {diagnostics:#?}"
        );
        assert!(
            hits.iter()
                .all(|diagnostic| diagnostic.severity == DoctorSeverity::Warning)
        );
        assert!(messages.contains(
            "feature `orders` references types declared in feature `customer` but does not declare `uses customer` in its header. Add `uses customer` to make the dependency explicit."
        ));
        assert!(messages.contains(
            "feature `orders` references types declared in feature `identity` but does not declare `uses identity` in its header. Add `uses identity` to make the dependency explicit."
        ));
        assert!(messages.contains(
            "feature `orders` references types declared in feature `catalog` but does not declare `uses catalog` in its header. Add `uses catalog` to make the dependency explicit."
        ));
    }

    #[test]
    fn doctor_allows_cross_feature_type_refs_with_declared_uses() {
        let fixture = FEATURE_USES_MISSING_FIXTURE.replace(
            "feature orders\n",
            "feature orders\n  uses customer, identity, catalog\n",
        );
        let package = package_from_sources(vec![("orders.lzi", fixture.as_str())]);
        let diagnostics = package.diagnostics();

        assert!(
            !codes(&diagnostics).contains("feature_uses_missing"),
            "declared uses should satisfy cross-feature refs; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_tool_with_stricter_policy_than_agent() {
        // Agent declares `policy @policy.read` but invokes a `command`
        // whose policy is `@policy.delete` — the conservative lattice
        // ordering flags this as `agent_tool_policy_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    policy @policy.delete
    deletes Customer

  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    safety @validator.pii_scrub
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_tool_policy_diagnostics"),
            "expected agent_tool_policy_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_write_tool_without_safety() {
        // Same write-tool fan-in but with no `safety` declared — Cut A
        // requires safety as the write-tool guard (Q-impl-4 deferred
        // `idempotency by` to Cut B).
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    policy @policy.delete
    deletes Customer

  agent triage
    policy @policy.delete
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_tool_write_unguarded_diagnostics"),
            "expected agent_tool_write_unguarded_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_pii_tool_without_safety() {
        // Registry declares `@tool.web_search` with `pii_classes contact`
        // and the agent invokes it with no safety — emit
        // `agent_pii_unsafetied_warning`.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
registry
  tools
    tool web_search
      effect read
      pii_classes contact
      adapter @adapter.serp

feature customer
  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      @tool.web_search
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_pii_unsafetied_warning"),
            "expected agent_pii_unsafetied_warning; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_unknown_discriminator_target() {
        // No `enum Intent` is declared anywhere — emit
        // `agent_discriminator_target_invalid_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer_support
  agent classify_intent
    policy @policy.read
    output discriminator Intent
    model @llm.classifier
    temperature 0
    seed 42
    prompt "./p.md"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_discriminator_target_invalid_diagnostics"),
            "expected agent_discriminator_target_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_evals_without_determinism_pin() {
        // Agent has evals but no `temperature 0` and no `seed` — emit
        // `eval_nondeterministic_warning`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent flaky
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case smoke
        requires output contains "ok"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("eval_nondeterministic_warning"),
            "expected eval_nondeterministic_warning; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_registry_tool_missing_effect() {
        // `tool_registry_effect_required_diagnostics` is the only id
        // that fires off the registry-side IR. The parser collects a
        // defect for every `tool <name>` whose block omits `effect`.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
registry
  tools
    tool calendar_create_event
      adapter @adapter.google_calendar
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tool_registry_effect_required_diagnostics"),
            "expected tool_registry_effect_required_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_eval_ordered_op_on_non_numeric_operands() {
        // `requires customer.email < "x"` is an ordered op on text —
        // emit `eval_ordered_op_invalid_diagnostics`.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent bounded
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case bad
        requires customer.email < "z@example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("eval_ordered_op_invalid_diagnostics"),
            "expected eval_ordered_op_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_when_app_urls_missing_or_empty() {
        for source in [APP_URLS_MISSING_FIXTURE, "app MyApp\n  urls\n"] {
            let package = package_from_sources(vec![("app.lzi", source)]);
            let diagnostics = package.diagnostics();
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code == "app_urls_missing")
                .unwrap_or_else(|| {
                    panic!(
                        "expected app_urls_missing; got {:?}",
                        diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
                    )
                });

            assert_eq!(diagnostic.severity, DoctorSeverity::Warning);
            assert_eq!(diagnostic.message, APP_URLS_MISSING_MESSAGE);
        }
    }

    #[test]
    fn doctor_rejects_cors_origin_in_unknown_environment() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  cors
    allow_origins staging "https://staging.example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_unknown_environment_diagnostics"),
            "expected cors_unknown_environment_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 36 — `app.logging` / `app.tracing`
    // closed catalogs + sample-rate range + exporter binding.

    #[test]
    fn doctor_rejects_app_logging_level_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    level verbose
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_level_invalid_diagnostics"),
            "expected app_logging_level_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_format_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    format yaml
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_format_invalid_diagnostics"),
            "expected app_logging_format_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_redact_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    redact secrets
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_redact_unknown_diagnostics"),
            "expected app_logging_redact_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_sample_rate_above_one() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    sample_rate 2.5
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_sample_rate_range_diagnostics"),
            "expected app_logging_sample_rate_range_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_tracing_sample_rate_below_zero() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  tracing
    sample_rate -0.1
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_tracing_sample_rate_range_diagnostics"),
            "expected app_tracing_sample_rate_range_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_tracing_exporter_unbound() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  tracing
    exporter mystery
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_tracing_exporter_unbound_diagnostics"),
            "expected app_tracing_exporter_unbound_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 37 — audit emit_to, event.trace
    // level, and health probe path shape.

    #[test]
    fn doctor_rejects_audit_emit_to_unknown_stream() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    audit actor, target.id
      emit_to nonexistent_stream
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("audit_emit_to_unknown_diagnostics"),
            "expected audit_emit_to_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_audit_emit_to_reserved_audit_log() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    audit actor, target.id
      emit_to audit_log
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("audit_emit_to_unknown_diagnostics"),
            "reserved stream `audit_log` must resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_audit_emit_to_authored_event_group() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  event_group customer_audit *
  command archive
    audit actor, target.id
      emit_to customer_audit
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("audit_emit_to_unknown_diagnostics"),
            "authored event_group must resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_event_trace_level_outside_catalog() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace welcome_email_sent
      level critical
      payload
        email: Text
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_level_invalid_diagnostics"),
            "expected event_trace_level_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_level_on_domain_event() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event customer_created
      level warn
      payload
        id: ID
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_level_on_domain_event_diagnostics"),
            "expected event_trace_level_on_domain_event_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_health_probe_path_without_leading_slash() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  runtime
    unit api
      healthcheck "healthz"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("health_probe_path_invalid_diagnostics"),
            "expected health_probe_path_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_canonical_health_probes() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  runtime
    unit api
      healthcheck "/healthz"
      readiness "/readyz"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("health_probe_path_invalid_diagnostics"),
            "canonical paths must not fire health probe diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_app_logging_with_canonical_values() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    level info
    format json
    redact pii
    sample_rate 1.0
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes_set = codes(&diagnostics);
        assert!(
            !codes_set.contains("app_logging_level_invalid_diagnostics"),
            "canonical logging must not fire level diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_format_invalid_diagnostics"),
            "canonical logging must not fire format diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_redact_unknown_diagnostics"),
            "canonical logging must not fire redact diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_sample_rate_range_diagnostics"),
            "canonical logging must not fire sample_rate diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_cors_wildcard_with_credentials() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  cors
    allow_origins production "https://app.example.com"
    allow_origins local "*"
    allow_credentials true
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_credentials_wildcard_conflict_diagnostics"),
            "expected cors_credentials_wildcard_conflict_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_cors_origin_not_in_urls() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"

  cors
    allow_origins production "https://stranger.example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_origin_undocumented_diagnostics"),
            "expected cors_origin_undocumented_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_cors_origin_matching_declared_url() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  urls
    web production "https://app.example.com"

  cors
    allow_origins production "https://app.example.com"
    allow_credentials true
    max_age "1h"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes_set = codes(&diagnostics);
        for code in [
            "cors_unknown_environment_diagnostics",
            "cors_credentials_wildcard_conflict_diagnostics",
            "cors_origin_undocumented_diagnostics",
        ] {
            assert!(
                !codes_set.contains(code),
                "well-formed CORS must not produce {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn doctor_rejects_approval_with_unknown_role() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    policy @policy.delete
    approval
      required_when target.tier = enterprise
      by @role.nonexistent
      timeout "24h"
      then deny
    deletes Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("approval_role_unresolved_diagnostics"),
            "expected approval_role_unresolved_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_approval_with_malformed_timeout() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    approval
      by @role.admin
      timeout "soon"
      then deny
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("approval_timeout_invalid_diagnostics"),
            "expected approval_timeout_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_approval_satisfies_write_tool_guard_without_agent_safety() {
        // Agent dispatches a write tool whose target command carries
        // `approval` — the guard is satisfied even though the agent
        // has no `safety` declaration.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin
    read: @scope.same_org

  command archive
    policy @policy.delete
    approval
      by @role.admin
      timeout "24h"
      then deny
    deletes Customer

  agent triage
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      command.archive
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_tool_write_unguarded_diagnostics"),
            "approval on target command must satisfy the write-tool guard; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_approval_missing_required_children() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  policies
    delete: @role.admin

  command archive
    approval
      by @role.admin
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("approval_contract_diagnostics"),
            "expected approval_contract_diagnostics for missing children; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_authored_event_trace_agent_run() {
        // `event.trace agent_run` is reserved by the IR — authoring
        // it as a domain event must fail with the reserved-name
        // diagnostic.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace agent_run
      payload
        agent_id: ID
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_subscriber_referencing_unknown_payload_field() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job aggregate_costs
    trigger event.trace agent_run
      fictional_field = payload.fictional_field
      cost_usd = payload.cost_usd
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_run_subscriber_payload_drift_diagnostics"),
            "expected agent_run_subscriber_payload_drift_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_subscriber_with_canonical_fields_only() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job aggregate_costs
    trigger event.trace agent_run
      cost_usd = payload.cost_usd
      tokens_total = payload.tokens_total
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_run_subscriber_payload_drift_diagnostics"),
            "canonical fields must not drift; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 35 — the 3 new reserved trace
    // event names (`command_run`, `job_run`, `webhook_run`) must
    // reuse the same `event_trace_reserved_name_diagnostics` path as
    // the Cut A.8 `agent_run` case. Authoring any of them is rejected.

    #[test]
    fn doctor_rejects_authored_event_trace_command_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace command_run
      payload
        cmd: Text
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for command_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_authored_event_trace_job_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace job_run
      payload
        job_id: ID
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for job_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_authored_event_trace_webhook_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace webhook_run
      payload
        url: Text
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for webhook_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 35 — `trigger_trace_unknown`. A
    // subscriber referencing `@trace.<name>` or
    // `trigger event.trace <name>` must resolve to a built-in trace
    // event or an authored `event.trace <name>` in the same file.

    #[test]
    fn doctor_rejects_trigger_trace_unknown_namespace_form() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job dangling
    trigger @trace.fictional_event
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "expected trigger_trace_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_trigger_trace_namespace_for_built_in() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job collect
    trigger @trace.agent_run
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "built-in @trace.agent_run must resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_trigger_trace_namespace_for_authored_event() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace customer_authored
      payload
        id: ID
  job collect
    trigger @trace.customer_authored
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "authored event.trace in same file must satisfy @trace.<name>; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_expose_http_path_colliding_cross_feature_with_api() {
        // Agent in `customer` exposes the same (method, path) as an
        // `api` block in `customer_outreach`. Cross-feature collision
        // fires `agent_expose_path_conflict_cross_feature_diagnostics`.
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
"#,
            ),
            (
                "customer_outreach.lzi",
                r#"
feature customer_outreach
  api customer_summary_stream
    method POST
    path "/api/customers/:id/summary"
    output Text
    policy @scope.public
    handler "./x.go"
"#,
            ),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_expose_path_conflict_cross_feature_diagnostics"),
            "expected agent_expose_path_conflict_cross_feature_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_unknown_audience_on_expose_http() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  agent restricted
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x"
      audience nonexistent_audience
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_expose_audience_unknown_diagnostics"),
            "expected agent_expose_audience_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_audience_declared_in_surface() {
        let package = package_from_sources(vec![
            (
                "customer.lzi",
                r#"
feature customer
  agent admin_only
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/admin/x"
      audience admin
"#,
            ),
            (
                "customer.web.lzx",
                r#"
surface customer web
  uses experience customer

  audience admin
"#,
            ),
        ]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_expose_audience_unknown_diagnostics"),
            "audience declared in .lzx must be honored; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // ---------------------------------------------------------------------
    // Row 30 — Storage bucket cycle: 5 typed `@cap.File` diagnostics.
    // ---------------------------------------------------------------------

    #[test]
    fn doctor_emits_cap_file_visibility_undeclared() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_export
  domain
    resource Export
      id: ID required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_undeclared"),
            "expected cap_file_visibility_undeclared on api output; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_skips_visibility_undeclared_on_resource_field() {
        // Resource fields default `visibility` to private; the
        // diagnostic only fires on api outputs.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_field
  domain
    resource Export
      file: @cap.File(max_size:10mb,accept:text/csv) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("cap_file_visibility_undeclared"),
            "resource fields default to private; should not emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_accept_input_output_mismatch() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_pipeline
  domain
    resource ImportBatch
      file: @cap.File(max_size:25mb,accept:application/json,visibility:private) required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_accept_input_output_mismatch"),
            "expected cap_file_accept_input_output_mismatch; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_overlapping_accept_lists() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_pipeline_ok
  domain
    resource ImportBatch
      file: @cap.File(max_size:25mb,accept:text/csv,visibility:private) required

  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("cap_file_accept_input_output_mismatch"),
            "overlapping accept lists should not emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_visibility_signed_ttl_mismatch_when_ttl_missing() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_signed
  api download
    method GET
    path "/api/x/download"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed)
    policy @policy.global_read
    handler "./api/x/download.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_signed_ttl_mismatch"),
            "signed visibility without signed_ttl must emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_visibility_signed_ttl_mismatch_when_ttl_with_private() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_private_ttl
  domain
    resource Export
      file: @cap.File(max_size:10mb,accept:text/csv,visibility:private,signed_ttl:1h) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_visibility_signed_ttl_mismatch"),
            "private visibility with signed_ttl must emit; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_size_unit_invalid() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_size
  domain
    resource Export
      blob: @cap.File(max_size:large,accept:text/csv) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_size_unit_invalid"),
            "expected cap_file_size_unit_invalid; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_cap_file_mime_family_unknown() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x_mime
  domain
    resource Export
      blob: @cap.File(max_size:10mb,accept:gibberish/csv,visibility:private) required
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cap_file_mime_family_unknown"),
            "expected cap_file_mime_family_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_well_formed_agent() {
        // Sanity gate: an agent that pins determinism, supplies safety,
        // and uses local read tools whose targets exist emits none of
        // the Cut A error codes.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    query.lookup by_id by id: ID
      policy @policy.read

  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    safety @validator.pii_email_scrub
    tools
      query.lookup.by_id
    evals
      case mentions_status
        requires output contains "active"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let cut_a_errors = [
            "agent_tool_policy_diagnostics",
            "agent_tool_write_unguarded_diagnostics",
            "agent_discriminator_target_invalid_diagnostics",
            "agent_discriminator_field_invalid_diagnostics",
            "eval_ordered_op_invalid_diagnostics",
            "tool_registry_effect_required_diagnostics",
        ];
        let surfaced = codes(&diagnostics);
        for code in cut_a_errors {
            assert!(
                !surfaced.contains(code),
                "well-formed agent should not emit {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    // -------------------------------------------------------------------------
    // Phase L — `auth` block cross-feature diagnostics.
    //
    // Auth ids per docs/proposals/bucket-auth-cycle.md §Doctor/LSP:
    //   - auth_password_algorithm_hash_mismatch
    //   - auth_password_no_session
    //   - auth_sessions_resource_unknown
    //   - auth_identity_field_unknown
    //   - auth_oauth_adapter_unbound
    //   - auth_oauth_no_password_alt
    //   - auth_session_ttl_too_short
    // -------------------------------------------------------------------------

    #[test]
    fn doctor_emits_auth_password_algorithm_hash_mismatch() {
        // `auth.password.algorithm bcrypt` diverges from
        // `@cap.Hashed(algorithm:argon2id)` on the session resource's
        // hash field.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    password
      algorithm bcrypt
      hash @fn.h
      verify @fn.v
      rate_limit "5 per 10 minutes"

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let mismatch: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_password_algorithm_hash_mismatch")
            .collect();
        assert_eq!(
            mismatch.len(),
            1,
            "expected exactly one auth_password_algorithm_hash_mismatch; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            mismatch[0].message.contains("bcrypt"),
            "diagnostic should cite authored algorithm: {}",
            mismatch[0].message
        );
        assert!(
            mismatch[0].message.contains("argon2id"),
            "diagnostic should cite resource axis: {}",
            mismatch[0].message
        );
    }

    #[test]
    fn doctor_emits_auth_sessions_resource_unknown() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    sessions
      resource BogusSession
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("auth_sessions_resource_unknown"),
            "expected auth_sessions_resource_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_auth_password_no_session() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Account
      email: @semantic.Email required

  auth
    identity Account.email
    password
      algorithm argon2id
      hash @fn.h
      verify @fn.v
      rate_limit "5 per 10 minutes"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_password_no_session")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one auth_password_no_session; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Warning);
        assert!(hits[0].message.contains("login will not issue sessions"));
    }

    #[test]
    fn doctor_infos_auth_oauth_no_password_alt() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    oauth google
      adapter @adapter.google_oauth

    sessions
      resource Session
      ttl "1 day"
      refresh false

  extensions
    adapter google_oauth: IntegrationAdapter[GoogleOAuth] at "./oauth.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_oauth_no_password_alt")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one auth_oauth_no_password_alt; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Info);
        assert!(hits[0].message.contains("OAuth-only"));
    }

    #[test]
    fn doctor_warns_auth_session_ttl_too_short() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    sessions
      resource Session
      ttl "30 minutes"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_session_ttl_too_short")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one auth_session_ttl_too_short; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Warning);
        assert!(
            hits[0]
                .message
                .contains("session TTL <1h forces frequent re-login")
        );
    }

    #[test]
    fn doctor_emits_auth_identity_field_unknown_for_missing_field() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

  auth
    identity Session.email
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_identity_field_unknown")
            .collect();
        assert!(
            !hits.is_empty(),
            "expected auth_identity_field_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(hits[0].message.contains("field not found"));
    }

    #[test]
    fn doctor_emits_auth_identity_field_unknown_for_non_identity_shape() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      note: Text required
      expires_at: DateTime required

  auth
    identity Session.note
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_identity_field_unknown")
            .collect();
        assert!(
            !hits.is_empty(),
            "expected auth_identity_field_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(hits[0].message.contains("identity-shaped"));
    }

    #[test]
    fn doctor_emits_auth_oauth_adapter_unbound() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    oauth google
      adapter @adapter.bogus_google_oauth

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("auth_oauth_adapter_unbound"),
            "expected auth_oauth_adapter_unbound; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_resolves_oauth_adapter_via_feature_extensions() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    oauth google
      adapter @adapter.google_oauth

    sessions
      resource Session
      ttl "1 day"
      refresh false

  extensions
    adapter google_oauth: IntegrationAdapter[GoogleOAuth] at "./oauth.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("auth_oauth_adapter_unbound"),
            "extension adapter must satisfy oauth adapter lookup; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_well_formed_auth_emits_no_auth_diagnostics() {
        // The canonical-shape positive case. None of the four auth_*
        // diagnostics should fire on a coherent block.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    password
      algorithm argon2id
      hash @fn.h
      verify @fn.v
      rate_limit "5 per 10 minutes"

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);
        for code in [
            "auth_password_algorithm_hash_mismatch",
            "auth_password_no_session",
            "auth_sessions_resource_unknown",
            "auth_identity_field_unknown",
            "auth_oauth_adapter_unbound",
            "auth_oauth_no_password_alt",
            "auth_session_ttl_too_short",
        ] {
            assert!(
                !surfaced.contains(code),
                "well-formed auth should not emit {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn doctor_resolves_identity_resource_via_feature_uses() {
        // `customer_auth uses customer` — Customer.email is declared
        // in the `customer` feature; auth identity in customer_auth
        // must resolve via the `uses` graph.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  domain
    resource Customer
      email: @semantic.Email required

feature customer_auth
  uses customer

  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

  auth
    identity Customer.email
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("auth_identity_field_unknown"),
            "uses-relative identity resolution failed: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // -------------------------------------------------------------------------
    // Migrations bucket cycle Route C — eight diagnostics on tenant_migration,
    // `previously migrated`, and `deploy` checkpoint/strategy fields.
    // -------------------------------------------------------------------------

    const MIGRATIONS_PREVIOUSLY_FWD_FIXTURE: &str =
        include_str!("../tests/fixtures/migrations/previously_forward_unresolved.lzi");
    const MIGRATIONS_PREVIOUSLY_CYCLE_FIXTURE: &str =
        include_str!("../tests/fixtures/migrations/previously_cycle.lzi");
    const MIGRATIONS_PREVIOUSLY_DUP_FIXTURE: &str =
        include_str!("../tests/fixtures/migrations/previously_duplicate_claim.lzi");
    const MIGRATIONS_TM_AXIS_FIXTURE: &str =
        include_str!("../tests/fixtures/migrations/tenant_migration_axis_unknown.lzi");
    const MIGRATIONS_TM_IDEMP_FIXTURE: &str =
        include_str!("../tests/fixtures/migrations/tenant_migration_no_idempotency.lzi");
    const MIGRATIONS_CHECKPOINT_INVALID_FIXTURE: &str =
        include_str!("../tests/fixtures/migrations/deploy_checkpoint_path_invalid.lzi");
    const MIGRATIONS_STRATEGY_INVALID_FIXTURE: &str =
        include_str!("../tests/fixtures/migrations/deploy_strategy_invalid.lzi");
    const MIGRATIONS_TM_TARGET_UNKNOWN_FIXTURE: &str = r#"
feature x
  defaults
    tenancy org

  tenant_migration backfill_x
    target query.missing
    axis org
    idempotency envelope.tenant_id
    handler "./migrations/backfill_x.go"
"#;
    const MIGRATIONS_TM_HANDLER_MISSING_FIXTURE: &str = r#"
feature x
  defaults
    tenancy org

  domain
    query.lookup by_id by id: ID

  tenant_migration backfill_x
    target query.by_id
    axis org
    idempotency envelope.tenant_id
    handler "./migrations/backfill_x.go"
"#;

    #[test]
    fn previously_forward_unresolved_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_PREVIOUSLY_FWD_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("PREVIOUSLY-FWD-001"),
            "expected PREVIOUSLY-FWD-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn previously_cycle_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_PREVIOUSLY_CYCLE_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("PREVIOUSLY-CYCLE-001"),
            "expected PREVIOUSLY-CYCLE-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn previously_duplicate_claim_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_PREVIOUSLY_DUP_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("PREVIOUSLY-DUP-001"),
            "expected PREVIOUSLY-DUP-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_axis_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_AXIS_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tenant-migration-axis-mismatch"),
            "expected tenant-migration-axis-mismatch in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_no_idempotency_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_IDEMP_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tenant-migration-idempotency-required"),
            "expected tenant-migration-idempotency-required in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_target_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_TARGET_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tenant-migration-target-unknown"),
            "expected tenant-migration-target-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_handler_missing_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_HANDLER_MISSING_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("tenant-migration-handler-missing"),
            "expected tenant-migration-handler-missing in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deploy_checkpoint_path_invalid_fires() {
        let package =
            package_from_sources(vec![("app.lzi", MIGRATIONS_CHECKPOINT_INVALID_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("DEPLOY-CHECKPOINT-001"),
            "expected DEPLOY-CHECKPOINT-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deploy_strategy_invalid_fires() {
        let package = package_from_sources(vec![("app.lzi", MIGRATIONS_STRATEGY_INVALID_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("DEPLOY-STRATEGY-001"),
            "expected DEPLOY-STRATEGY-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // DEPLOY-CHECKPOINT-002 (stale snapshot) requires an on-disk
    // snapshot file. The fixture lives in
    // `tests/fixtures/migrations/snapshot_stale/` so the doctor rule
    // can resolve the path relative to the manifest's location.
    #[test]
    fn deploy_checkpoint_stale_fires() {
        let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/migrations/snapshot_stale/app.lzi");
        let source = std::fs::read_to_string(&manifest_path).expect("read app");
        let mut package = package_from_sources(vec![]);
        if let Some(manifest) = parse_app_manifest(&source) {
            package.app = Some(DoctorAppManifest {
                path: manifest_path,
                source,
                manifest,
            });
        }
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("DEPLOY-CHECKPOINT-002"),
            "expected DEPLOY-CHECKPOINT-002 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_observability_source_001_fires_on_unknown_token() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app crm
  observability
    error_source dev,qa
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("OBSERVABILITY-SOURCE-001"),
            "expected OBSERVABILITY-SOURCE-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_observability_panic_001_warns_when_recover_disabled_outside_dev() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app crm
  environments
    prod
  observability
    panic_recover false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("OBSERVABILITY-PANIC-001"),
            "expected OBSERVABILITY-PANIC-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // OpenAPI bucket cycle (row 48) — deprecation diagnostics on
    // `Command.deprecated` / `Api.deprecated` typed lifts.
    // =========================================================================

    const OPENAPI_REPLACEMENT_UNKNOWN_FIXTURE: &str =
        include_str!("../tests/fixtures/openapi/deprecated_replacement_unknown.lzi");
    const OPENAPI_SUNSET_DATE_INVALID_FIXTURE: &str =
        include_str!("../tests/fixtures/openapi/deprecated_sunset_date_invalid.lzi");
    const OPENAPI_SUNSET_IN_PAST_FIXTURE: &str =
        include_str!("../tests/fixtures/openapi/deprecated_sunset_in_past.lzi");
    const OPENAPI_TEXT_PATTERN_API_FIXTURE: &str =
        include_str!("../tests/fixtures/openapi/text_pattern_api_block.lzi");

    #[test]
    fn deprecated_replacement_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", OPENAPI_REPLACEMENT_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-replacement-unknown"),
            "expected deprecated-replacement-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_sunset_date_invalid_fires() {
        let package = package_from_sources(vec![("x.lzi", OPENAPI_SUNSET_DATE_INVALID_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated_sunset_date_invalid"),
            "expected deprecated_sunset_date_invalid in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_sunset_in_past_fires() {
        let package = package_from_sources(vec![("x.lzi", OPENAPI_SUNSET_IN_PAST_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-sunset-past"),
            "expected deprecated-sunset-past in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_no_replacement_fires_for_command() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  command legacy_update
    policy @policy.update
    deprecated
    creates Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-no-replacement"),
            "expected deprecated-no-replacement in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_no_replacement_fires_for_api() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-no-replacement"),
            "expected deprecated-no-replacement in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_no_replacement_skips_when_replacement_resolves() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  command legacy_update
    policy @policy.update
    deprecated replacement command.update_v2
    creates Customer

  command update_v2
    policy @policy.update
    creates Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("deprecated-no-replacement"),
            "did not expect deprecated-no-replacement in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn api_deprecated_replacement_unknown_fires() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated
      replacement api.export_v2
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-replacement-unknown"),
            "expected deprecated-replacement-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_replacement_unknown_fires_for_cross_feature_api() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated replacement billing.api.export_v2
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-replacement-unknown"),
            "expected deprecated-replacement-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn api_deprecated_sunset_past_fires_info() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated
      replacement api.export_v2
      sunset "2024-01-01"

  api export_v2
    method GET
    path "/api/customers/export-v2"
    output [Customer]
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "deprecated-sunset-past")
            .collect();
        assert_eq!(hits.len(), 1, "diagnostics: {diagnostics:?}");
        assert_eq!(hits[0].severity, DoctorSeverity::Info);
    }

    #[test]
    fn deprecated_sunset_future_does_not_fire() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  command legacy_update
    policy @policy.update
    deprecated
      replacement command.update_v2
      sunset "2027-01-01"
    creates Customer

  command update_v2
    policy @policy.update
    creates Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("deprecated-sunset-past"),
            "did not expect deprecated-sunset-past in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // i18n bucket cycle (row 54) — 5 critical doctor diagnostics anchored
    // on `app.locale` / `Translation` / `LocaleNegotiate` IR. The full
    // 15-diagnostic catalog (`translation_locale_*`, `rule_message_ref_*`,
    // `locale_negotiate_*`, `app_locale_*`, `cldr_plural_arm_invalid`)
    // is covered by the `i18n_diagnostics` walk; this set exercises the
    // top-5 most-likely authoring mistakes from the proposal.
    // =========================================================================

    const I18N_DEFAULT_NOT_SUPPORTED_FIXTURE: &str =
        include_str!("../tests/fixtures/i18n/default_not_supported.lzi");
    const I18N_TRANSLATION_LOCALE_UNSUPPORTED_FIXTURE: &str =
        include_str!("../tests/fixtures/i18n/translation_locale_unsupported.lzi");
    const I18N_TRANSLATION_KEY_UNRESOLVED_FIXTURE: &str =
        include_str!("../tests/fixtures/i18n/translation_key_unresolved.lzi");
    const I18N_CLDR_PLURAL_ARM_INVALID_FIXTURE: &str =
        include_str!("../tests/fixtures/i18n/cldr_plural_arm_invalid.lzi");
    const I18N_LOCALE_NEGOTIATE_SOURCE_INVALID_FIXTURE: &str =
        include_str!("../tests/fixtures/i18n/locale_negotiate_source_invalid.lzi");

    #[test]
    fn app_locale_default_unsupported_fires() {
        let package = package_from_sources(vec![("app.lzi", I18N_DEFAULT_NOT_SUPPORTED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_locale_default_unsupported"),
            "expected app_locale_default_unsupported in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn translation_locale_unsupported_fires() {
        let package = package_from_sources(vec![(
            "app.lzi",
            I18N_TRANSLATION_LOCALE_UNSUPPORTED_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("translation_locale_unsupported"),
            "expected translation_locale_unsupported in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn rule_message_ref_unresolved_fires() {
        let package =
            package_from_sources(vec![("app.lzi", I18N_TRANSLATION_KEY_UNRESOLVED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("rule_message_ref_unresolved"),
            "expected rule_message_ref_unresolved in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cldr_plural_arm_invalid_fires() {
        let package = package_from_sources(vec![("app.lzi", I18N_CLDR_PLURAL_ARM_INVALID_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cldr_plural_arm_invalid"),
            "expected cldr_plural_arm_invalid in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn locale_negotiate_source_invalid_fires() {
        let package = package_from_sources(vec![(
            "app.lzi",
            I18N_LOCALE_NEGOTIATE_SOURCE_INVALID_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("locale_negotiate_source_invalid"),
            "expected locale_negotiate_source_invalid in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // Cache bucket cycle (row 51) — 5 doctor diagnostics on QueryCache /
    // Command.invalidates / registry capabilities.
    // =========================================================================

    const CACHE_INVALIDATES_UNRESOLVED_FIXTURE: &str =
        include_str!("../tests/fixtures/cache/invalidates_target_unresolved.lzi");
    const CACHE_NAMESPACE_COLLISION_FIXTURE: &str =
        include_str!("../tests/fixtures/cache/namespace_collision.lzi");
    const CACHE_CAPABILITY_UNDECLARED_FIXTURE: &str =
        include_str!("../tests/fixtures/cache/capability_undeclared.lzi");
    // CL.C.3 — feature-level `cache <name>` profile diagnostics.
    const CACHE_PROFILE_UNKNOWN_FIXTURE: &str =
        include_str!("../tests/fixtures/cache/profile_unknown.lzi");
    const CACHE_TAG_UNKNOWN_FIXTURE: &str = include_str!("../tests/fixtures/cache/tag_unknown.lzi");
    const CACHE_TTL_CONTRACT_SWR_FIXTURE: &str =
        include_str!("../tests/fixtures/cache/ttl_contract_swr_exceeds.lzi");

    #[test]
    fn cache_invalidates_target_unresolved_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_INVALIDATES_UNRESOLVED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_invalidates_target_unresolved"),
            "expected cache_invalidates_target_unresolved in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_namespace_collision_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_NAMESPACE_COLLISION_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_namespace_collision"),
            "expected cache_namespace_collision in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_capability_undeclared_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_CAPABILITY_UNDECLARED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_capability_undeclared"),
            "expected cache_capability_undeclared in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_ttl_unit_invalid_fires_on_empty_quoted_prose() {
        // Direct fact injection — the parser does not let an empty
        // quoted ttl through (`parse_cache_ttl` short-circuits on the
        // empty payload), but the doctor rule still guards the
        // typed-promotion path so it stays defensive against future
        // parser changes.
        let mut package = package_from_sources(vec![]);
        let cache = lazuli_ir::QueryCache {
            key: "k".into(),
            ttl: lazuli_ir::CacheTtl::Quoted("".into()),
            tags: Vec::new(),
            namespace: None,
            profile_ref: None,
        };
        let query = lazuli_ir::Query::List(lazuli_ir::ListQuery {
            name: "list".into(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: None,
            modifier: None,
            cache: Some(cache),
            previous_names: Vec::new(),
            span_ref: None,
        });
        package.tier3_facts.push(Tier3FeatureFacts {
            feature: "customer".into(),
            path: PathBuf::from("x.lzi"),
            feature_line: 1,
            tenancy_axis: None,
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            resource_previous_names: Vec::new(),
            field_previous_names: Vec::new(),
            all_resource_names_in_feature: BTreeSet::new(),
            all_field_names_in_feature: BTreeMap::new(),
            job_lines: BTreeMap::new(),
            webhook_lines: BTreeMap::new(),
            notification_lines: BTreeMap::new(),
            tenant_migration_lines: BTreeMap::new(),
            event_group_lines: BTreeMap::new(),
            commands: Vec::new(),
            command_lines: BTreeMap::new(),
            queries: vec![query],
            query_lines: BTreeMap::new(),
            caches: Vec::new(),
            cache_lines: BTreeMap::new(),
            api_names_text_pattern: Vec::new(),
            apis: Vec::new(),
            api_lines: BTreeMap::new(),
            agents: Vec::new(),
            translation: None,
            translation_line: 1,
            records: Vec::new(),
            enums: Vec::new(),
            events: Vec::new(),
            policies_declared: false,
            reports: Vec::new(),
            report_lines: BTreeMap::new(),
            resources: Vec::new(),
            report_decls: Vec::new(),
            aggregates: Vec::new(),
            aggregate_lines: BTreeMap::new(),
        });
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_ttl_unit_invalid"),
            "expected cache_ttl_unit_invalid in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // -------------------------------------------------------------------------
    // CL.C.3 — feature-level `cache <name>` profile diagnostics:
    // `cache-profile-unknown`, `cache-tag-unknown`, `cache-ttl-contract`.
    // -------------------------------------------------------------------------

    #[test]
    fn cache_profile_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_PROFILE_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache-profile-unknown"),
            "expected cache-profile-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_tag_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_TAG_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache-tag-unknown"),
            "expected cache-tag-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_ttl_contract_swr_exceeds_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_TTL_CONTRACT_SWR_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache-ttl-contract"),
            "expected cache-ttl-contract in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn openapi_text_pattern_api_block_fires() {
        // The diagnostic fires when the source contains an `api` token
        // that the typed lifter did not promote into `feature.apis`.
        // Authoring an `api` block without the required `method`/`path`/
        // `output` fails the feature skeleton parse, so the fixture
        // routes through a hand-built `tier3_facts` entry that mirrors
        // a real-world mixed package (some features typed, one feature
        // legacy text-pattern). The shape is regression-style: when the
        // fixture changes the diagnostic shape, this test catches it.
        let mut package = package_from_sources(vec![]);
        package.tier3_facts.push(Tier3FeatureFacts {
            feature: "legacy".to_owned(),
            path: PathBuf::from("legacy.lzi"),
            feature_line: 1,
            tenancy_axis: None,
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            resource_previous_names: Vec::new(),
            field_previous_names: Vec::new(),
            all_resource_names_in_feature: BTreeSet::new(),
            all_field_names_in_feature: BTreeMap::new(),
            job_lines: BTreeMap::new(),
            webhook_lines: BTreeMap::new(),
            notification_lines: BTreeMap::new(),
            tenant_migration_lines: BTreeMap::new(),
            event_group_lines: BTreeMap::new(),
            commands: Vec::new(),
            command_lines: BTreeMap::new(),
            queries: Vec::new(),
            query_lines: BTreeMap::new(),
            caches: Vec::new(),
            cache_lines: BTreeMap::new(),
            api_names_text_pattern: vec!["customer_legacy".to_owned()],
            apis: Vec::new(),
            api_lines: BTreeMap::new(),
            agents: Vec::new(),
            translation: None,
            translation_line: 1,
            records: Vec::new(),
            enums: Vec::new(),
            events: Vec::new(),
            policies_declared: false,
            reports: Vec::new(),
            report_lines: BTreeMap::new(),
            resources: Vec::new(),
            report_decls: Vec::new(),
            aggregates: Vec::new(),
            aggregate_lines: BTreeMap::new(),
        });
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("openapi_text_pattern_api_block"),
            "expected openapi_text_pattern_api_block in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // Webhooks expanded cycle — eight new doctor diagnostics.
    // =========================================================================

    /// `WEBHOOK-PAYLOAD-001` fires when `payload from
    /// webhook_events.<X>` cannot be resolved against the registry
    /// catalog.
    #[test]
    fn webhook_payload_001_unresolved_envelope() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
registry
  webhook_events
    crm_customer_upsert
      external_id: Text required

feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    payload from webhook_events.unknown_envelope
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.org_id
    handler "./integrations/upsert_customer_from_crm.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-PAYLOAD-001"),
            "expected WEBHOOK-PAYLOAD-001, got {codes:?}"
        );
    }

    /// `WEBHOOK-PAYLOAD-002` fires when `tenant_from payload.<axis>`
    /// references a field the envelope does not declare.
    #[test]
    fn webhook_payload_002_tenant_field_missing_in_envelope() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
registry
  webhook_events
    crm_customer_upsert
      external_id: Text required

feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    payload from webhook_events.crm_customer_upsert
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    handler "./integrations/upsert.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-PAYLOAD-002"),
            "expected WEBHOOK-PAYLOAD-002, got {codes:?}"
        );
    }

    /// `WEBHOOK-REPLAY-001` fires when `replay allow` is declared
    /// without `within "<duration>"`.
    #[test]
    fn webhook_replay_001_allow_without_window() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    replay
      allow
    handler "./integrations/upsert.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-REPLAY-001"),
            "expected WEBHOOK-REPLAY-001, got {codes:?}"
        );
    }

    /// `WEBHOOK-DLQ-001` fires when `dlq emit <event>` references an
    /// event the feature does not declare anywhere.
    #[test]
    fn webhook_dlq_001_unresolved_emit_event() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    retry 3 backoff exponential
    dlq emit not_declared_anywhere
    handler "./integrations/upsert.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-DLQ-001"),
            "expected WEBHOOK-DLQ-001, got {codes:?}"
        );
    }

    /// `WEBHOOK-DLQ-003` fires when `retry` is declared without `dlq`.
    #[test]
    fn webhook_dlq_003_retry_without_dlq() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer_import
  webhook crm_customer_upsert
    path "/webhooks/crm/customer-upsert"
    verify hmac sha256
      secret env.CRM_WEBHOOK_SECRET
      header "X-CRM-Signature"
    tenant_from payload.org_id
    idempotency by payload.external_id
    retry 3 backoff exponential
    handler "./integrations/upsert.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-DLQ-003"),
            "expected WEBHOOK-DLQ-003, got {codes:?}"
        );
    }

    /// `WEBHOOK-EVENT-001` fires when a `webhook_events.<X>` envelope
    /// is declared in registry but no webhook references it.
    #[test]
    fn webhook_event_001_dead_envelope_in_registry() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
registry
  webhook_events
    orphan_envelope
      external_id: Text required
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"WEBHOOK-EVENT-001"),
            "expected WEBHOOK-EVENT-001, got {codes:?}"
        );
    }

    #[test]
    fn webhook_event_version_decreasing_previous_exceeds_current() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  webhook_event customer.archived
    payload
      customer_id: ID
    version 1
    previous_version 2
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"webhook-event-version-decreasing"),
            "expected webhook-event-version-decreasing, got {codes:?}"
        );
    }

    #[test]
    fn webhook_event_payload_empty_rejects_empty_schema() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  webhook_event customer.created
    payload
    version 1
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"webhook-event-payload-empty"),
            "expected webhook-event-payload-empty, got {codes:?}"
        );
    }

    #[test]
    fn webhook_event_deprecated_no_replacement_requires_trail() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  webhook_event customer.deleted
    payload
      customer_id: ID
    version 3
    deprecated true
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"webhook-event-deprecated-no-replacement"),
            "expected webhook-event-deprecated-no-replacement, got {codes:?}"
        );
    }

    // =========================================================================
    // Notifications expanded bucket cycle — six new doctor diagnostics on
    // `notification.digest` and `notification.throttle`.
    // =========================================================================

    fn notification_package(extra_children: &str) -> DoctorPackage {
        let source = format!(
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
{extra_children}
"#
        );
        package_from_sources(vec![("package.lzi", source.as_str())])
    }

    fn assert_notification_diag(code: &str, extra_children: &str) {
        let package = notification_package(extra_children);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&code), "expected {code}, got {codes:?}");
    }

    /// `NOTIF-DIGEST-001` fires when `digest every "<duration>"` does
    /// not match the closed shape `<N> (seconds|minutes|hours|days)`.
    #[test]
    fn notif_digest_001_every_invalid_shape() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 month"
      group_by customer_id
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-001"),
            "expected NOTIF-DIGEST-001, got {codes:?}"
        );
    }

    /// `NOTIF-DIGEST-002` fires when `digest max_size` is 0 or above
    /// the 10_000 ceiling. Both extremes are authoring smells: 0 is
    /// dead; > 10k blows up the in-window buffer.
    #[test]
    fn notif_digest_002_max_size_out_of_range() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 hour"
      group_by customer_id
      max_size 99999
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-002"),
            "expected NOTIF-DIGEST-002, got {codes:?}"
        );
    }

    /// `NOTIF-DIGEST-003` fires when `digest template_strategy` is not
    /// in the closed catalog.
    #[test]
    fn notif_digest_003_template_strategy_unknown() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 hour"
      group_by customer_id
      template_strategy squash
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-003"),
            "expected NOTIF-DIGEST-003, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-001` fires when neither `per_recipient` nor
    /// `per_channel` is present.
    #[test]
    fn notif_throttle_001_axis_missing() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "1 hour"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-001"),
            "expected NOTIF-THROTTLE-001, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-002` fires when `burst` is larger than the
    /// parsed `max_per` window.
    #[test]
    fn notif_throttle_002_burst_exceeds_max_per() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "1 second"
      per_recipient
      burst 2
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-002"),
            "expected NOTIF-THROTTLE-002, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-003` fires when `throttle max_per` does not
    /// match `<N> (seconds|minutes|hours|days)`.
    #[test]
    fn notif_throttle_003_max_per_invalid_shape() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "forever"
      per_recipient
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-003"),
            "expected NOTIF-THROTTLE-003, got {codes:?}"
        );
    }

    /// Two extra cases per new diagnostic, paired with the focused
    /// tests above, give each code three covered variants without
    /// repeating a full package fixture 18 times.
    #[test]
    fn notif_digest_throttle_diagnostics_cover_three_cases_each() {
        for extra in [
            "    digest\n      every forever\n",
            "    digest\n      every \"\"\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-001", extra);
        }
        for extra in [
            "    digest\n      every 1h\n      max_size 0\n",
            "    digest\n      every 1h\n      max_size 10001\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-002", extra);
        }
        for extra in [
            "    digest\n      every 1h\n      template_strategy replace\n",
            "    digest\n      every 1h\n      template_strategy \"merge\"\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-003", extra);
        }
        for extra in [
            "    throttle\n      max_per 1h\n",
            "    throttle\n      max_per 1h\n      burst 1\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-001", extra);
        }
        for extra in [
            "    throttle\n      max_per 1s\n      per_channel\n      burst 2\n",
            "    throttle\n      max_per 0s\n      per_recipient\n      burst 1\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-002", extra);
        }
        for extra in [
            "    throttle\n      max_per later\n      per_channel\n",
            "    throttle\n      max_per \"1 month\"\n      per_recipient\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-003", extra);
        }
    }

    // -------------------------------------------------------------------------
    // AUTH-SESSION-* doctor codes — tenant-pin shim validation
    // -------------------------------------------------------------------------

    fn auth_fact_with_extra_columns(
        feature: &str,
        sessions_resource: &str,
        extra_columns: Vec<ir::SessionExtraColumn>,
    ) -> AuthFacts {
        AuthFacts {
            feature: feature.to_owned(),
            auth: ir::Auth {
                identity: ir::AuthIdentity {
                    field: ir::FieldRef {
                        resource: ir::QualifiedName {
                            feature: None,
                            name: "User".to_owned(),
                        },
                        field: "email".to_owned(),
                    },
                    public_contract: None,
                },
                password: None,
                sessions: Some(ir::AuthSessions {
                    resource: ir::QualifiedName {
                        feature: None,
                        name: sessions_resource.to_owned(),
                    },
                    ttl: "7 days".to_owned(),
                    refresh: false,
                    extra_columns,
                }),
                mfa: None,
                oauth: vec![],
                span_ref: None,
            },
            path: PathBuf::from(format!("features/{feature}/{feature}.lzi")),
            line: 1,
            identity_line: 1,
            password_line: None,
            password_algorithm_line: None,
            sessions_line: Some(5),
            sessions_resource_line: Some(6),
            mfa_line: None,
            oauth_lines: BTreeMap::new(),
        }
    }

    fn extra_id_column(field_name: &str) -> ir::SessionExtraColumn {
        ir::SessionExtraColumn {
            field_name: field_name.to_owned(),
            column_name: format!("{field_name}_id"),
            go_type: "lazuli.ID".to_owned(),
            references: Some("Org".to_owned()),
            required: true,
        }
    }

    fn extra_non_id_column(field_name: &str) -> ir::SessionExtraColumn {
        ir::SessionExtraColumn {
            field_name: field_name.to_owned(),
            column_name: field_name.to_owned(),
            go_type: "string".to_owned(),
            references: None,
            required: true,
        }
    }

    fn call_auth_diagnostics(facts: &[AuthFacts]) -> Vec<DoctorDiagnostic> {
        let mut feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>> =
            BTreeMap::new();
        for fact in facts {
            if let Some(sessions) = fact.auth.sessions.as_ref() {
                let mut resources: BTreeMap<String, ResourceFact> = BTreeMap::new();
                resources.insert(
                    sessions.resource.name.clone(),
                    ResourceFact {
                        path: fact.path.clone(),
                        line: 1,
                        fields: BTreeMap::new(),
                    },
                );
                feature_resources.insert(fact.feature.clone(), resources);
            }
        }
        auth_diagnostics(
            facts,
            &feature_resources,
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
        )
    }

    #[test]
    fn auth_session_tenant_001_fires_on_non_id_go_type() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_non_id_column("region")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-TENANT-001"),
            "expected AUTH-SESSION-TENANT-001, got {codes:?}"
        );
    }

    #[test]
    fn auth_session_tenant_001_does_not_fire_on_id_type() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-TENANT-001"),
            "AUTH-SESSION-TENANT-001 must not fire for lazuli.ID columns; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_extra_001_fires_on_two_extra_columns() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org"), extra_id_column("workspace")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-EXTRA-001"),
            "expected AUTH-SESSION-EXTRA-001 for 2 extra columns; got {codes:?}"
        );
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "AUTH-SESSION-EXTRA-001")
            .collect();
        assert_eq!(
            errors[0].severity,
            DoctorSeverity::Error,
            "AUTH-SESSION-EXTRA-001 must be error severity"
        );
    }

    #[test]
    fn auth_session_extra_001_does_not_fire_on_one_extra_column() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-EXTRA-001"),
            "AUTH-SESSION-EXTRA-001 must not fire for a single extra column; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_extra_001_does_not_fire_when_no_extra_columns() {
        let fact = auth_fact_with_extra_columns("auth_feature", "TenantSession", vec![]);
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-EXTRA-001"),
            "AUTH-SESSION-EXTRA-001 must not fire when extra_columns is empty; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_fires_on_issue_session_call_in_handler() {
        let root = temp_project_root("callsite-001-fires");
        let handler_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.go");
        write_file(
            &handler_path,
            r#"package handlers

import "github.com/lazuli-lang/lazuli/runtime/go/lazuli/auth"

func Login(ctx *lazuli.Ctx, input LoginInput) (string, error) {
    token, _, err := auth.IssueSession(ctx, db, userID, auth.SessionAttrs{})
    return token, err
}
"#,
        );

        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-CALLSITE-001"),
            "expected AUTH-SESSION-CALLSITE-001 for auth.IssueSession in user handler; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_does_not_fire_when_no_extra_columns() {
        let root = temp_project_root("callsite-001-no-extra");
        let handler_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.go");
        write_file(
            &handler_path,
            r#"package handlers

func Login(ctx *lazuli.Ctx, input LoginInput) (string, error) {
    token, _, err := auth.IssueSession(ctx, db, userID, auth.SessionAttrs{})
    return token, err
}
"#,
        );

        let fact = auth_fact_with_extra_columns("auth_feature", "TenantSession", vec![]);
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-CALLSITE-001"),
            "AUTH-SESSION-CALLSITE-001 must not fire when session has no extra columns; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_skips_gen_go_files() {
        let root = temp_project_root("callsite-001-skip-gen");
        let gen_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.gen.go");
        write_file(
            &gen_path,
            "func Login() { auth.IssueSession(ctx, db, id, auth.SessionAttrs{}) }\n",
        );

        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-CALLSITE-001"),
            "AUTH-SESSION-CALLSITE-001 must not fire for .gen.go files; got {codes:?}"
        );
    }

    // ---------------------------------------------------------------
    // Roadmap §1.2 — HTTP hygiene contracts: cookie / proxy / limits.
    // Each block ships one diagnostic code that fires on any of its
    // closed-catalog violations.
    // ---------------------------------------------------------------

    #[test]
    fn doctor_rejects_cookie_same_site_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  cookie
    default
      same_site loose
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_cookie_contract_diagnostics"),
            "expected app_cookie_contract_diagnostics for unknown same_site; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_cookie_max_age_unparseable() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  cookie
    default
      max_age "forever"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_cookie_contract_diagnostics"),
            "expected app_cookie_contract_diagnostics for unparseable max_age; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_cookie_block_in_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  cookie
    default
      signed true
      secure true
      http_only true
      same_site strict
      max_age "7d"
    session
      same_site lax
      max_age "12h"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("app_cookie_contract_diagnostics"),
            "cookie block in closed catalog must not raise app_cookie_contract_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_proxy_trusted_unparseable_cidr() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  proxy
    trusted not_a_cidr
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_proxy_contract_diagnostics"),
            "expected app_proxy_contract_diagnostics for unparseable CIDR; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_proxy_real_ip_header_empty() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  proxy
    trusted 10.0.0.0/8
    real_ip_header ""
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_proxy_contract_diagnostics"),
            "expected app_proxy_contract_diagnostics for empty header name; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_proxy_block_with_well_formed_cidrs_and_headers() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  proxy
    trusted 10.0.0.0/8, 172.16.0.0/12, 2001:db8::/32
    real_ip_header X-Forwarded-For
    forwarded_proto_header X-Forwarded-Proto
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("app_proxy_contract_diagnostics"),
            "well-formed proxy block must not raise app_proxy_contract_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_limits_body_size_unparseable() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  limits
    body_size "huge"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_limits_contract_diagnostics"),
            "expected app_limits_contract_diagnostics for unparseable size; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_limits_timeout_unparseable() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  limits
    timeout "soon"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_limits_contract_diagnostics"),
            "expected app_limits_contract_diagnostics for unparseable duration; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_limits_block_with_well_formed_literals() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  limits
    body_size "10mb"
    header_size "16kb"
    upload_size "100mb"
    timeout "30s"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("app_limits_contract_diagnostics"),
            "well-formed limits block must not raise app_limits_contract_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =============================================================
    // Roadmap §1.10 — `headers-contract` /
    // `secret-rotation-overlap-contract` /
    // `secret-rotation-binding-unknown` tests.
    // =============================================================

    #[test]
    fn doctor_errors_under_production_when_headers_block_absent() {
        // Production profile errors when the app has no `headers`
        // block at all. Strict + Prototype defer until the author
        // opts in by declaring even a partial block.
        let mut package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  title "Acme CRM"
  environments
    production
"#,
        )]);
        package.security_profile = SecurityProfile::Production;
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("headers-contract"),
            "expected headers-contract under Production profile; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_warns_when_partial_headers_block_misses_required_slots() {
        // Author opted in by declaring a `headers` block but only
        // populated one slot. Strict profile (default) emits a
        // warning naming the missing slots.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  headers
    csp "default-src 'self'"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("headers-contract"),
            "expected headers-contract when partial headers block omits required slots; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_accepts_full_app_headers_block() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  headers
    csp "default-src 'self'"
    hsts max_age 31536000 include_subdomains preload
    x_frame_options DENY
    x_content_type_options nosniff
    referrer_policy strict-origin-when-cross-origin
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            !codes.contains("headers-contract"),
            "well-formed headers block must not produce headers-contract; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_rejects_unknown_referrer_policy_token() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  headers
    csp "default-src 'self'"
    hsts max_age 31536000
    x_frame_options DENY
    x_content_type_options nosniff
    referrer_policy bogus-policy
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("headers-contract"),
            "expected headers-contract for unknown referrer_policy; got {:?}",
            codes
        );
        let message = diagnostics
            .iter()
            .find(|d| d.code == "headers-contract")
            .map(|d| d.message.as_str())
            .unwrap_or_default();
        assert!(
            message.contains("referrer_policy") || message.contains("bogus-policy"),
            "diagnostic should name referrer_policy or the bad value; got {message}"
        );
    }

    #[test]
    fn doctor_rejects_secret_rotation_overlap_longer_than_cadence() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  secret_rotation default
    cadence 24h
    overlap 48h
    auto_rollback true
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("secret-rotation-overlap-contract"),
            "expected secret-rotation-overlap-contract; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_accepts_secret_rotation_overlap_shorter_than_cadence() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  secret_rotation default
    cadence 90d
    overlap 24h
    auto_rollback true
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            !codes.contains("secret-rotation-overlap-contract"),
            "well-formed overlap must not fire; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_rejects_encryption_key_pointing_at_unknown_rotation_profile() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
      rotation manual
      rotation_profile not_declared
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("secret-rotation-binding-unknown"),
            "expected secret-rotation-binding-unknown for missing profile; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_accepts_encryption_key_binding_to_declared_rotation_profile() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
      rotation manual
      rotation_profile default
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  secret_rotation default
    cadence 90d
    overlap 24h
    auto_rollback true
"#,
            ),
        ]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            !codes.contains("secret-rotation-binding-unknown"),
            "declared profile must satisfy the binding; got {:?}",
            codes
        );
    }
}
