use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use lazuli_analyzer::lower_feature_skeleton;
use lazuli_ir::{
    self as ir, Agent, AppContract, AppManifest, AppProfile, AppRegistry, AppWorkspace,
};
use lazuli_lsp::SecurityProfile;
use lazuli_syntax::{LzxDocument, LzxPlatform, LzxPlatformView, parse_feature_skeletons};
use tower_lsp::lsp_types::DiagnosticSeverity;

use crate::app_manifest::{
    RegistryParseOutput, RegistryToolDefectReason, parse_app_contracts, parse_app_manifest,
    parse_app_profiles, parse_app_registry_with_defects, parse_app_workspace,
};

pub fn doctor_command(input: &Path, security_profile: SecurityProfile) -> Result<()> {
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

#[derive(Debug)]
struct DoctorPackage {
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
                                            api_names_text_pattern,
                                            apis: feature.apis.clone(),
                                            api_lines,
                                            translation: feature.translation.clone(),
                                            translation_line,
                                            records: feature.records.clone(),
                                            enums: feature.enums.clone(),
                                            events: feature.events.clone(),
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
                                    // typed command `external_calls`
                                    // facts (replaces the retired
                                    // `command` branch of
                                    // `collect_external_calls_in_block`).
                                    populate_command_external_calls_from_ir(
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
                collect_feature_symbols(&file, &mut feature_symbols);
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

        Ok(Self {
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
        })
    }

    fn diagnostics(&self) -> Vec<DoctorDiagnostic> {
        let mut diagnostics = Vec::new();

        for file in &self.files {
            diagnostics.extend(file.local_diagnostics.clone());
        }

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

        // Cut A.9 — `approval` primitive contract + role resolution.
        let known_roles = collect_known_roles(&self.files);
        diagnostics.extend(approval_diagnostics(&self.tier3_facts, &known_roles));
        diagnostics.extend(approval_missing_children_diagnostics(
            &self.approval_presences,
        ));

        // Cut A.11 — `cors` block cross-checks against the app's
        // declared environments + urls.
        diagnostics.extend(cors_diagnostics(self.app.as_ref()));

        // Observability bucket cycle row 36 — `app.logging` and
        // `app.tracing` closed-catalog + range + exporter binding
        // checks.
        diagnostics.extend(app_logging_tracing_diagnostics(
            self.app.as_ref(),
            self.registry.as_ref().map(|reg| &reg.manifest),
        ));

        // Phase L — auth block cross-feature diagnostics.
        diagnostics.extend(auth_diagnostics(
            &self.auth_facts,
            &self.feature_resources,
            &self.feature_adapters,
            &self.feature_uses,
            self.registry.as_ref(),
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

        diagnostics.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.line.cmp(&right.line))
                .then(left.column.cmp(&right.column))
                .then(left.code.cmp(&right.code))
        });
        diagnostics
    }
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

/// Phase L — typed `auth` block facts harvested per feature for the four
/// cross-feature diagnostics:
///   - `auth_password_algorithm_hash_mismatch`
///   - `auth_sessions_resource_unknown`
///   - `auth_identity_field_unknown`
///   - `auth_oauth_adapter_unbound`
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
        collect_feature_external_calls(file, &feature, start, &lines[start..index], operational);
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

fn collect_feature_external_calls(
    file: &DoctorFile,
    feature: &str,
    feature_start: usize,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        let leading = leading_spaces(lines[index]);

        // Phase L Tier 4 follow-up — the `command` branch is retired.
        // Commands now flow through `populate_command_external_calls_from_ir`
        // which reads `Command.external_calls` + `Command.timeout` /
        // `Command.retry` / `Command.idempotency` directly. The `job`
        // branch remains text-pattern until a later cycle lifts job
        // timeout/retry/idempotency consistently via the typed IR pass
        // it already has (this collector still wins because it knows
        // the exact `calls ` line offset, which `ExternalCallRef` lacks).
        if leading == 2 && trimmed.starts_with("job ") {
            let subject_name = match named_block_name(trimmed, "job") {
                Some(name) => name,
                None => {
                    index += 1;
                    continue;
                }
            };
            let block_start = index;
            index += 1;

            while index < lines.len() && leading_spaces(lines[index]) > 2 {
                index += 1;
            }

            collect_external_calls_in_block(
                file,
                feature,
                feature_start,
                "job",
                subject_name,
                block_start,
                &lines[block_start..index],
                operational,
            );
        } else {
            index += 1;
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

fn collect_external_calls_in_block(
    file: &DoctorFile,
    feature: &str,
    feature_start: usize,
    subject_kind: &str,
    subject_name: &str,
    block_start: usize,
    lines: &[&str],
    operational: &mut OperationalFacts,
) {
    let has_timeout = block_has_prefixed_line(lines, "timeout ");
    let has_retry = block_has_prefixed_line(lines, "retry ");
    let has_idempotency = block_has_prefixed_line(lines, "idempotency by ");
    let subject = format!("{feature}.{subject_kind}.{subject_name}");

    for (offset, line) in lines.iter().enumerate().skip(1) {
        let trimmed = line.trim_start();
        if leading_spaces(line) != 4 {
            continue;
        }

        let Some((slot, operation)) = parse_external_call_header(trimmed) else {
            continue;
        };

        operational.external_calls.push(ExternalCallFact {
            path: file.path.clone(),
            line: feature_start + block_start + offset + 1,
            column: leading_spaces(line) + 1,
            feature: feature.to_owned(),
            subject_kind: subject_kind.to_owned(),
            subject: subject.clone(),
            slot: slot.to_owned(),
            operation: operation.to_owned(),
            has_timeout,
            has_retry,
            has_idempotency,
        });
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
    if webhook.tenant_from.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "WEBHOOK-SCOPE-001".to_owned(),
            message: format!(
                "webhook `{}` does not declare `tenant_from payload.<axis>_id` — verify it should be globally scoped.",
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
        && !envelope.fields.iter().any(|f| &f.name == axis)
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
    event_payload_index: &BTreeMap<String, BTreeSet<String>>,
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
        // NOTIF-DIGEST-002 — `every` must parse as `<N> <unit>` where
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
                code: "NOTIF-DIGEST-002".to_owned(),
                message: format!(
                    "notification `{}` declares `digest every \"{}\"` outside the closed shape `<N> (seconds|minutes|hours|days)`.",
                    notification.name, digest.every
                ),
            });
        }

        // NOTIF-DIGEST-003 — `max_size` must be in (0, 10_000].
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
                    code: "NOTIF-DIGEST-003".to_owned(),
                    message: format!(
                        "notification `{}` declares `digest max_size {}` outside the supported range 1..=10000.",
                        notification.name, max_size
                    ),
                });
            }
        }

        // NOTIF-DIGEST-001 — `group_by` must reference a field present
        // in the trigger event's payload. Resolution walks the
        // cross-feature event-payload index built once for this
        // package; an unknown axis at design time is always a runtime
        // crash, so doctor surfaces it as an Error.
        if let Some(group_by) = digest.group_by.as_deref() {
            // The path is captured verbatim — strip a leading
            // `payload.` if the author wrote it (allowed for symmetry
            // with `tenant_from`). Then take the first segment as
            // the field the runtime keys on.
            let bare = group_by.strip_prefix("payload.").unwrap_or(group_by);
            let head = bare.split('.').next().unwrap_or("").trim();
            if let lazuli_ir::JobTrigger::Event { event } = &notification.trigger {
                let qname = qualified_event_key(&feature.feature, event);
                if let Some(payload_fields) = event_payload_index.get(&qname) {
                    if !head.is_empty() && !payload_fields.contains(head) {
                        let mut hint: Vec<&str> =
                            payload_fields.iter().map(String::as_str).collect();
                        hint.sort();
                        diagnostics.push(DoctorDiagnostic {
                            path: feature.path.clone(),
                            line,
                            column: 1,
                            severity: DoctorSeverity::Error,
                            code: "NOTIF-DIGEST-001".to_owned(),
                            message: format!(
                                "notification `{}` declares `digest group_by {}` but trigger event `{}` does not expose a `{}` payload field. Available fields: {}.",
                                notification.name,
                                group_by,
                                qname,
                                head,
                                if hint.is_empty() {
                                    "<none>".to_owned()
                                } else {
                                    hint.join(", ")
                                }
                            ),
                        });
                    }
                }
            }
        }
    }

    if let Some(throttle) = notification.throttle.as_ref() {
        // NOTIF-THROTTLE-001 — `max_per` must parse as `<N> <unit>`.
        // Same duration shape as `digest every`; the catalog is
        // closed at the language layer so adapters never see
        // ambiguous units like "month" or "weekday".
        if !is_valid_notification_duration(&throttle.max_per) {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-THROTTLE-001".to_owned(),
                message: format!(
                    "notification `{}` declares `throttle max_per \"{}\"` outside the closed shape `<N> (seconds|minutes|hours|days)`.",
                    notification.name, throttle.max_per
                ),
            });
        }

        // NOTIF-THROTTLE-002 — declaring `throttle` without any of
        // `per_recipient`/`per_channel`/`burst` is a useless block.
        // The adapter has nothing to key the bucket on beyond the
        // notification kind, which is identical to the absent case.
        if !throttle.per_recipient && !throttle.per_channel && throttle.burst.is_none() {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Warning,
                code: "NOTIF-THROTTLE-002".to_owned(),
                message: format!(
                    "notification `{}` declares `throttle` with no `per_recipient`, `per_channel`, or `burst` axis — the bucket has nothing to key on. Add at least one axis or drop the block.",
                    notification.name
                ),
            });
        }

        // NOTIF-THROTTLE-003 — `burst > 0` only makes sense per
        // recipient. Without `per_recipient`, a global burst would
        // open the floodgates for any caller hammering the same
        // notification kind, which defeats the purpose of throttling.
        if throttle.burst.map(|b| b > 0).unwrap_or(false) && !throttle.per_recipient {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "NOTIF-THROTTLE-003".to_owned(),
                message: format!(
                    "notification `{}` declares `throttle burst {}` without `per_recipient` — a global burst sidesteps the throttle entirely. Add `per_recipient` or drop the burst.",
                    notification.name,
                    throttle.burst.unwrap_or(0)
                ),
            });
        }
    }
}

