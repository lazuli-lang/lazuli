
    use super::*;
    use lazuli_ir::{
        Assignment, CommandInput, CommandKind, Defaults, EnumLiteral, EnumVariant,
        FieldConstraints, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, PolicyRef, UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn enum_ref(name: &str) -> TypeRef {
        TypeRef::EnumRef(qn(name))
    }

    fn mk_enum(name: &str, variants: &[&str]) -> EnumDecl {
        EnumDecl {
            name: name.into(),
            public_contract: None,
            variants: variants
                .iter()
                .map(|variant| EnumVariant {
                    name: (*variant).to_owned(),
                    storage_value: None,
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                })
                .collect(),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.into(),
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

    fn mk_resource(name: &str, status_field: &str, lifecycle: Option<Lifecycle>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![mk_field(status_field, enum_ref("PublicationStatus"))],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: vec![],
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
        }
    }

    fn mk_lifecycle() -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states: vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("publishing", LifecycleStateKind::Intermediate),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            transitions: vec![mk_transition("begin_publishing", "scheduled", "publishing")],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_state(name: &str, kind: LifecycleStateKind) -> LifecycleState {
        LifecycleState {
            name: name.into(),
            kind,
            span_ref: None,
        }
    }

    fn mk_transition(name: &str, from: &str, to: &str) -> LifecycleTransition {
        LifecycleTransition {
            name: name.into(),
            from: vec![from.into()],
            to: to.into(),
            policy: None,
            audit: None,
            timestamps: None,
            emits: vec![],
            requires: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_cmd(name: &str, resource: &str, status_field: &str, variant: &str) -> Command {
        Command {
            name: name.into(),
            public_contract: None,
            kind: CommandKind::Update,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Updates(UpdateEffect {
                resource: qn(resource),
                assignments: vec![Assignment {
                    field: status_field.into(),
                    value: Expr::Enum(EnumLiteral {
                        type_name: Some(qn("PublicationStatus")),
                        variant: variant.into(),
                    }),
                }],
            }),
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
            previous_names: vec![],
            span_ref: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn mk_feature(resource: Resource, commands: Vec<Command>, enum_field_name: &str) -> Feature {
        Feature {
            name: "publishing".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![mk_enum(
                enum_field_name,
                &["scheduled", "publishing", "published", "cancelled"],
            )],
            resources: vec![resource],
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands,
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

    fn three_transition_commands(status_field: &str) -> Vec<Command> {
        vec![
            mk_cmd("mark_publishing", "Publication", status_field, "publishing"),
            mk_cmd("mark_published", "Publication", status_field, "published"),
            mk_cmd("mark_cancelled", "Publication", status_field, "cancelled"),
        ]
    }

    #[test]
    fn positive_three_transition_commands_fire() {
        let resource = mk_resource("Publication", "status", None);
        let feature = mk_feature(
            resource,
            three_transition_commands("status"),
            "PublicationStatus",
        );

        let findings = check(&feature, Path::new("features/publication/publication.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].status_field, "status");
        assert_eq!(findings[0].enum_name, "PublicationStatus");
        assert_eq!(Finding::CODE, "VOCAB-LIFECYCLE-001");
    }

    #[test]
    fn negative_resource_already_has_lifecycle_does_not_fire() {
        let resource = mk_resource("Publication", "status", Some(mk_lifecycle()));
        let feature = mk_feature(
            resource,
            three_transition_commands("status"),
            "PublicationStatus",
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "resource with lifecycle must not trigger VOCAB-LIFECYCLE-001"
        );
    }

    #[test]
    fn negative_fewer_than_three_commands_does_not_fire() {
        let resource = mk_resource("Publication", "status", None);
        let feature = mk_feature(
            resource,
            vec![
                mk_cmd("mark_publishing", "Publication", "status", "publishing"),
                mk_cmd("mark_published", "Publication", "status", "published"),
            ],
            "PublicationStatus",
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "two transition commands are below the rule threshold"
        );
    }

    #[test]
    fn negative_non_status_field_name_does_not_fire() {
        let resource = mk_resource("Publication", "category", None);
        let feature = mk_feature(
            resource,
            three_transition_commands("category"),
            "PublicationStatus",
        );

        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "enum field names outside the closed status catalog must not fire"
        );
    }

    #[test]
    fn positive_finding_lists_command_names() {
        let resource = mk_resource("Publication", "status", None);
        let feature = mk_feature(
            resource,
            three_transition_commands("status"),
            "PublicationStatus",
        );

        let findings = check(&feature, Path::new("features/publication/publication.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].transition_commands,
            vec![
                "mark_publishing".to_owned(),
                "mark_published".to_owned(),
                "mark_cancelled".to_owned(),
            ]
        );
    }
