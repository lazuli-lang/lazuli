    // Package fixture helpers extracted from .
    // Tightly coupled to  internals — kept as 
    // re-exports via the parent's .

    use std::path::PathBuf;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;

    use lazuli_analyzer::lower_feature_skeleton;
    use lazuli_syntax::{self, parse_feature_skeletons};

    use crate::doctor::*;

    pub(super) fn package_from_sources(sources: Vec<(&str, &str)>) -> DoctorPackage {
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
                                    defaults_policy: feature.defaults.policy.clone(),
                                    defaults_timestamps: feature.defaults.timestamps,
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
                                    policies: feature.policies.clone(),
                                    extensions: feature.extensions.clone(),
                                    reports: feature.reports.clone(),
                                    report_lines,
                                    resources: feature.resources.clone(),
                                    report_decls: skeleton.reports.clone(),
                                    aggregates: feature.aggregates.clone(),
                                    aggregate_lines: BTreeMap::new(),
                                    errors: feature.errors.clone(),
                                    uses: feature.uses.clone(),
                                    channels: feature.channels.clone(),
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
                                    policies: feature.policies.clone(),
                                    extensions: feature.extensions.clone(),
                                    reports: feature.reports.clone(),
                                    report_lines: BTreeMap::new(),
                                    resources: feature.resources.clone(),
                                    report_decls: skeleton.reports.clone(),
                                    aggregates: feature.aggregates.clone(),
                                    aggregate_lines: BTreeMap::new(),
                                    errors: feature.errors.clone(),
                                    uses: feature.uses.clone(),
                                    channels: feature.channels.clone(),
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
            // v2 — Strict-default severity config to match
            // `security_profile` above. `package_from_sources_with_manifest`
            // overwrites this with a config resolved from the test
            // manifest's `[doctor]` section.
            config: lazuli_doctor_config::ResolvedDoctorConfig::default(),
            single_file_input: true,
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
            injected: Vec::new(),
        }
    }

    pub(super) fn package_from_sources_with_manifest(
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
        let manifest: lazuli_manifest::lazurite_manifest::Manifest =
            toml::from_str(manifest_source).unwrap();
        // v2 — the severity config now drives every preset/override
        // decision. Build it from the SAME manifest `[doctor]` section
        // the test wrote, at the package's Strict profile, so
        // preset/override-driven test assertions keep their behavior.
        package.config = lazuli_doctor_config::ResolvedDoctorConfig::from_doctor(
            manifest.doctor.as_ref(),
            package.security_profile,
        );
        package.lazurite_manifest = Some(manifest);
        package
    }

    pub(super) fn minimal_manifest(extra: &str) -> String {
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
