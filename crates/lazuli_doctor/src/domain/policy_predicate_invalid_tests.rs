
    use super::*;
    use lazuli_ir::{
        BuiltinType, CommandEffect, CommandInput, CommandKind, CompareOp, ConditionalPolicyAtom,
        Defaults, EvalPredicate, Expr, Feature, Path as IrPath, Policies, PolicyCategory, PolicyRef,
        Predicate, TypeRef, TypedSlot,
    };

    fn scope_pred(value: &str) -> EvalPredicate {
        EvalPredicate::Closed(Predicate::Comparison {
            left: Expr::Path(IrPath::from_segments(["input", "scope"])),
            op: CompareOp::Eq,
            right: Expr::String(value.into()),
        })
    }

    fn typed_slot(name: &str) -> TypedSlot {
        TypedSlot {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            constraints: Default::default(),
            validate_skip: false,
        }
    }

    fn mk_command(name: &str, policy: PolicyRef, inputs: &[&str]) -> Command {
        Command {
            name: name.into(),
            public_contract: None,
            kind: CommandKind::Create,
            route: vec![],
            input: CommandInput::Typed(inputs.iter().map(|n| typed_slot(n)).collect()),
            target: None,
            lets: vec![],
            effect: CommandEffect::None,
            policy,
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

    fn mk_feature(categories: Vec<PolicyCategory>, commands: Vec<Command>) -> Feature {
        Feature {
            name: "account".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
            events: vec![],
            rules: vec![],
            policies: Policies {
                categories,
                fields: vec![],
                span_ref: None,
            },
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

    fn cat(name: &str, conditional: Vec<ConditionalPolicyAtom>) -> PolicyCategory {
        PolicyCategory {
            name: name.into(),
            atoms: vec![],
            conditional_atoms: conditional,
            previous_names: vec![],
            when_denied: None,
            when_denied_route: None,
        }
    }

    #[test]
    fn negative_known_input_field_passes() {
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@policy.admin".into(),
                    when: scope_pred("production"),
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Local("create".into()),
                &["scope"],
            )],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn positive_unknown_input_field_fires() {
        // Predicate references `input.ghost`; command declares only `scope`.
        let ghost_pred = EvalPredicate::Closed(Predicate::Comparison {
            left: Expr::Path(IrPath::from_segments(["input", "ghost"])),
            op: CompareOp::Eq,
            right: Expr::String("x".into()),
        });
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@policy.admin".into(),
                    when: ghost_pred,
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Local("create".into()),
                &["scope"],
            )],
        );
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].kind,
            FindingKind::UnknownInputField("ghost".into())
        );
        assert_eq!(Finding::CODE, "POLICY-PREDICATE-001");
    }

    #[test]
    fn positive_unknown_atom_namespace_fires() {
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@bogus.admin".into(),
                    when: scope_pred("production"),
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Local("create".into()),
                &["scope"],
            )],
        );
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::UnknownAtom);
    }

    #[test]
    fn unparseable_predicate_surfaces_sentinel() {
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@policy.admin".into(),
                    when: EvalPredicate::Unparsed("scope and weird".into()),
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Local("create".into()),
                &["scope"],
            )],
        );
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        match &findings[0].kind {
            FindingKind::UnknownInputField(f) => assert!(f.contains("unparseable")),
            other => panic!("expected unparseable sentinel, got {other:?}"),
        }
    }

    #[test]
    fn unconditional_category_is_ignored() {
        // No conditional atoms → not in scope, even with no command.
        let feature = mk_feature(
            vec![PolicyCategory {
                name: "create".into(),
                atoms: vec!["@role.admin".into()],
                conditional_atoms: vec![],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            vec![],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn atom_resolves_via_policy_dot_atom_ref() {
        // Command references the category via PolicyRef::Atom("policy.create").
        let feature = mk_feature(
            vec![cat(
                "create",
                vec![ConditionalPolicyAtom {
                    atom: "@role.admin".into(),
                    when: scope_pred("production"),
                }],
            )],
            vec![mk_command(
                "create",
                PolicyRef::Atom("policy.create".into()),
                &["scope"],
            )],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
