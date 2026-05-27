    use super::*;
    use lazuli_ir::{
        Command, CommandEffect, CommandKind, Defaults, EnumDecl, Event, EventKind, Field,
        FieldConstraints, Policies, PolicyRef, Resource, TypedSlot,
    };

    fn qn(feature: Option<&str>, name: &str) -> QualifiedName {
        QualifiedName {
            feature: feature.map(str::to_owned),
            name: name.to_owned(),
        }
    }

    fn enum_ref(feature: Option<&str>, name: &str) -> TypeRef {
        TypeRef::EnumRef(qn(feature, name))
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
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
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

    fn module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            features,
        }
    }

    fn app(mode: &str) -> AppManifest {
        serde_json::from_value(serde_json::json!({
            "name": "TestApp",
            "defaults": {},
            "architecture": { "mode": mode }
        }))
        .expect("minimal AppManifest fixture")
    }

    fn public_contract() -> Option<PublicContract> {
        Some(PublicContract {
            version: 1,
            span_ref: None,
        })
    }

    fn enum_decl(name: &str, public_contract: Option<PublicContract>) -> EnumDecl {
        EnumDecl {
            name: name.to_owned(),
            public_contract,
            variants: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn resource(
        name: &str,
        fields: Vec<Field>,
        public_contract: Option<PublicContract>,
    ) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
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
        }
    }

    fn command(name: &str, input: CommandInput) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input,
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
        }
    }

    fn event(name: &str, payload: Vec<lazuli_ir::EventField>) -> Event {
        Event {
            name: name.to_owned(),
            kind: EventKind::Domain,
            payload,
            payload_none: false,
            level: None,
            outbox: lazuli_ir::OutboxMode::None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn non_microservices_mode_does_not_fire() {
        let mut account = empty_feature("account");
        account.enums.push(enum_decl("Gender", None));
        let mut host = empty_feature("host");
        host.uses.push("account".to_owned());
        host.resources.push(resource(
            "User",
            vec![field("gender", enum_ref(Some("account"), "Gender"))],
            None,
        ));

        let findings = check(&module(vec![account, host]), Some(&app("modular_monolith")));

        assert!(findings.is_empty());
    }

    #[test]
    fn cross_feature_type_ref_without_contract_fires() {
        let mut account = empty_feature("account");
        account.enums.push(enum_decl("Gender", None));
        let mut host = empty_feature("host");
        host.resources.push(resource(
            "User",
            vec![field("gender", enum_ref(Some("account"), "Gender"))],
            None,
        ));

        let findings = check(&module(vec![account, host]), Some(&app("microservices")));

        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "CROSS-FEATURE-CONTRACT-MISSING-001");
        assert_eq!(findings[0].consumer_feature, "host");
        assert_eq!(findings[0].origin_feature, "account");
        assert_eq!(findings[0].symbol, "Gender");
        assert_eq!(
            findings[0].consumer_site,
            "field `gender` of resource `User`"
        );
        assert!(findings[0]
            .message()
            .contains("public contract Gender as v1"));
    }

    #[test]
    fn cross_feature_type_ref_with_contract_does_not_fire() {
        let mut account = empty_feature("account");
        account.enums.push(enum_decl("Gender", public_contract()));
        let mut host = empty_feature("host");
        host.resources.push(resource(
            "User",
            vec![field("gender", enum_ref(Some("account"), "Gender"))],
            None,
        ));

        let findings = check(&module(vec![account, host]), Some(&app("microservices")));

        assert!(findings.is_empty());
    }

    #[test]
    fn intra_feature_type_ref_does_not_fire() {
        let mut host = empty_feature("host");
        host.resources.push(resource("Listing", vec![], None));
        host.resources.push(resource(
            "Booking",
            vec![field("listing", user_defined(None, "Listing"))],
            None,
        ));

        let findings = check(&module(vec![host]), Some(&app("microservices")));

        assert!(findings.is_empty());
    }

    #[test]
    fn multiple_consumers_emit_one_finding_each() {
        let mut account = empty_feature("account");
        account.resources.push(resource("User", vec![], None));
        let mut host = empty_feature("host");
        host.uses.push("account".to_owned());
        host.commands.push(command(
            "book",
            CommandInput::Typed(vec![slot("user", user_defined(None, "User"))]),
        ));
        let mut customer_outreach = empty_feature("customer_outreach");
        customer_outreach.uses.push("account".to_owned());
        customer_outreach.events.push(event(
            "campaign_sent",
            vec![lazuli_ir::EventField {
                name: "user".to_owned(),
                type_ref: user_defined(None, "User"),
                optional: false,
            }],
        ));

        let findings = check(
            &module(vec![account, host, customer_outreach]),
            Some(&app("microservices")),
        );

        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|finding| {
            finding.consumer_feature == "host"
                && finding.consumer_site == "input `user` of command `book`"
        }));
        assert!(findings.iter().any(|finding| {
            finding.consumer_feature == "customer_outreach"
                && finding.consumer_site == "payload `user` of event `campaign_sent`"
        }));
    }

    #[test]
    fn sql_query_return_shape_without_contract_fires() {
        let mut account = empty_feature("account");
        account.resources.push(resource("User", vec![], None));
        let mut host = empty_feature("host");
        host.queries.push(Query::Sql(lazuli_ir::SqlQuery {
            name: "recent_users".to_owned(),
            sql_kind: lazuli_ir::SqlQueryKind::Sql,
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            returns: TypeRef::Many(Box::new(user_defined(Some("account"), "User"))),
            sql_path: "queries/recent_users.sql".to_owned(),
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
        }));

        let findings = check(&module(vec![account, host]), Some(&app("microservices")));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].consumer_site,
            "return type of query.sql `recent_users`"
        );
    }
