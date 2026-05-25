//! `DoctorPackage` — the package-wide doctor fact bundle.
//!
//! Carries every IR slice + per-file source needed by the
//! `diagnostics()` dispatcher (which lives in `dispatch.rs`).
//! `load` runs once per `lazuli doctor` invocation and walks all
//! `.lzi` / `.lzx` sources in the package, lifting every Tier-3
//! fact family + auth / plan-gate facts into a single struct.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 2.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_analyzer::lower_feature_skeleton;
use lazuli_lsp::SecurityProfile;
use lazuli_syntax::parse_feature_skeletons;

use crate::app_manifest::{
    RegistryParseOutput, parse_app_contracts, parse_app_manifest, parse_app_profiles,
    parse_app_registry_with_defects, parse_app_workspace,
};
use crate::lazurite_manifest::{self, Manifest};

use super::aggregators;
use super::helpers::{doctor_project_root, resolve_test_discipline_severity};
use super::parsers::{is_lzi_path, is_lzx_path};
use super::scanners::derive_feature_name;
use super::{
    AgentFacts, ApprovalBlockPresence, AuthFacts, CommandKey, CommandPolicy, DoctorAppContract,
    DoctorAppManifest, DoctorAppProfile, DoctorAppRegistry, DoctorAppWorkspace, DoctorDiagnostic,
    DoctorFile, DoctorSeverity, ExperienceFacts, FeatureSymbols, FieldPreviousFact,
    OperationalFacts, RegistryToolDefect, ResourceFact, ResourcePreviousFact, Tier3FeatureFacts,
    collect_approval_block_presence, collect_auth_anchors, collect_canonical_facts,
    collect_construct_lines, collect_event_group_lines, collect_feature_adapters,
    collect_feature_uses, collect_lzx_experience_facts, collect_lzx_operational_facts,
    collect_package_paths, collect_query_lines, collect_text_pattern_api_names,
    find_keyword_line, line_col_for_offset, money_arithmetic_001_diagnostics,
    money_compare_001_diagnostics, populate_command_external_calls_from_ir,
    populate_commands_from_ir, populate_feature_resources_from_ir,
    populate_feature_symbols_from_ir, populate_job_external_calls_from_ir,
    semantic_type_unknown_diagnostics_for_feature,
    semantic_type_unknown_diagnostics_for_syntax_feature, tenancy_axis_for,
    vocab_tests_missing_001_diagnostics,
};

#[derive(Debug)]
pub(crate) struct DoctorPackage {
    pub(super) project_root: PathBuf,
    pub(super) security_profile: SecurityProfile,
    /// `true` when `lazuli doctor` was invoked on a single `.lzi`/`.lzx`
    /// file rather than a project directory. Single-file mode skips
    /// project-level checks (e.g. `MANIFEST-REQUIRED-001`) that depend
    /// on having a real project root with `app.lzi` + `Lazurite.toml`.
    pub(super) single_file_input: bool,
    pub(super) lazurite_manifest: Option<Manifest>,
    pub(super) files: Vec<DoctorFile>,
    pub(super) workspace: Option<DoctorAppWorkspace>,
    pub(super) contracts: Vec<DoctorAppContract>,
    pub(super) app: Option<DoctorAppManifest>,
    pub(super) registry: Option<DoctorAppRegistry>,
    pub(super) profiles: Vec<DoctorAppProfile>,
    pub(super) commands: BTreeMap<CommandKey, CommandPolicy>,
    pub(super) experiences: BTreeMap<String, ExperienceFacts>,
    pub(super) operational: OperationalFacts,
    /// Cut A: agent IR per feature, loaded through
    /// `lazuli_syntax::parse_feature_skeletons` +
    /// `lazuli_analyzer::lower_feature_skeleton`.
    pub(super) agents: Vec<AgentFacts>,
    /// Cut A: per-feature enum/record/query/command symbol tables used
    /// for discriminator + tool-policy cross-resolution.
    pub(super) feature_symbols: BTreeMap<String, FeatureSymbols>,
    /// Cut A: registry `tool <name>` headers that lacked `effect`.
    pub(super) registry_tool_defects: Vec<RegistryToolDefect>,
    /// Phase L Tier 4b — minimal text-pattern walk of `approval` blocks
    /// inside command bodies. Only used for the `missing children`
    /// variant of `approval_contract_diagnostics`; every other approval
    /// check reads `Command.approval` from `Tier3FeatureFacts` (IR).
    /// The walker exists because parse-error approval blocks never
    /// reach the IR — they short-circuit the feature lift.
    pub(super) approval_presences: Vec<ApprovalBlockPresence>,
    /// Phase L: lowered `auth` block per feature, paired with source
    /// line anchors for subblock-precise diagnostics.
    pub(super) auth_facts: Vec<AuthFacts>,
    /// Phase L: per-feature resource declarations + field type text.
    /// Used to resolve `auth identity Customer.email` and
    /// `auth sessions resource CustomerSession` and to read
    /// `@cap.Hashed(algorithm:…)` axes off session resource fields.
    pub(super) feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>>,
    /// Phase L: per-feature `extensions adapter <local>` declarations
    /// for the `auth_oauth_adapter_unbound` adapter resolution scope.
    pub(super) feature_adapters: BTreeMap<String, BTreeSet<String>>,
    /// Phase L: per-feature `uses <other_feature>, ...` references so
    /// `auth identity Customer.email` in `feature customer_auth` can
    /// resolve `Customer` in `feature customer` when `uses customer` is
    /// declared.
    pub(super) feature_uses: BTreeMap<String, BTreeSet<String>>,
    /// Phase L Tier 3: lifted `Job` / `Webhook` / `Notification` /
    /// `EventGroup` per feature, paired with source line anchors so the
    /// six new diagnostics (`JOB-*`, `WEBHOOK-SCOPE-*`,
    /// `NOTIF-CHANNEL-*`, `EVENTGROUP-NESTING-*`) attach to the right
    /// authoring site.
    pub(super) tier3_facts: Vec<Tier3FeatureFacts>,
    /// PG.B — package-wide plan-and-gate facts (closed plan catalog,
    /// subscription anchor, per-callable gate directives). `None` when
    /// the package authors no `plan` blocks and no `gate` directives.
    pub(super) plan_gate_facts: Option<lazuli_analyzer::PlanGateFacts>,
}

impl DoctorPackage {
    pub(super) fn load(input: &Path, security_profile: SecurityProfile) -> Result<Self> {
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
    pub(super) fn coverage_inputs(&self) -> (Vec<lazuli_ir::Feature>, Vec<lazuli_doctor::coverage::LzxViewRef>) {
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
    pub(super) fn coverage_preset(&self) -> Option<lazuli_doctor::coverage::CoveragePreset> {
        use lazuli_doctor::coverage::CoveragePreset;
        self.lazurite_manifest
            .as_ref()
            .and_then(|m| m.doctor.as_ref())
            .and_then(|d| d.coverage.as_ref())
            .and_then(|cov| cov.preset.as_deref())
            .and_then(CoveragePreset::parse)
    }
}
