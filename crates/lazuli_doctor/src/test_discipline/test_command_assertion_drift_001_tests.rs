    use super::*;

    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, CompareOp, Defaults,
        DeleteEffect, EvalPredicate, Expr, Feature, Field, FieldConstraints, Invariant, Lifecycle,
        LifecycleState, LifecycleStateKind, Path as IrPath, Policies, PolicyRef, Predicate,
        QualifiedName, Resource, TestAssertion, TestBlock, TypeRef, UpdateEffect,
    };

    fn mk_cmd(name: &str, effect: CommandEffect, tests: Option<TestBlock>) -> Command {
        Command {
            name: name.into(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect,
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
            tests,
            triggers: vec![],
            synthesized_from_cap_file: None,
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: None,
            derived_from: None,
        }
    }

    fn mk_qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.into(),
        }
    }

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
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

    fn mk_resource(name: &str, lifecycle: Option<Lifecycle>, invariants: Vec<Invariant>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![mk_field("status"), mk_field("host_reply")],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle,
            invariants,
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            append_only: false,
        }
    }

    fn mk_feature(resources: Vec<Resource>, commands: Vec<Command>) -> Feature {
        Feature {
            name: "trust".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
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

    fn denies_when_target_eq(field: &str, value: &str) -> TestBlock {
        TestBlock {
            assertions: vec![TestAssertion::DeniesWhen {
                predicate: Predicate::Comparison {
                    left: Expr::Path(IrPath::from_segments(["target", field])),
                    op: CompareOp::Eq,
                    right: Expr::String(value.to_string()),
                },
            }],
            span_ref: None,
        }
    }

    #[test]
    fn fires_for_leave_host_reply_pattern() {
        // The bug shape: `denies when target.status = removed` declared
        // on the command, but resource has no invariant or lifecycle
        // gate on `status`, so the handler's WHERE clause silently
        // ignores it.
        let cmd = mk_cmd(
            "leave_host_reply",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
        let findings = check(&feature, Path::new("trust.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "status");
        assert_eq!(findings[0].command, "leave_host_reply");
        assert_eq!(findings[0].resource.as_deref(), Some("Review"));
        assert!(findings[0].message().contains("§7.1"));
    }

    #[test]
    fn quiet_when_lifecycle_discriminator_matches_field() {
        // If `status` IS the lifecycle field, the state machine is the
        // implicit WHERE clause — no drift.
        let lifecycle = Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "ReviewStatus".into(),
            states: vec![
                LifecycleState {
                    name: "active".into(),
                    kind: LifecycleStateKind::Initial,
                    span_ref: None,
                },
                LifecycleState {
                    name: "removed".into(),
                    kind: LifecycleStateKind::Terminal,
                    span_ref: None,
                },
            ],
            transitions: vec![],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        };
        let cmd = mk_cmd(
            "leave_host_reply",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(
            vec![mk_resource("Review", Some(lifecycle), vec![])],
            vec![cmd],
        );
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn quiet_when_invariant_mentions_field() {
        // Build an invariant whose serialized form mentions the
        // "status" field. The v0.1 rule string-matches the field name
        // through the JSON projection of `EvalPredicate` — cheap and
        // conservative pending a shared predicate visitor.
        let inv = Invariant {
            name: "host_reply_only_when_active".into(),
            when: EvalPredicate::Closed(Predicate::Comparison {
                left: Expr::Path(IrPath::from_segments(["target", "status"])),
                op: CompareOp::Eq,
                right: Expr::String("active".to_string()),
            }),
            message: "".into(),
            span_ref: None,
        };
        let cmd = mk_cmd(
            "leave_host_reply",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(
            vec![mk_resource("Review", None, vec![inv])],
            vec![cmd],
        );
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn quiet_when_command_has_triggers() {
        // A command bound to lifecycle transitions inherits state
        // filtering — no drift possible.
        let mut cmd = mk_cmd(
            "leave_host_reply",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        cmd.triggers.push("publish".to_string());
        let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn quiet_for_returns_commands() {
        // Read-only commands have no implicit WHERE clause to drift on.
        let cmd = mk_cmd(
            "summarize",
            CommandEffect::None,
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn quiet_for_delete_command_with_lifecycle_backing() {
        let lifecycle = Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "ReviewStatus".into(),
            states: vec![LifecycleState {
                name: "active".into(),
                kind: LifecycleStateKind::Initial,
                span_ref: None,
            }],
            transitions: vec![],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        };
        let cmd = mk_cmd(
            "drop",
            CommandEffect::Deletes(DeleteEffect {
                resource: mk_qn("Review"),
            }),
            Some(denies_when_target_eq("status", "removed")),
        );
        let feature = mk_feature(
            vec![mk_resource("Review", Some(lifecycle), vec![])],
            vec![cmd],
        );
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }

    #[test]
    fn ignores_non_target_paths() {
        // `input.X = Y` is not a WHERE clause — argument validation.
        // The rule stays silent.
        let tests = TestBlock {
            assertions: vec![TestAssertion::DeniesWhen {
                predicate: Predicate::Comparison {
                    left: Expr::Path(IrPath::from_segments(["input", "owner_id"])),
                    op: CompareOp::Eq,
                    right: Expr::Nil,
                },
            }],
            span_ref: None,
        };
        let cmd = mk_cmd(
            "update_self",
            CommandEffect::Updates(UpdateEffect {
                resource: mk_qn("Review"),
                assignments: vec![],
            }),
            Some(tests),
        );
        let feature = mk_feature(vec![mk_resource("Review", None, vec![])], vec![cmd]);
        assert!(check(&feature, Path::new("trust.lzi")).is_empty());
    }
