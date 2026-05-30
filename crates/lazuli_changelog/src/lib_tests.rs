
    use super::*;
    use lazuli_ir::*;

    fn cmd(name: &str, kind: CommandKind) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
            route: Vec::new(),
            input: CommandInput::Empty,
            target: None,
            lets: Vec::new(),
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: Vec::new(),
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: Vec::new(),
            external_calls: Vec::new(),
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            previous_names: Vec::new(),
            span_ref: None,
            derived_from: None,
        }
    }

    fn module_with(commands: Vec<Command>) -> Module {
        let feature = Feature {
            name: "customer".to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            errors: None,
            commands,
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        };
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    #[test]
    fn detects_added_command() {
        let old = module_with(vec![cmd("create", CommandKind::Create)]);
        let new = module_with(vec![
            cmd("create", CommandKind::Create),
            cmd("reassign", CommandKind::Update),
        ]);
        let report = diff(&old, &new);
        assert_eq!(report.added.len(), 1);
        assert_eq!(report.added[0].name, "reassign");
        assert!(report.removed.is_empty());
    }

    #[test]
    fn detects_removed_command() {
        let old = module_with(vec![
            cmd("create", CommandKind::Create),
            cmd("reassign", CommandKind::Update),
        ]);
        let new = module_with(vec![cmd("create", CommandKind::Create)]);
        let report = diff(&old, &new);
        assert_eq!(report.removed.len(), 1);
        assert_eq!(report.removed[0].name, "reassign");
    }

    #[test]
    fn detects_new_deprecation() {
        let old = module_with(vec![cmd("reassign", CommandKind::Update)]);
        let mut deprecated_cmd = cmd("reassign", CommandKind::Update);
        deprecated_cmd.deprecated = Some(Deprecation {
            since: Some("2026.04".to_owned()),
            replacement: Some(DeprecationReplacement::LocalCommand(
                "reassign_v2".to_owned(),
            )),
            sunset: Some("2026-12-31".to_owned()),
        });
        let new = module_with(vec![deprecated_cmd]);
        let report = diff(&old, &new);
        assert_eq!(report.deprecated.len(), 1);
        assert_eq!(report.deprecated[0].since.as_deref(), Some("2026.04"));
    }