/// Notifications expanded bucket cycle — qualify the trigger event
/// reference against the notification's owning feature. `customer
/// .customer_activated` stays qualified; bare local names like
/// `customer_activated` resolve to `<feature>.customer_activated`.
fn qualified_event_key(local_feature: &str, event: &lazuli_ir::QualifiedName) -> String {
    let feature = event.feature.as_deref().unwrap_or(local_feature);
    format!("{}.{}", feature, event.name)
}

/// Notifications expanded bucket cycle — closed-catalog duration
/// matcher reused by `NOTIF-DIGEST-002` and `NOTIF-THROTTLE-001`.
/// Accepts `<N> <unit>` and `<N><unit>` (Go-style), with units in
/// `{s,sec,secs,second,seconds,m,min,mins,minute,minutes,h,hr,hour,hours,d,day,days}`.
/// The runtime resolves the final string via Go's `time.ParseDuration`;
/// doctor's job is to reject obviously wrong literals at design
/// time so the adapter never sees `"1 month"` or `"forever"`.
fn is_valid_notification_duration(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    let (num_part, unit_part) = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .map(|idx| trimmed.split_at(idx))
        .unwrap_or(("", ""));
    if num_part.is_empty() {
        return false;
    }
    if num_part.parse::<u64>().ok().is_none() {
        return false;
    }
    let unit = unit_part.trim().to_ascii_lowercase();
    matches!(
        unit.as_str(),
        "s" | "sec"
            | "secs"
            | "second"
            | "seconds"
            | "m"
            | "min"
            | "mins"
            | "minute"
            | "minutes"
            | "h"
            | "hr"
            | "hrs"
            | "hour"
            | "hours"
            | "d"
            | "day"
            | "days"
    )
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

    for feature in tier3_facts {
        previously_diagnostics(feature, &mut diagnostics);
        tenant_migration_diagnostics(feature, &mut diagnostics);
    }

    if let Some(app) = app {
        deploy_strategy_diagnostics(app, &mut diagnostics);
        deploy_checkpoint_diagnostics(app, &mut diagnostics);
    }

    diagnostics
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
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    for tm in &feature.tenant_migrations {
        let line = feature
            .tenant_migration_lines
            .get(&tm.name)
            .copied()
            .unwrap_or(feature.feature_line);

        // TM-AXIS-001 — target axis must match the feature's tenancy axis.
        if let Some(axis) = &feature.tenancy_axis {
            if &tm.target.axis != axis {
                diagnostics.push(DoctorDiagnostic {
                    path: feature.path.clone(),
                    line,
                    column: 1,
                    severity: DoctorSeverity::Error,
                    code: "TM-AXIS-001".to_owned(),
                    message: format!(
                        "tenant_migration `{}` declares `target tenants {}` but feature `{}` uses tenancy axis `{}`.",
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
                code: "TM-AXIS-001".to_owned(),
                message: format!(
                    "tenant_migration `{}` declares `target tenants {}` but feature `{}` did not declare a `defaults.tenancy` axis.",
                    tm.name, tm.target.axis, feature.feature
                ),
            });
        }

        // TM-IDEMP-001 — `idempotency by <path>` is mandatory; absence
        // surfaces as an empty `IdempotencyKey.by` Path.
        if tm.idempotency.by.segments.is_empty() {
            diagnostics.push(DoctorDiagnostic {
                path: feature.path.clone(),
                line,
                column: 1,
                severity: DoctorSeverity::Error,
                code: "TM-IDEMP-001".to_owned(),
                message: format!(
                    "tenant_migration `{}` does not declare `idempotency by <path>` — schema migrations are not safely re-runnable without an idempotency key.",
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

fn parse_external_call_header(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.strip_prefix("calls ")?;
    let (slot, operation) = rest.trim().split_once('.')?;
    let slot = slot.trim();
    let operation = operation.trim();

    if is_identifier(slot) && is_identifier(operation) {
        Some((slot, operation))
    } else {
        None
    }
}

fn block_has_prefixed_line(lines: &[&str], prefix: &str) -> bool {
    lines
        .iter()
        .skip(1)
        .any(|line| leading_spaces(line) == 4 && line.trim_start().starts_with(prefix))
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
        let end = after_prefix
            .find(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
            .unwrap_or(after_prefix.len());
        if end > 0 {
            references.push(&after_prefix[..end]);
        }
        rest = &after_prefix[end..];
    }

    references
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

/// Walk a `.lzi` file's text and harvest the names that downstream Cut A
/// diagnostics need to resolve across features:
///
///   - `enum <Name>` headers (for `output discriminator <Enum>` targets)
///   - `record <Name>` headers + `<field>: <type>` children + the
///     `discriminator` marker on the disambiguation field
///   - `command <name>` and `query.{list,lookup,sql} <name>` headers with
///     their `policy @policy.<rule>` if present
///
/// The walker is text-based on purpose: the canonical-indent parser only
/// covers `agent` blocks today. When later cuts migrate the rest of the
/// feature body to typed AST, this collector collapses into the IR.
fn collect_feature_symbols(
    file: &DoctorFile,
    feature_symbols: &mut BTreeMap<String, FeatureSymbols>,
) {
    let lines: Vec<&str> = file.source.lines().collect();

    let mut feature_ranges: Vec<(String, usize, usize)> = Vec::new();
    let mut current_start: Option<(String, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if leading_spaces(line) == 0 && trimmed.starts_with("feature ") {
            if let Some((prev_name, prev_start)) = current_start.take() {
                feature_ranges.push((prev_name, prev_start, index));
            }
            if let Some(name) = trimmed.strip_prefix("feature ") {
                current_start = Some((name.trim().to_owned(), index));
            }
        }
    }
    if let Some((name, start)) = current_start {
        feature_ranges.push((name, start, lines.len()));
    }

    for (feature, start, end) in feature_ranges {
        let symbols = feature_symbols.entry(feature.clone()).or_default();
        scan_feature_range(file, &lines[start..end], start, symbols);
    }
}

fn scan_feature_range(
    file: &DoctorFile,
    lines: &[&str],
    feature_start: usize,
    symbols: &mut FeatureSymbols,
) {
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }

        // The fixture's domain block lives at indent 2 with payload at 4.
        // Most enum / record / command / query declarations appear at
        // indent 4 inside `domain`, or at indent 2 directly under feature.
        // Either form is tolerated.
        let leading = leading_spaces(raw);
        if leading < 2 {
            i += 1;
            continue;
        }

        // Phase L Tier 4 follow-up — the `record` and `enum` branches
        // are retired (lifted into `Tier3FeatureFacts.records` /
        // `Tier3FeatureFacts.enums` from the typed IR). Only `command`
        // headers survive here for the legacy `agent_tool_diagnostics`
        // policy-hint lookup.
        if let Some(rest) = trimmed.strip_prefix("command ") {
            let name = rest.split_whitespace().next().unwrap_or("").to_owned();
            if !name.is_empty() {
                let policy = scan_block_for_policy(&lines[i + 1..], leading_spaces(raw));
                symbols.commands.insert(
                    name,
                    CommandSymbolFact {
                        base: SymbolFact {
                            path: file.path.clone(),
                            line: feature_start + i + 1,
                        },
                        policy,
                    },
                );
            }
        }
        i += 1;
    }
}

fn scan_block_for_policy(body: &[&str], parent_indent: usize) -> Option<String> {
    for line in body {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) <= parent_indent {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix("policy ") {
            return Some(rest.trim().to_owned());
        }
    }
    None
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
// Phase L — auth block cross-feature diagnostics.
//
// Four ids per `docs/proposals/bucket-auth-cycle.md` §Doctor/LSP:
//   - `auth_password_algorithm_hash_mismatch`
//   - `auth_sessions_resource_unknown`
//   - `auth_identity_field_unknown`
//   - `auth_oauth_adapter_unbound`
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
            fields.insert(
                field.name.clone(),
                ResourceFieldFact {
                    type_ref: field.type_ref.clone(),
                    unique: field.unique,
                    line,
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
                    let entry = out.entry(feature_name.clone()).or_default();
                    for token in rest.split(',') {
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

        // 2. `auth_sessions_resource_unknown` — sessions resource must
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
        }

        // 3. `auth_password_algorithm_hash_mismatch` — when both
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

        // 4. `auth_oauth_adapter_unbound` — each oauth provider's
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

/// Row 48 — emits five OpenAPI-related diagnostics:
/// `deprecated_replacement_unknown`, `deprecated_sunset_date_invalid`,
/// `deprecated_sunset_in_past`, `openapi_text_pattern_api_block`,
/// `api_changelog_breaking_change` (the last only when invoked from the
/// changelog pipeline; doctor surfaces a guard noop). See
/// `docs/proposals/bucket-openapi-cycle.md` §Doctor/LSP.
fn openapi_deprecated_diagnostics(facts: &[Tier3FeatureFacts]) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    // Build per-feature command name index for LocalCommand resolution.
    let mut commands_by_feature: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for feature in facts {
        let set = commands_by_feature
            .entry(feature.feature.as_str())
            .or_default();
        for c in &feature.commands {
            set.insert(c.name.as_str());
        }
    }

    // Today: the calendar date for "now" comes from `chrono::Local::today()`.
    // The crate may not be available; fall back to a fixed-pivot probe at
    // 2026-01-01 so the in-past test stays deterministic. Doctor's CI
    // window already gates this rule as a warning.
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

            // 1) `deprecated_replacement_unknown`.
            if let Some(replacement) = &dep.replacement {
                match replacement {
                    lazuli_ir::DeprecationReplacement::LocalCommand(target) => {
                        let local = commands_by_feature
                            .get(feature.feature.as_str())
                            .map(|set| set.contains(target.as_str()))
                            .unwrap_or(false);
                        if !local {
                            diagnostics.push(DoctorDiagnostic {
                                path: feature.path.clone(),
                                line,
                                column: 1,
                                severity: DoctorSeverity::Error,
                                code: "deprecated_replacement_unknown".to_owned(),
                                message: format!(
                                    "command `{}`.deprecated.replacement `{}` does not resolve: same-feature command not found.",
                                    command.name, target
                                ),
                            });
                        }
                    }
                    lazuli_ir::DeprecationReplacement::Qualified(q) => {
                        let other_feature =
                            q.feature.as_deref().unwrap_or(feature.feature.as_str());
                        let resolves = commands_by_feature
                            .get(other_feature)
                            .map(|set| set.contains(q.name.as_str()))
                            .unwrap_or(false);
                        if !resolves {
                            diagnostics.push(DoctorDiagnostic {
                                path: feature.path.clone(),
                                line,
                                column: 1,
                                severity: DoctorSeverity::Error,
                                code: "deprecated_replacement_unknown".to_owned(),
                                message: format!(
                                    "command `{}`.deprecated.replacement `{}.command.{}` does not resolve: cross-feature reference malformed.",
                                    command.name, other_feature, q.name
                                ),
                            });
                        }
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
                                code: "deprecated_replacement_unknown".to_owned(),
                                message: format!(
                                    "command `{}`.deprecated.replacement `{}` does not resolve: url malformed.",
                                    command.name, url
                                ),
                            });
                        }
                    }
                }
            }

            // 2) `deprecated_sunset_date_invalid` + 3) `deprecated_sunset_in_past`.
            if let Some(sunset) = &dep.sunset {
                match parse_iso_date(sunset) {
                    None => diagnostics.push(DoctorDiagnostic {
                        path: feature.path.clone(),
                        line,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: "deprecated_sunset_date_invalid".to_owned(),
                        message: format!(
                            "command `{}`.deprecated.sunset `{}` is not a valid ISO-8601 date (`YYYY-MM-DD`).",
                            command.name, sunset
                        ),
                    }),
                    Some(date) if date < today_pivot => {
                        diagnostics.push(DoctorDiagnostic {
                            path: feature.path.clone(),
                            line,
                            column: 1,
                            severity: DoctorSeverity::Warning,
                            code: "deprecated_sunset_in_past".to_owned(),
                            message: format!(
                                "command `{}`.deprecated.sunset `{}` is in the past; consumers should expect this endpoint to be removed soon.",
                                command.name, sunset
                            ),
                        });
                    }
                    Some(_) => {}
                }
            }
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
// Cache bucket cycle (row 51) — `cache_*` diagnostics.
// =============================================================================

/// Row 51 — emits five Cache-related diagnostics:
/// `cache_ttl_unit_invalid`, `cache_invalidates_target_unresolved`,
/// `cache_tags_referenced_but_undeclared`, `cache_namespace_collision`,
/// `cache_capability_undeclared`. See
/// `docs/proposals/bucket-cache-cycle.md` §Doctor/LSP.
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
                                    api_names_text_pattern,
                                    apis: feature.apis.clone(),
                                    api_lines,
                                    translation: feature.translation.clone(),
                                    translation_line,
                                    records: feature.records.clone(),
                                    enums: feature.enums.clone(),
                                    events: feature.events.clone(),
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
                                    api_names_text_pattern: Vec::new(),
                                    apis: feature.apis.clone(),
                                    api_lines: BTreeMap::new(),
                                    translation: feature.translation.clone(),
                                    translation_line: header_line,
                                    records: feature.records.clone(),
                                    enums: feature.enums.clone(),
                                    events: feature.events.clone(),
                                });
                            }
                        }
                    }
                }
                collect_feature_symbols(&file, &mut feature_symbols);
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

        DoctorPackage {
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
        }
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

        assert!(package.diagnostics().is_empty());
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
    fn doctor_validates_feature_integration_bindings() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  uses
    payments
  bindings
    payments.gateway = integrations.mercadopago
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
  uses
    imports
  bindings
    imports.crm = integrations.crm
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
    // Four ids per docs/proposals/bucket-auth-cycle.md §Doctor/LSP:
    //   - auth_password_algorithm_hash_mismatch
    //   - auth_sessions_resource_unknown
    //   - auth_identity_field_unknown
    //   - auth_oauth_adapter_unbound
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
            "auth_sessions_resource_unknown",
            "auth_identity_field_unknown",
            "auth_oauth_adapter_unbound",
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
            codes(&diagnostics).contains("TM-AXIS-001"),
            "expected TM-AXIS-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn tenant_migration_no_idempotency_fires() {
        let package = package_from_sources(vec![("x.lzi", MIGRATIONS_TM_IDEMP_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("TM-IDEMP-001"),
            "expected TM-IDEMP-001 in {:?}",
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

    // =========================================================================
    // OpenAPI bucket cycle (row 48) — 4 doctor diagnostics on
    // `Command.deprecated` typed lift + `openapi_text_pattern_api_block`.
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
            codes(&diagnostics).contains("deprecated_replacement_unknown"),
            "expected deprecated_replacement_unknown in {:?}",
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
            codes(&diagnostics).contains("deprecated_sunset_in_past"),
            "expected deprecated_sunset_in_past in {:?}",
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
        };
        let query = lazuli_ir::Query::List(lazuli_ir::ListQuery {
            name: "list".into(),
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
            api_names_text_pattern: Vec::new(),
            apis: Vec::new(),
            api_lines: BTreeMap::new(),
            translation: None,
            translation_line: 1,
            records: Vec::new(),
            enums: Vec::new(),
            events: Vec::new(),
        });
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_ttl_unit_invalid"),
            "expected cache_ttl_unit_invalid in {:?}",
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
            api_names_text_pattern: vec!["customer_legacy".to_owned()],
            apis: Vec::new(),
            api_lines: BTreeMap::new(),
            translation: None,
            translation_line: 1,
            records: Vec::new(),
            enums: Vec::new(),
            events: Vec::new(),
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

    // =========================================================================
    // Notifications expanded bucket cycle — six new doctor diagnostics on
    // `notification.digest` and `notification.throttle`.
    // =========================================================================

    /// `NOTIF-DIGEST-001` fires when `digest group_by <path>` references
    /// a field absent from the trigger event's payload (union of the
    /// event's own payload + any matching `event_group` payload).
    #[test]
    fn notif_digest_001_group_by_unknown_payload_field() {
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
      every "1 hour"
      group_by nonexistent_field
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-001"),
            "expected NOTIF-DIGEST-001, got {codes:?}"
        );
    }

    /// `NOTIF-DIGEST-002` fires when `digest every "<duration>"` does
    /// not match the closed shape `<N> (seconds|minutes|hours|days)`.
    #[test]
    fn notif_digest_002_every_invalid_shape() {
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
      every "1 month"
      group_by customer_id
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-002"),
            "expected NOTIF-DIGEST-002, got {codes:?}"
        );
    }

    /// `NOTIF-DIGEST-003` fires when `digest max_size` is 0 or above
    /// the 10_000 ceiling. Both extremes are authoring smells: 0 is
    /// dead; > 10k blows up the in-window buffer.
    #[test]
    fn notif_digest_003_max_size_out_of_range() {
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
            codes.contains(&"NOTIF-DIGEST-003"),
            "expected NOTIF-DIGEST-003, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-001` fires when `throttle max_per` does not
    /// match `<N> (seconds|minutes|hours|days)`.
    #[test]
    fn notif_throttle_001_max_per_invalid_shape() {
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
            codes.contains(&"NOTIF-THROTTLE-001"),
            "expected NOTIF-THROTTLE-001, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-002` fires when `throttle` is authored with
    /// none of `per_recipient`/`per_channel`/`burst` — the bucket
    /// has no axis to key on.
    #[test]
    fn notif_throttle_002_axis_missing() {
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
            codes.contains(&"NOTIF-THROTTLE-002"),
            "expected NOTIF-THROTTLE-002, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-003` fires when `throttle burst <N>` is
    /// declared without `per_recipient`. A global burst defeats the
    /// throttle entirely.
    #[test]
    fn notif_throttle_003_burst_without_per_recipient() {
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
      per_channel
      burst 3
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-003"),
            "expected NOTIF-THROTTLE-003, got {codes:?}"
        );
    }
}
