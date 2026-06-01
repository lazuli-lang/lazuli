    use super::*;
    use lazuli_ir::{
        AppArchitecture, Command, CommandEffect, CommandKind, Defaults, Field, FieldConstraints,
        Module, Policies, PolicyRef, PublicContract as PC, Resource, TypedSlot,
    };

    fn qn(feature: Option<&str>, name: &str) -> QualifiedName {
        QualifiedName {
            feature: feature.map(str::to_owned),
            name: name.to_owned(),
        }
    }

    fn user_defined(feature: Option<&str>, name: &str) -> TypeRef {
        TypeRef::UserDefined(qn(feature, name))
    }

    fn field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn slot(name: &str, type_ref: TypeRef) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref,
            required: true,
            constraints: FieldConstraints::default(),
        validate_skip: false,
        }
    }

    fn empty_feature(name: &str) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn resource_with_contract(name: &str, public_contract: Option<PC>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields: vec![field(
                "id",
                TypeRef::Builtin(lazuli_ir::BuiltinType::Id),
            )],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
        }
    }

    fn account_with_version(symbol: &str, version: u16) -> Feature {
        let mut feature = empty_feature("account");
        feature
            .resources
            .push(resource_with_contract(symbol, Some(PC { version, span_ref: None })));
        feature
    }

    fn consumer_pinned(symbol: &str, pin: Option<u16>) -> Feature {
        let mut feature = empty_feature("billing");
        feature.uses = vec!["account".to_owned()];
        feature.uses_versions = vec![pin];
        feature.uses_spans = vec![lazuli_ir::SpanRef { start: 0, end: 0 }];
        feature.commands.push(Command {
            name: "ChargeAccount".to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Typed(vec![slot(
                "account",
                user_defined(Some("account"), symbol),
            )]),
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            emits: vec![],
            rate_limit: None,
            audit: None,
            approval: None,
            invalidates: vec![],
            external_calls: vec![],
            timeout: None,
            retry: None,
            idempotency: None,
            write_window: None,
            deprecated: None,
            handler: None,
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        });
        feature
    }

    fn module_with(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: None,
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features,
        }
    }

    fn microservices_app() -> AppManifest {
        AppManifest {
            name: "TestApp".into(),
            title: None,
            version: None,
            lazuli_version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            error_pages: Vec::new(),
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: Some(AppArchitecture {
                mode: Some("microservices".into()),
                service_ready: None,
                enforce_service_boundaries: None,
            }),
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
            cookie: None,
            proxy: None,
            limits: None,
            headers: None,
            route_guard: None,
            actor_query: None,
            span_ref: None,
        }
    }

    #[test]
    fn pinned_at_drifted_version_fires() {
        let module = module_with(vec![
            account_with_version("Customer", 2),
            consumer_pinned("Customer", Some(1)),
        ]);
        let app = microservices_app();

        let findings = check(&module, Some(&app));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.consumer_feature, "billing");
        assert_eq!(f.origin_feature, "account");
        assert_eq!(f.symbol, "Customer");
        assert_eq!(f.consumer_version, 1);
        assert_eq!(f.origin_version, 2);
        assert!(f.message().contains("v1"));
        assert!(f.message().contains("v2"));
    }

    #[test]
    fn pinned_at_matching_version_does_not_fire() {
        let module = module_with(vec![
            account_with_version("Customer", 1),
            consumer_pinned("Customer", Some(1)),
        ]);
        let app = microservices_app();

        assert!(check(&module, Some(&app)).is_empty());
    }

    #[test]
    fn unpinned_consumer_does_not_fire() {
        // No pin → consumer floats with origin; drift is impossible.
        let module = module_with(vec![
            account_with_version("Customer", 2),
            consumer_pinned("Customer", None),
        ]);
        let app = microservices_app();

        assert!(check(&module, Some(&app)).is_empty());
    }

    #[test]
    fn origin_without_contract_does_not_fire() {
        // Origin lacks `public contract` → CONTRACT-MISSING-001 handles it,
        // not this rule. Drift detection only runs against contracted
        // origins.
        let mut account = empty_feature("account");
        account
            .resources
            .push(resource_with_contract("Customer", None));
        let module = module_with(vec![account, consumer_pinned("Customer", Some(1))]);
        let app = microservices_app();

        assert!(check(&module, Some(&app)).is_empty());
    }

    #[test]
    fn non_microservices_mode_does_not_fire() {
        let module = module_with(vec![
            account_with_version("Customer", 2),
            consumer_pinned("Customer", Some(1)),
        ]);
        let mut app = microservices_app();
        app.architecture.as_mut().unwrap().mode = Some("modular_monolith".into());

        assert!(check(&module, Some(&app)).is_empty());
        assert_eq!(Finding::CODE, "CROSS-FEATURE-CONTRACT-VERSION-DRIFT-001");
    }
