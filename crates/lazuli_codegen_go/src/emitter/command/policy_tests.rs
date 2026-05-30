
    // RB.S6.C — `policy_expr` rendering. These tests exercise the
    // predicate-atom and combinator emission through `emit_command_file`
    // because the rendered kv rows surface in the `Command[I,O]` struct
    // literal at the orchestrator level. Lifted out of `file_emit.rs`
    // (wave R8-2b) so the policy concern owns its own tests.
    use super::super::test_support::{
        base_command, base_feature, emit_with_customer_fallback as emit, local_qname, typed_slot,
    };
    use lazuli_ir::{
        BuiltinType, CommandEffect, CommandInput, CreateEffect, PolicyExpr, Record, Tenancy,
    };

    // The Record import is dragged in for typed-record output binding
    // ergonomics in later cells; keep a smoke-fn so the `Record` import
    // doesn't bit-rot when its emission branch lands.
    #[allow(dead_code)]
    fn _record_compiles(_: Record) {}
    #[allow(dead_code)]
    fn _tenancy_compiles(_: Tenancy) {}

    #[test]
    fn policy_expr_authenticated_renders_predicate_atom() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("create");
        cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy_expr = Some(PolicyExpr::Authenticated);
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("Name: \"authenticated\""),
            "expected `Name: \"authenticated\"` literal in:\n{out}"
        );
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"authenticated\"}"),
            "expected predicate atom in:\n{out}"
        );
    }

    #[test]
    fn policy_expr_has_permission_renders_rbac_permission_atom() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("start");
        cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy_expr = Some(PolicyExpr::HasPermission("queries:start".to_owned()));
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("{Namespace: \"rbac.permission\", Name: \"queries:start\"}"),
            "expected rbac.permission atom in:\n{out}"
        );
        assert!(
            out.contains("Name: \"has_permission queries:start\""),
            "expected display name in:\n{out}"
        );
    }

    #[test]
    fn policy_expr_and_combinator_renders_paren_and_predicate_atoms() {
        let mut feature = base_feature("customer");
        let mut cmd = base_command("start");
        cmd.input = CommandInput::Typed(vec![typed_slot("name", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy_expr = Some(PolicyExpr::And(vec![
            PolicyExpr::Authenticated,
            PolicyExpr::HasRole("manager".to_owned()),
        ]));
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"authenticated\"}"),
            "missing authenticated atom in:\n{out}"
        );
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"and\"}"),
            "missing and atom in:\n{out}"
        );
        assert!(
            out.contains("{Namespace: \"rbac.role\", Name: \"manager\"}"),
            "missing rbac.role atom in:\n{out}"
        );
        assert!(
            out.contains("Name: \"authenticated and has_role manager\""),
            "missing combined display name in:\n{out}"
        );
    }

    // GAP-09 — input-value-predicate policy atoms emit a `When` guard on
    // the `lazuli.PolicyAtom`, gated through the feature-local
    // `@policy.<name>` resolution path (`format_local_policy`).
    #[test]
    fn conditional_policy_atom_emits_when_guard() {
        use lazuli_ir::{
            CompareOp, ConditionalPolicyAtom, EvalPredicate, Expr, Path, Policies, PolicyCategory,
            PolicyRef, Predicate,
        };

        let mut feature = base_feature("catalog");
        feature.policies = Policies {
            categories: vec![PolicyCategory {
                name: "create".into(),
                atoms: vec![],
                conditional_atoms: vec![
                    ConditionalPolicyAtom {
                        atom: "@role.admin".into(),
                        when: EvalPredicate::Closed(Predicate::Comparison {
                            left: Expr::Path(Path::from_segments(["input", "scope"])),
                            op: CompareOp::Eq,
                            right: Expr::String("production".into()),
                        }),
                    },
                    ConditionalPolicyAtom {
                        atom: "@role.manager".into(),
                        when: EvalPredicate::Closed(Predicate::Comparison {
                            left: Expr::Path(Path::from_segments(["input", "scope"])),
                            op: CompareOp::Eq,
                            right: Expr::String("media".into()),
                        }),
                    },
                ],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };

        let mut cmd = base_command("create");
        cmd.input = CommandInput::Typed(vec![typed_slot("scope", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy = PolicyRef::Local("create".into());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains(
                "{Namespace: \"role\", Name: \"admin\", When: &lazuli.PolicyWhen{Path: \"input.scope\", Op: \"=\", Value: \"production\"}}"
            ),
            "expected admin atom guarded on input.scope == production in:\n{out}"
        );
        assert!(
            out.contains(
                "{Namespace: \"role\", Name: \"manager\", When: &lazuli.PolicyWhen{Path: \"input.scope\", Op: \"=\", Value: \"media\"}}"
            ),
            "expected manager atom guarded on input.scope == media in:\n{out}"
        );
    }
