//! Doctor command test suite.
//!
//! Extracted verbatim from `doctor/mod.rs` to keep mod.rs lean.
//! Do not add new tests outside of this `mod tests { ... }` block - the
//! wrapper preserves string-literal indentation for the existing tests.

mod tests {
    use crate::doctor::aggregators::auth::auth_diagnostics;
    use crate::doctor::*;
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
            "MANIFEST-REQUIRED-001 should not fire without @lazuli/plugin-* refs"
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

        result.expect("doctor should pass without Lazurite.toml when no @lazuli/plugin-* refs");
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
    policy @lazuli/plugin-payments
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
        let root =
            std::env::temp_dir().join(format!("lazuli-doctor-single-file-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp doctor project");

        // Sibling file in the parent dir uses @lazuli/plugin-payments, but we
        // are NOT going to invoke the doctor on it.
        fs::write(
            root.join("sibling.lzi"),
            r#"
feature billing
  command charge
    policy @lazuli/plugin-payments
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
            .map(|d| DoctorDiagnostic::from_lsp(PathBuf::from("registry.lzi"), d).code)
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
                d.line == 4 && (d.code == "app-env-contract" || d.code == "env-schema-contract")
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
    policy @lazuli/plugin-payments
"#,
            )],
            &minimal_manifest(""),
        );
        let diagnostics = package.diagnostics();
        assert!(codes(&diagnostics).contains("PLUGIN-NOT-DECLARED-001"));
    }

    #[test]
    fn doctor_emits_semantic_plugin_001_for_unknown_alias_without_plugin() {
        // B3 — `@semantic.BrazilianCPF` reference with no plugin in
        // `Lazurite.toml [plugins]` that declares the alias should
        // surface SEMANTIC-PLUGIN-001. See
        // `docs/proposals/semantic-types-plugin-locales.md` §New diagnostics.
        let package = package_from_sources_with_manifest(
            vec![(
                "host.lzi",
                r#"
feature host
  domain
    resource Host
      cpf: @semantic.BrazilianCPF optional
"#,
            )],
            &minimal_manifest(""),
        );
        let diagnostics = package.diagnostics();
        // The legacy `semantic_type_unknown` may also fire when no
        // alias map resolves the name; both share the same root cause
        // and either signals the gap to the author.
        let signals: Vec<&str> = diagnostics
            .iter()
            .filter(|d| {
                (d.code == "SEMANTIC-PLUGIN-001" || d.code == "semantic_type_unknown")
                    && d.message.contains("BrazilianCPF")
            })
            .map(|d| d.code.as_str())
            .collect();
        assert!(
            !signals.is_empty(),
            "expected SEMANTIC-PLUGIN-001 or semantic_type_unknown, got: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_emits_plugin_unused_when_manifest_declares_unreferenced_plugin() {
        let package = package_from_sources_with_manifest(
            vec![("app.lzi", "app Demo\n")],
            &minimal_manifest(
                r#"
[plugins]
"@lazuli/plugin-payments" = { module = "example.com/payments", version = "v0.1.0" }
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
"@lazuli/plugin-payments" = { module = "example.com/payments", version = "v0.1.0" }
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

        // Filter out the `ERR-VOCAB-*` family — those are the new Cell
        // ANALYZE-1 warnings about missing `when_denied` overrides, not
        // related to the surface-to-command resolution this test pins.
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("ERR-VOCAB-")
                    && !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(diagnostics.is_empty(), "got: {:#?}", diagnostics);
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
        // abstract experience without breaking validation". The new
        // `ERR-VOCAB-*` family (Cell ANALYZE-1) is also filtered here:
        // those nudge authors toward customized `when_denied` text but
        // do not block the surface-to-command resolution pinned here.
        let diagnostics = package.diagnostics();
        let blocking: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                matches!(d.severity, DoctorSeverity::Error | DoctorSeverity::Warning)
                    && !d.code.starts_with("ERR-VOCAB-")
                    && !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT"
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

        // Filter out the `ERR-VOCAB-*` family — Cell ANALYZE-1 warnings
        // about missing `when_denied` overrides are orthogonal to the
        // route-binding-from-context behavior this test pins.
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("ERR-VOCAB-")
                    && !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(diagnostics.is_empty(), "got: {:#?}", diagnostics);
    }

    #[test]
    fn doctor_reports_app_manifest_operational_gaps() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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

        assert!(
            package
                .diagnostics()
                .into_iter()
                .filter(|d| !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT")
                .collect::<Vec<_>>()
                .is_empty()
        );
    }

    #[test]
    fn doctor_uses_registry_for_env_and_capabilities() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
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

        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();

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
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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

        assert!(
            package
                .diagnostics()
                .into_iter()
                .filter(|d| !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT")
                .collect::<Vec<_>>()
                .is_empty()
        );
    }

    #[test]
    fn doctor_resolves_features_and_requirements_from_enabled_packs() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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

        let leftover: Vec<_> = valid
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected external call contract to pass doctor: {:#?}",
            leftover
        );

        let invalid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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

        let leftover: Vec<_> = valid
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected profile contract to pass doctor: {:#?}",
            leftover
        );

        let invalid = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
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
  lazuli_version "0.16"
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

        let leftover: Vec<_> = valid
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected valid workspace contract to pass doctor: {:#?}",
            leftover
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

        let leftover: Vec<_> = valid
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected external contract to pass doctor: {:#?}",
            leftover
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

    // Z2 — `design-custom-*` doctor integration (3 rules over `design.lzi` IR).
    #[test]
    fn doctor_design_custom_lints_fire_for_collisions_reserved_and_bad_hex() {
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        // `design.lzi` with: a custom token that collides with a color group
        // entry, a reserved Shadcn-semantic name, and an invalid hex value.
        // We need the allowlist file present so `design_token_diagnostics`
        // doesn't bail out early — emit a minimal stub.
        write_file(
            &root.join("dist/ts-web/design/allowlist.json"),
            r#"{"bg":["brand-blue"],"text":[],"font":[]}"#,
        );
        write_file(
            &root.join("design.lzi"),
            r##"design hostpoint
  color
    brand-blue "#28bbdd"
  custom
    brand-blue "#28bbdd"
    primary "#7c3aed"
    oops "not-a-color"
"##,
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);

        assert!(
            surfaced.contains("design-custom-duplicate"),
            "expected duplicate diagnostic; got {:?}",
            surfaced,
        );
        assert!(
            surfaced.contains("design-custom-reserved-name"),
            "expected reserved-name diagnostic; got {:?}",
            surfaced,
        );
        assert!(
            surfaced.contains("design-custom-invalid-value"),
            "expected invalid-value diagnostic; got {:?}",
            surfaced,
        );
    }

    #[test]
    fn doctor_design_custom_lints_silent_on_clean_design() {
        // Regression: a `design.lzi` with a well-formed custom group should
        // NOT fire any `design-custom-*` diagnostic.
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        write_file(
            &root.join("dist/ts-web/design/allowlist.json"),
            r#"{"bg":["primary","chat-bubble-mine"],"text":[],"font":[]}"#,
        );
        write_file(
            &root.join("design.lzi"),
            r##"design hostpoint
  color
    primary "#28bbdd"
  custom
    chat-bubble-mine "#dcf8c6" dark "#005c4b"
    map-marker-active "#ff5722"
"##,
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);
        assert!(
            !surfaced.contains("design-custom-duplicate"),
            "unexpected duplicate diagnostic on clean design; got {:?}",
            surfaced,
        );
        assert!(
            !surfaced.contains("design-custom-reserved-name"),
            "unexpected reserved-name diagnostic on clean design; got {:?}",
            surfaced,
        );
        assert!(
            !surfaced.contains("design-custom-invalid-value"),
            "unexpected invalid-value diagnostic on clean design; got {:?}",
            surfaced,
        );
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
    fn doctor_query_view_reports_missing_sql_file() {
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        write_file(
            &root.join("app/features/host/host.lzi"),
            r#"
feature host
  record HostHomeRow
    id: ID required
  query.view host_home_view
    returns list of HostHomeRow
    source @file.host_home_view.sql
    params
      user_id: ID required
"#,
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);

        assert!(
            surfaced.contains("QUERY-VIEW-SQL-FILE-001"),
            "expected missing SQL file diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_query_view_reports_unsafe_sql_pattern() {
        let temp = tempfile::TempDir::new().expect("create tempdir");
        let root = temp.path();

        write_file(&root.join("app.lzi"), "app Acme\n");
        write_file(
            &root.join("app/features/host/host.lzi"),
            r#"
feature host
  record HostHomeRow
    id: ID required
  query.view host_home_view
    returns list of HostHomeRow
    source @file.host_home_view.sql
    params
      user_id: ID required
"#,
        );
        write_file(
            &root.join("app/features/host/queries/host_home_view.sql"),
            "select id from host_rows where title like '%' + $1 + '%'\n",
        );

        let package = DoctorPackage::load(root, SecurityProfile::Strict).expect("load package");
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);

        assert!(
            surfaced.contains("QUERY-VIEW-SQL-UNSAFE-001"),
            "expected unsafe SQL diagnostic; got {:?}",
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

    const SEMANTIC_UNKNOWN_FIXTURE: &str =
        include_str!("../../tests/fixtures/semantic_unknown.lzi");

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

    // -------------------------------------------------------------------------
    // SCOPE-OWNER-COLUMN-001 — warn when @scope.owner / @scope.same_org
    // policy is declared but the targeted resource has no matching column.
    // Mirrors the codegen-side silent-skip so authors see the gap at design
    // time. (Hostpoint pilot 2026-05-17 evidence.)
    // -------------------------------------------------------------------------

    #[test]
    fn doctor_warns_scope_owner_when_resource_has_no_owner_column() {
        let package = package_from_sources(vec![(
            "trust.lzi",
            r#"
feature trust
  policies
    update: @scope.owner

  domain
    resource Review
      status: Text required

  command flag
    route id: ID
    input
      reason: Text required
    policy @policy.update
    updates Review
      status = "flagged"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let scope_diags: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code == "SCOPE-OWNER-COLUMN-001")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            !scope_diags.is_empty(),
            "expected SCOPE-OWNER-COLUMN-001 on Review with no owner column; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            scope_diags[0].contains("@scope.owner"),
            "message should name the offending atom: {}",
            scope_diags[0]
        );
        assert!(
            scope_diags[0].contains("Review"),
            "message should name the resource: {}",
            scope_diags[0]
        );
    }

    #[test]
    fn doctor_accepts_scope_owner_when_resource_has_user_id_column() {
        let package = package_from_sources(vec![(
            "account.lzi",
            r#"
feature account
  policies
    delete: @scope.owner

  domain
    resource UserSession
      user_id: ID required
      token: Text required

  command revoke
    route id: ID
    input
      id: ID required
    policy @policy.delete
    deletes UserSession
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("SCOPE-OWNER-COLUMN-001"),
            "user_id should resolve @scope.owner; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_scope_same_org_when_resource_has_no_org_column() {
        let package = package_from_sources(vec![(
            "payments.lzi",
            r#"
feature payments
  policies
    update: @scope.same_org

  domain
    resource Charge
      amount: Integer required

  command flag
    route id: ID
    input
      id: ID required
    policy @policy.update
    updates Charge
      amount = 0
"#,
        )]);
        let diagnostics = package.diagnostics();
        let scope_diags: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code == "SCOPE-OWNER-COLUMN-001")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            !scope_diags.is_empty(),
            "expected SCOPE-OWNER-COLUMN-001 on Charge with no org column; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            scope_diags[0].contains("@scope.same_org"),
            "message should name same_org: {}",
            scope_diags[0]
        );
    }

    // -------------------------------------------------------------------------
    // field_derived_from_unresolved — Tier 4c lint per naming-reconciliation
    // proposal §4. Resource field's `derived from <expr>` must reference
    // sibling fields (or whitelisted keywords). Closes 1 of the 3 net-new
    // Tier 4c lints surfaced 2026-05-17.
    // -------------------------------------------------------------------------

    #[test]
    fn doctor_warns_derived_from_referencing_unknown_sibling() {
        let package = package_from_sources(vec![(
            "billing.lzi",
            r#"
feature billing
  domain
    resource Charge
      amount: Integer required
      is_premium: Boolean derived from total_amount > 1000
"#,
        )]);
        let diagnostics = package.diagnostics();
        let derived: Vec<&str> = diagnostics
            .iter()
            .filter(|d| d.code == "field_derived_from_unresolved")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            !derived.is_empty(),
            "expected field_derived_from_unresolved on Charge.is_premium; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            derived[0].contains("total_amount"),
            "message should name unresolved identifier `total_amount`: {}",
            derived[0]
        );
        assert!(
            derived[0].contains("is_premium"),
            "message should name the offending field: {}",
            derived[0]
        );
    }

    #[test]
    fn doctor_accepts_derived_from_referencing_sibling_field() {
        let package = package_from_sources(vec![(
            "billing.lzi",
            r#"
feature billing
  domain
    resource Charge
      amount: Integer required
      is_premium: Boolean derived from amount > 1000
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("field_derived_from_unresolved"),
            "sibling field `amount` should resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // -------------------------------------------------------------------------
    // resource_unique_qualifier_unknown + resource_validates_path_unknown —
    // Tier 4c lints per naming-reconciliation proposal §4 rows 1+2.
    //
    // Lint code shipped + ready, but `Resource.constraints` and
    // `Resource.validates` slots are not yet populated by
    // `lower_resource_decl` (`crates/lazuli_analyzer/src/lib.rs:2702-2718`
    // hardcodes both to empty Vec). The lint walkers stay silent until
    // the analyzer wires the lift from `ResourceDecl.validates` +
    // domain-level `constraints` block.
    //
    // The unit tests below were dropped because they would assert against
    // an empty IR slot. When the upstream lift lands, re-introduce the
    // tests by mirroring `doctor_warns_derived_from_referencing_unknown_sibling`
    // (which DOES work because `derived_from` is lifted).
    //
    // Tracked as a Tier 4c follow-up per the naming-reconciliation
    // proposal §"Net diagnostic action items" rows 1+2.
    // -------------------------------------------------------------------------

    #[test]
    fn doctor_accepts_derived_from_with_keywords_and_string_literals() {
        // `and` / `not` are keywords; "high" is a string literal —
        // none should be flagged as unresolved identifiers.
        let package = package_from_sources(vec![(
            "billing.lzi",
            r#"
feature billing
  domain
    resource Charge
      amount: Integer required
      status: Text required
      flagged: Boolean derived from amount > 1000 and status != "high"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("field_derived_from_unresolved"),
            "keywords + string literals must not be flagged; got {:?}",
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
        include_str!("../../tests/fixtures/migrations/previously_forward_unresolved.lzi");
    const MIGRATIONS_PREVIOUSLY_CYCLE_FIXTURE: &str =
        include_str!("../../tests/fixtures/migrations/previously_cycle.lzi");
    const MIGRATIONS_PREVIOUSLY_DUP_FIXTURE: &str =
        include_str!("../../tests/fixtures/migrations/previously_duplicate_claim.lzi");
    const MIGRATIONS_TM_AXIS_FIXTURE: &str =
        include_str!("../../tests/fixtures/migrations/tenant_migration_axis_unknown.lzi");
    const MIGRATIONS_TM_IDEMP_FIXTURE: &str =
        include_str!("../../tests/fixtures/migrations/tenant_migration_no_idempotency.lzi");
    const MIGRATIONS_CHECKPOINT_INVALID_FIXTURE: &str =
        include_str!("../../tests/fixtures/migrations/deploy_checkpoint_path_invalid.lzi");
    const MIGRATIONS_STRATEGY_INVALID_FIXTURE: &str =
        include_str!("../../tests/fixtures/migrations/deploy_strategy_invalid.lzi");
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
        include_str!("../../tests/fixtures/openapi/deprecated_replacement_unknown.lzi");
    const OPENAPI_SUNSET_DATE_INVALID_FIXTURE: &str =
        include_str!("../../tests/fixtures/openapi/deprecated_sunset_date_invalid.lzi");
    const OPENAPI_SUNSET_IN_PAST_FIXTURE: &str =
        include_str!("../../tests/fixtures/openapi/deprecated_sunset_in_past.lzi");
    const OPENAPI_TEXT_PATTERN_API_FIXTURE: &str =
        include_str!("../../tests/fixtures/openapi/text_pattern_api_block.lzi");

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
        include_str!("../../tests/fixtures/i18n/default_not_supported.lzi");
    const I18N_TRANSLATION_LOCALE_UNSUPPORTED_FIXTURE: &str =
        include_str!("../../tests/fixtures/i18n/translation_locale_unsupported.lzi");
    const I18N_TRANSLATION_KEY_UNRESOLVED_FIXTURE: &str =
        include_str!("../../tests/fixtures/i18n/translation_key_unresolved.lzi");
    const I18N_CLDR_PLURAL_ARM_INVALID_FIXTURE: &str =
        include_str!("../../tests/fixtures/i18n/cldr_plural_arm_invalid.lzi");
    const I18N_LOCALE_NEGOTIATE_SOURCE_INVALID_FIXTURE: &str =
        include_str!("../../tests/fixtures/i18n/locale_negotiate_source_invalid.lzi");

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
    // MISSING-POLICY-ON-QUERY-001 - query public fallback visibility.
    // =========================================================================

    const MISSING_POLICY_ON_QUERY_HAPPY_FIXTURE: &str =
        include_str!("../../tests/fixtures/missing-policy-on-query/happy.lzi");
    const MISSING_POLICY_ON_QUERY_MISSING_FIXTURE: &str =
        include_str!("../../tests/fixtures/missing-policy-on-query/missing.lzi");
    const MISSING_POLICY_ON_QUERY_EXPLICIT_PUBLIC_FIXTURE: &str =
        include_str!("../../tests/fixtures/missing-policy-on-query/explicit_public.lzi");

    #[test]
    fn missing_policy_on_query_happy_fixture_has_zero_diagnostics() {
        let package =
            package_from_sources(vec![("happy.lzi", MISSING_POLICY_ON_QUERY_HAPPY_FIXTURE)]);
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            diagnostics.is_empty(),
            "expected happy fixture to emit zero diagnostics, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn missing_policy_on_query_missing_fixture_fires_once() {
        let package = package_from_sources(vec![(
            "missing.lzi",
            MISSING_POLICY_ON_QUERY_MISSING_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "MISSING-POLICY-ON-QUERY-001"),
            1,
            "expected exactly one MISSING-POLICY-ON-QUERY-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn missing_policy_on_query_explicit_public_fixture_has_zero_diagnostics() {
        let package = package_from_sources(vec![(
            "explicit_public.lzi",
            MISSING_POLICY_ON_QUERY_EXPLICIT_PUBLIC_FIXTURE,
        )]);
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            diagnostics.is_empty(),
            "expected explicit public fixture to emit zero diagnostics, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn duplicate_query_name_author_duplicate_fires_once_through_doctor() {
        let package = package_from_sources(vec![(
            "catalog.lzi",
            r#"
feature catalog
  query.list list_customers
  query.list list_customers
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "DUPLICATE-QUERY-NAME-001")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one DUPLICATE-QUERY-NAME-001; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
        assert!(
            hits[0]
                .message
                .contains("feature 'catalog' declares query 'list_customers' more than once"),
            "message should name the feature and duplicate query; got {}",
            hits[0].message
        );
    }

    // =========================================================================
    // Cache bucket cycle (row 51) — 5 doctor diagnostics on QueryCache /
    // Command.invalidates / registry capabilities.
    // =========================================================================

    const CACHE_INVALIDATES_UNRESOLVED_FIXTURE: &str =
        include_str!("../../tests/fixtures/cache/invalidates_target_unresolved.lzi");
    const CACHE_NAMESPACE_COLLISION_FIXTURE: &str =
        include_str!("../../tests/fixtures/cache/namespace_collision.lzi");
    const CACHE_CAPABILITY_UNDECLARED_FIXTURE: &str =
        include_str!("../../tests/fixtures/cache/capability_undeclared.lzi");
    // CL.C.3 — feature-level `cache <name>` profile diagnostics.
    const CACHE_PROFILE_UNKNOWN_FIXTURE: &str =
        include_str!("../../tests/fixtures/cache/profile_unknown.lzi");
    const CACHE_TAG_UNKNOWN_FIXTURE: &str =
        include_str!("../../tests/fixtures/cache/tag_unknown.lzi");
    const CACHE_TTL_CONTRACT_SWR_FIXTURE: &str =
        include_str!("../../tests/fixtures/cache/ttl_contract_swr_exceeds.lzi");

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
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        });
        package.tier3_facts.push(Tier3FeatureFacts {
            feature: "customer".into(),
            path: PathBuf::from("x.lzi"),
            feature_line: 1,
            tenancy_axis: None,
            defaults_policy: None,
            defaults_timestamps: false,
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
            policies: lazuli_ir::Policies::default(),
            extensions: Vec::new(),
            reports: Vec::new(),
            report_lines: BTreeMap::new(),
            resources: Vec::new(),
            report_decls: Vec::new(),
            aggregates: Vec::new(),
            aggregate_lines: BTreeMap::new(),
            errors: None,
            uses: Vec::new(),
            channels: Vec::new(),
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
            defaults_policy: None,
            defaults_timestamps: false,
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
            policies: lazuli_ir::Policies::default(),
            extensions: Vec::new(),
            reports: Vec::new(),
            report_lines: BTreeMap::new(),
            resources: Vec::new(),
            report_decls: Vec::new(),
            aggregates: Vec::new(),
            aggregate_lines: BTreeMap::new(),
            errors: None,
            uses: Vec::new(),
            channels: Vec::new(),
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
                    access_ttl: None,
                    rotation: None,
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

    // =========================================================================
    // IR Error-Vocab (Cell ANALYZE-1) — fixture-driven coverage for the 7
    // `ERR-VOCAB-*` diagnostics. Each fixture trips exactly one rule (with
    // ERR-VOCAB-003 occasionally co-firing alongside ERR-VOCAB-001 on the
    // same source — both are legitimate, both warnings).
    //
    // The happy-path fixture asserts ZERO `ERR-VOCAB-*` codes fire.
    //
    // Cross-feature key resolution (`@translation.X` resolved through
    // `uses`) is exercised by `err_vocab_002_silent_through_uses_two_features`.
    //
    // See `docs/proposals/ir-error-messages-vocab.md` §6 §11 Cell ANALYZE-1.
    // =========================================================================

    const ERR_VOCAB_NO_WHEN_DENIED_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/no_when_denied.lzi");
    const ERR_VOCAB_KEY_UNKNOWN_FROM_POLICY_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/key_unknown_from_policy.lzi");
    const ERR_VOCAB_BUILTIN_FALLBACK_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/builtin_fallback.lzi");
    const ERR_VOCAB_CODE_UNKNOWN_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/code_unknown.lzi");
    const ERR_VOCAB_EXPOSE_UNKNOWN_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/expose_unknown.lzi");
    const ERR_VOCAB_WHEN_DENIED_NO_POLICY_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/when_denied_no_policy.lzi");
    const ERR_VOCAB_EXPOSE_5XX_MESSAGE_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/expose_5xx_message.lzi");
    const ERR_VOCAB_HAPPY_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/happy.lzi");
    const ROUTE_GUARD_HAPPY_LZI: &str = include_str!("../../tests/fixtures/route-guard/happy.lzi");
    const ROUTE_GUARD_HAPPY_LZX: &str = include_str!("../../tests/fixtures/route-guard/happy.lzx");
    const ROUTE_GUARD_UNGUARDED_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/view_unguarded_with_gated_backend.lzx");
    const ROUTE_GUARD_LAXER_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/view_laxer_than_backend.lzx");
    const ROUTE_GUARD_REDIRECT_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/redirect_unreachable.lzx");
    const ROUTE_GUARD_MISSING_ACTOR_LZI: &str =
        include_str!("../../tests/fixtures/route-guard/missing_actor_query.lzi");
    const ROUTE_GUARD_MISSING_ACTOR_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/missing_actor_query.lzx");
    const ROUTE_GUARD_AUDIENCE_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/audience_runtime_disagreement.lzx");
    const LIFECYCLE_GATE_HAPPY_LZI: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/happy.lzi");
    const LIFECYCLE_GATE_HAPPY_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/happy.lzx");
    const LIFECYCLE_GATE_UNKNOWN_RESOURCE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/unknown_resource.lzx");
    const LIFECYCLE_GATE_UNKNOWN_STATE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/unknown_state.lzx");
    const LIFECYCLE_GATE_MISSING_STATE_COVERAGE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/missing_state_coverage.lzx");
    const LIFECYCLE_GATE_EXTRA_STATE_ARM_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/extra_state_arm.lzx");
    const LIFECYCLE_GATE_WILDCARD_OVERUSE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/wildcard_overuse.lzx");
    const LIFECYCLE_GATE_REDIRECT_CYCLE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/redirect_cycle.lzx");
    const LIFECYCLE_GATE_RESUME_RESOURCE_MISMATCH_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/resume_resource_mismatch.lzx");
    const LIFECYCLE_GATE_WRONG_QUERY_KIND_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/wrong_query_kind.lzx");
    const LIFECYCLE_GATE_WITHOUT_ACTOR_GATE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/lifecycle_without_actor_gate.lzx");
    const LIFECYCLE_GATE_CROSS_FEATURE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/cross_feature_resume.lzx");

    fn err_vocab_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("ERR-VOCAB-"))
            .collect()
    }

    fn count_code(diagnostics: &[DoctorDiagnostic], code: &str) -> usize {
        diagnostics.iter().filter(|d| d.code == code).count()
    }

    fn route_guard_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("ROUTE-GUARD-"))
            .collect()
    }

    fn lifecycle_gate_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("LIFECYCLE-GATE-"))
            .collect()
    }

    fn route_guard_fixture(lzx: &str) -> DoctorPackage {
        package_from_sources(vec![
            ("happy.lzi", ROUTE_GUARD_HAPPY_LZI),
            ("case.lzx", lzx),
        ])
    }

    fn lifecycle_gate_fixture(lzx: &str) -> DoctorPackage {
        package_from_sources(vec![
            ("happy.lzi", LIFECYCLE_GATE_HAPPY_LZI),
            ("case.lzx", lzx),
        ])
    }

    #[test]
    fn err_vocab_001_fires_for_no_when_denied_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_NO_WHEN_DENIED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-001"),
            1,
            "expected ERR-VOCAB-001 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_002_fires_for_key_unknown_from_policy_fixture() {
        let package =
            package_from_sources(vec![("app.lzi", ERR_VOCAB_KEY_UNKNOWN_FROM_POLICY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-002"),
            1,
            "expected ERR-VOCAB-002 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_003_fires_for_builtin_fallback_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_BUILTIN_FALLBACK_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-003"),
            1,
            "expected ERR-VOCAB-003 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_code_unknown_fires_for_code_unknown_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_CODE_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-CODE-UNKNOWN"),
            1,
            "expected ERR-VOCAB-CODE-UNKNOWN to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_expose_unknown_fires_for_expose_unknown_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_EXPOSE_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-EXPOSE-UNKNOWN"),
            1,
            "expected ERR-VOCAB-EXPOSE-UNKNOWN to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_when_denied_no_policy_fires_for_when_denied_no_policy_fixture() {
        let package =
            package_from_sources(vec![("app.lzi", ERR_VOCAB_WHEN_DENIED_NO_POLICY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-WHEN-DENIED-NO-POLICY"),
            1,
            "expected ERR-VOCAB-WHEN-DENIED-NO-POLICY to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_expose_5xx_message_fires_for_expose_5xx_message_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_EXPOSE_5XX_MESSAGE_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-EXPOSE-5XX-MESSAGE"),
            1,
            "expected ERR-VOCAB-EXPOSE-5XX-MESSAGE to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_happy_fixture_fires_no_err_vocab_diagnostics() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_HAPPY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let err_vocab: Vec<_> = err_vocab_diags(&diagnostics);
        assert!(
            err_vocab.is_empty(),
            "happy.lzi must emit zero ERR-VOCAB-* diagnostics; got: {:?}",
            err_vocab.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Cross-feature key resolution: `feature sales` declares
    // `policies create.when_denied @translation.shared_key` and that key
    // lives in `feature crm`'s translation block. `feature sales`
    // imports it via `uses crm`. ERR-VOCAB-002 must stay silent.
    #[test]
    fn err_vocab_002_silent_through_uses_two_features() {
        const CRM_FIXTURE: &str = r#"
app AcmeApp
  title "Acme"
  version "0.1.0"
  targets
    backend go
  environments
    local
  locale
    default "pt-BR"
    supported "pt-BR"

feature crm
  domain
    resource Customer
      id: ID required

  translation
    catalog "./i18n/crm.<locale>.json"

    key shared_key
      pt-BR "Apenas administradores podem realizar esta ação."
"#;
        const SALES_FIXTURE: &str = r#"
feature sales
  uses crm
  domain
    resource Lead
      id: ID required

  policies
    create: @role.sales
      when_denied @translation.shared_key

  command create
    policy @policy.create
    creates Lead
"#;
        let package =
            package_from_sources(vec![("crm.lzi", CRM_FIXTURE), ("sales.lzi", SALES_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let err_vocab_002 = count_code(&diagnostics, "ERR-VOCAB-002");
        assert_eq!(
            err_vocab_002,
            0,
            "cross-feature `@translation.shared_key` (declared in `crm`, used by `sales`) must \
             resolve through `uses crm`; got ERR-VOCAB-002 diagnostics: {:?}",
            diagnostics
                .iter()
                .filter(|d| d.code == "ERR-VOCAB-002")
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    const AUTH_REFRESH_HAPPY: &str = include_str!("../../tests/fixtures/auth-refresh/happy.lzi");
    const AUTH_REFRESH_001: &str =
        include_str!("../../tests/fixtures/auth-refresh/missing_secret_provider.lzi");
    const AUTH_REFRESH_002: &str =
        include_str!("../../tests/fixtures/auth-refresh/grace_exceeds_refresh_ttl.lzi");
    const AUTH_REFRESH_003: &str =
        include_str!("../../tests/fixtures/auth-refresh/schema_missing_columns.lzi");
    const AUTH_REFRESH_004: &str =
        include_str!("../../tests/fixtures/auth-refresh/revoke_user_missing_user_fk.lzi");
    const AUTH_REFRESH_005: &str =
        include_str!("../../tests/fixtures/auth-refresh/refresh_ttl_long.lzi");
    const AUTH_REFRESH_006: &str =
        include_str!("../../tests/fixtures/auth-refresh/missing_on_refresh_failure.lzi");
    const AUTH_REFRESH_007: &str =
        include_str!("../../tests/fixtures/auth-refresh/auto_promotion_applied.lzi");
    const AUTH_REFRESH_008: &str =
        include_str!("../../tests/fixtures/auth-refresh/auto_refresh_not_surfaced.lzi");
    const AUTH_REFRESH_009: &str =
        include_str!("../../tests/fixtures/auth-refresh/cookie_domain_missing.lzi");

    fn auth_refresh_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("AUTH-REFRESH-"))
            .collect()
    }

    fn assert_auth_refresh_fixture(source: &str, expected_code: &str) -> Vec<DoctorDiagnostic> {
        let package = package_from_sources(vec![("auth_refresh.lzi", source)]);
        let diagnostics = package.diagnostics();
        let auth_refresh = auth_refresh_diags(&diagnostics);
        assert_eq!(
            auth_refresh.len(),
            1,
            "expected exactly one AUTH-REFRESH diagnostic ({expected_code}); got {:?}",
            auth_refresh
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
        assert_eq!(auth_refresh[0].code, expected_code);
        diagnostics
    }

    #[test]
    fn route_guard_happy_fixture_fires_no_route_guard_diagnostics() {
        let package = package_from_sources(vec![
            ("happy.lzi", ROUTE_GUARD_HAPPY_LZI),
            ("happy.lzx", ROUTE_GUARD_HAPPY_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert!(
            route_guard.is_empty(),
            "happy route guard fixtures must emit zero ROUTE-GUARD-* diagnostics; got: {:?}",
            route_guard
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn route_guard_001_fires_for_unguarded_gated_backend_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_UNGUARDED_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-001"]
        );
    }

    #[test]
    fn route_guard_002_fires_for_laxer_view_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_LAXER_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-002"]
        );
    }

    #[test]
    fn route_guard_003_fires_for_unreachable_redirect_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_REDIRECT_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-003"]
        );
    }

    #[test]
    fn route_guard_004_fires_as_warning_for_missing_actor_query_fixture() {
        let package = package_from_sources(vec![
            ("missing_actor_query.lzi", ROUTE_GUARD_MISSING_ACTOR_LZI),
            ("missing_actor_query.lzx", ROUTE_GUARD_MISSING_ACTOR_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert_eq!(route_guard.len(), 1, "got {route_guard:?}");
        assert_eq!(route_guard[0].code, "ROUTE-GUARD-004");
        assert_eq!(route_guard[0].severity, DoctorSeverity::Warning);
    }

    #[test]
    fn route_guard_005_fires_as_info_for_runtime_audience_disagreement_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_AUDIENCE_LZX);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert_eq!(route_guard.len(), 1, "got {route_guard:?}");
        assert_eq!(route_guard[0].code, "ROUTE-GUARD-005");
        assert_eq!(route_guard[0].severity, DoctorSeverity::Info);
    }

    #[test]
    fn lifecycle_gate_happy_fixture_fires_no_lifecycle_gate_diagnostics() {
        let package = package_from_sources(vec![
            ("happy.lzi", LIFECYCLE_GATE_HAPPY_LZI),
            ("happy.lzx", LIFECYCLE_GATE_HAPPY_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
        assert!(
            lifecycle_gate.is_empty(),
            "happy lifecycle gate fixtures must emit zero LIFECYCLE-GATE-* diagnostics; got: {:?}",
            lifecycle_gate
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn lifecycle_gate_fixtures_emit_exactly_the_documented_code() {
        for (source, expected) in [
            (LIFECYCLE_GATE_UNKNOWN_RESOURCE_LZX, "LIFECYCLE-GATE-001"),
            (LIFECYCLE_GATE_UNKNOWN_STATE_LZX, "LIFECYCLE-GATE-002"),
            (
                LIFECYCLE_GATE_MISSING_STATE_COVERAGE_LZX,
                "LIFECYCLE-GATE-003",
            ),
            (LIFECYCLE_GATE_EXTRA_STATE_ARM_LZX, "LIFECYCLE-GATE-004"),
            (LIFECYCLE_GATE_WILDCARD_OVERUSE_LZX, "LIFECYCLE-GATE-005"),
            (LIFECYCLE_GATE_REDIRECT_CYCLE_LZX, "LIFECYCLE-GATE-006"),
            (
                LIFECYCLE_GATE_RESUME_RESOURCE_MISMATCH_LZX,
                "LIFECYCLE-GATE-007",
            ),
            (LIFECYCLE_GATE_WRONG_QUERY_KIND_LZX, "LIFECYCLE-GATE-008"),
            (LIFECYCLE_GATE_WITHOUT_ACTOR_GATE_LZX, "LIFECYCLE-GATE-009"),
        ] {
            let package = lifecycle_gate_fixture(source);
            let diagnostics = package.diagnostics();
            let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
            assert_eq!(
                lifecycle_gate
                    .iter()
                    .map(|d| d.code.as_str())
                    .collect::<Vec<_>>(),
                vec![expected],
                "expected exactly {expected}; got {:?}",
                lifecycle_gate
                    .iter()
                    .map(|d| (&d.code, &d.message))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn lifecycle_gate_cross_feature_resume_resolves_through_uses() {
        let package = lifecycle_gate_fixture(LIFECYCLE_GATE_CROSS_FEATURE_LZX);
        let diagnostics = package.diagnostics();
        let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
        assert!(
            lifecycle_gate.is_empty(),
            "qualified @resume account.account_onboarding must resolve through host.uses account; got {:?}",
            lifecycle_gate
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn auth_refresh_happy_fixture_has_zero_diagnostics() {
        let package = package_from_sources(vec![("auth_refresh.lzi", AUTH_REFRESH_HAPPY)]);
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            diagnostics.is_empty(),
            "happy auth-refresh fixture must emit zero diagnostics; got {:?}",
            diagnostics
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn auth_refresh_fixtures_trigger_exact_codes() {
        for (source, code) in [
            (AUTH_REFRESH_001, "AUTH-REFRESH-001"),
            (AUTH_REFRESH_002, "AUTH-REFRESH-002"),
            (AUTH_REFRESH_003, "AUTH-REFRESH-003"),
            (AUTH_REFRESH_004, "AUTH-REFRESH-004"),
            (AUTH_REFRESH_005, "AUTH-REFRESH-005"),
            (AUTH_REFRESH_006, "AUTH-REFRESH-006"),
            (AUTH_REFRESH_007, "AUTH-REFRESH-007"),
            (AUTH_REFRESH_008, "AUTH-REFRESH-008"),
            (AUTH_REFRESH_009, "AUTH-REFRESH-009"),
        ] {
            assert_auth_refresh_fixture(source, code);
        }
    }

    #[test]
    fn auth_refresh_003_fires_for_incomplete_column_set() {
        let diagnostics = assert_auth_refresh_fixture(AUTH_REFRESH_003, "AUTH-REFRESH-003");
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "AUTH-REFRESH-003")
            .expect("AUTH-REFRESH-003 present");
        assert!(
            diag.message.contains("parent_session_id"),
            "missing-column message should name the incomplete column set: {}",
            diag.message
        );
    }

    #[test]
    fn auth_refresh_007_message_surfaces_resolved_defaults() {
        let diagnostics = assert_auth_refresh_fixture(AUTH_REFRESH_007, "AUTH-REFRESH-007");
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "AUTH-REFRESH-007")
            .expect("AUTH-REFRESH-007 present");
        assert!(diag.message.contains("refresh_ttl 14d"), "{}", diag.message);
        assert!(
            diag.message.contains("rotation_grace 1m"),
            "{}",
            diag.message
        );
        assert!(
            diag.message
                .contains("theft_detection_action revoke_session_family"),
            "{}",
            diag.message
        );
    }

    #[test]
    fn auth_refresh_info_diagnostics_are_non_blocking() {
        for (source, code) in [
            (AUTH_REFRESH_006, "AUTH-REFRESH-006"),
            (AUTH_REFRESH_007, "AUTH-REFRESH-007"),
            (AUTH_REFRESH_008, "AUTH-REFRESH-008"),
            (AUTH_REFRESH_009, "AUTH-REFRESH-009"),
        ] {
            let diagnostics = assert_auth_refresh_fixture(source, code);
            let diag = diagnostics
                .iter()
                .find(|d| d.code == code)
                .expect("diagnostic present");
            assert_eq!(diag.severity, DoctorSeverity::Info, "{code}");
            assert!(
                diagnostics
                    .iter()
                    .all(|d| d.severity != DoctorSeverity::Error),
                "{code} fixture should not contain error-severity diagnostics"
            );
        }
    }
}
