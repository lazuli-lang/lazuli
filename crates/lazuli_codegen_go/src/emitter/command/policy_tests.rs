
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

    // RBAC-OR-001 — a named policy category with 2+ role atoms
    // (`view: @role.ADMIN, @role.MANAGER`) MUST join them with the `or`
    // predicate, not `and`. The user model is single-role, so an AND of
    // two role atoms can never be satisfied by any real caller (every such
    // screen would 403). A comma-separated category atom list has OR
    // semantics (docs/audience-policy.md). AND-composition is expressed via
    // structured `policy <expr>` / `PolicyExpr::And`, never here.
    #[test]
    fn multi_atom_named_policy_joins_with_or_not_and() {
        use lazuli_ir::{Policies, PolicyCategory, PolicyRef};

        let mut feature = base_feature("customer_management");
        feature.policies = Policies {
            categories: vec![PolicyCategory {
                name: "view".into(),
                atoms: vec!["@role.ADMIN".into(), "@role.MANAGER".into()],
                conditional_atoms: vec![],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };

        let mut cmd = base_command("list_customers");
        cmd.input = CommandInput::Typed(vec![typed_slot("q", BuiltinType::Text, false)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy = PolicyRef::Local("view".into());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // The two role atoms are present...
        assert!(
            out.contains("{Namespace: \"role\", Name: \"ADMIN\"}"),
            "expected ADMIN role atom in:\n{out}"
        );
        assert!(
            out.contains("{Namespace: \"role\", Name: \"MANAGER\"}"),
            "expected MANAGER role atom in:\n{out}"
        );
        // ...joined by `or`, wrapped in `( ... )`.
        assert!(
            out.contains(
                "[]lazuli.PolicyAtom{{Namespace: \"predicate\", Name: \"(\"}, {Namespace: \"role\", Name: \"ADMIN\"}, {Namespace: \"predicate\", Name: \"or\"}, {Namespace: \"role\", Name: \"MANAGER\"}, {Namespace: \"predicate\", Name: \")\"}}"
            ),
            "expected `( ADMIN or MANAGER )` OR-joined atom list in:\n{out}"
        );
        // ...and NEVER joined by `and` (the bug: single-role user can't hold both).
        assert!(
            !out.contains("{Namespace: \"role\", Name: \"ADMIN\"}, {Namespace: \"predicate\", Name: \"and\"}"),
            "multi-role named policy must NOT join role atoms with `and`:\n{out}"
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

    // SECURITY (POLICY-REF-UNRESOLVED) — a cross-feature `PolicyRef::External`
    // policy reference cannot be resolved to its atom list in the per-feature
    // codegen pass (the emitter only has THIS feature's `policies` block, not
    // the referenced feature's). The pre-fix code emitted a Name-only
    // `lazuli.Policy{Name: "..."}` with NO `Atoms` — a command guarded by an
    // external policy shipped EFFECTIVELY UNGUARDED (policy bypass / fail-open
    // for any call site that doesn't treat empty atoms as deny). The fix fails
    // CLOSED: emit an explicit `{Namespace: "predicate", Name: "deny"}` atom so
    // the runtime evaluator denies the call (403) rather than allow it.
    #[test]
    fn external_policy_ref_fails_closed_with_deny_atom() {
        use lazuli_ir::PolicyRef;

        let mut feature = base_feature("billing");
        let mut cmd = base_command("charge");
        cmd.input = CommandInput::Typed(vec![typed_slot("amount", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        // Reference a policy declared in ANOTHER feature.
        cmd.policy = PolicyRef::External {
            feature: "accounts".into(),
            name: "restricted".into(),
        };
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // Fail CLOSED: an explicit deny atom is present.
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"deny\"}"),
            "external/unresolvable policy must emit a deny atom (fail closed):\n{out}"
        );
        // It must NEVER emit a Name-only empty-atoms policy for a declared
        // (non-public) policy reference — that is the bypass.
        assert!(
            !out.contains("lazuli.Policy{Name: \"accounts.policy.restricted\"},"),
            "regression: external policy emitted Name-only (no Atoms) = silent allow/bypass:\n{out}"
        );
    }

    // SECURITY (POLICY-REF-UNRESOLVED) — a `@policy.<name>` reference whose
    // category does NOT exist in the feature's `policies` block also cannot be
    // resolved. Pre-fix it degraded to a Name-only empty-atoms policy (same
    // bypass shape). The fix fails closed with a deny atom.
    #[test]
    fn unresolvable_named_policy_ref_fails_closed_with_deny_atom() {
        use lazuli_ir::PolicyRef;

        let mut feature = base_feature("catalog");
        // Note: NO `policies` block declares `nonexistent`.
        let mut cmd = base_command("purge");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy = PolicyRef::Atom("policy.nonexistent".into());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"deny\"}"),
            "unresolvable named policy must emit a deny atom (fail closed):\n{out}"
        );
        assert!(
            !out.contains("lazuli.Policy{Name: \"@policy.nonexistent\"},"),
            "regression: unresolvable named policy emitted Name-only (no Atoms) = bypass:\n{out}"
        );
    }

    // No-regression — built-in `@policy.authenticated` / `@policy.public`
    // resolve WITHOUT a declared category (the CRUD synth default + marketing
    // reads). They must emit their `@scope.*` atoms, NOT a deny. Without this,
    // the fail-closed change would 403 every CRUD-synth command.
    #[test]
    fn builtin_authenticated_and_public_policies_resolve_not_deny() {
        use lazuli_ir::PolicyRef;

        for (name, scope) in [("authenticated", "authenticated"), ("public", "public")] {
            let mut feature = base_feature("catalog");
            // No `policies` block declares the built-in.
            let mut cmd = base_command("act");
            cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Text, true)]);
            cmd.effect = CommandEffect::Creates(CreateEffect {
                resource: local_qname("Customer"),
                from_input: true,
                assignments: vec![],
            });
            cmd.policy = PolicyRef::Local(name.to_owned());
            feature.commands.push(cmd);

            let out = emit(&feature).expect("emits");
            assert!(
                out.contains(&format!("{{Namespace: \"scope\", Name: \"{scope}\"}}")),
                "built-in @policy.{name} must resolve to @scope.{scope}:\n{out}"
            );
            assert!(
                !out.contains("{Namespace: \"predicate\", Name: \"deny\"}"),
                "built-in @policy.{name} must NOT be turned into a deny:\n{out}"
            );
        }
    }

    // REGRESSION (POLICY-REF-UNRESOLVED-001 false-positive) — a command-level
    // GAP-09 predicate-gated policy of the form
    // `@policy.admin when input.scope == "Production", @policy.finance when
    // input.scope == "MediaPlacement"` lands as ONE opaque `PolicyRef::Atom`
    // string. Post-6068b856 the deny-fallback caught the (whole-string) miss
    // and DENIED the command. The fix parses the GAP-09 atoms, resolves each
    // `@policy.<name>` to its category, and emits the resolved role atoms gated
    // by their `When` predicate — NOT a deny.
    #[test]
    fn conditional_comma_policy_resolves_each_ref_with_when_guard_not_deny() {
        use lazuli_ir::{Policies, PolicyCategory, PolicyRef};

        let mut feature = base_feature("billing_config");
        feature.policies = Policies {
            categories: vec![
                PolicyCategory {
                    name: "admin".into(),
                    atoms: vec!["@role.ADMIN".into()],
                    conditional_atoms: vec![],
                    previous_names: vec![],
                    when_denied: None,
                    when_denied_route: None,
                },
                PolicyCategory {
                    name: "finance".into(),
                    atoms: vec!["@role.ADMIN".into(), "@role.FINANCIAL".into()],
                    conditional_atoms: vec![],
                    previous_names: vec![],
                    when_denied: None,
                    when_denied_route: None,
                },
            ],
            fields: Vec::new(),
            span_ref: None,
        };

        let mut cmd = base_command("create_billing_type");
        cmd.input = CommandInput::Typed(vec![typed_slot("scope", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        // This is exactly how the analyzer lowers the pauta source line: the
        // whole conditional comma string in a single `PolicyRef::Atom` (the
        // leading `@` of the FIRST ref is stripped by `lower_policy_atom`).
        cmd.policy = PolicyRef::Atom(
            "policy.admin when input.scope == \"Production\", @policy.finance when input.scope == \"MediaPlacement\"".into(),
        );
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        // admin -> @role.ADMIN gated on Production.
        assert!(
            out.contains(
                "{Namespace: \"role\", Name: \"ADMIN\", When: &lazuli.PolicyWhen{Path: \"input.scope\", Op: \"=\", Value: \"Production\"}}"
            ),
            "expected admin->ADMIN atom gated on input.scope == Production in:\n{out}"
        );
        // finance -> (@role.ADMIN or @role.FINANCIAL) gated on MediaPlacement.
        assert!(
            out.contains(
                "{Namespace: \"role\", Name: \"FINANCIAL\", When: &lazuli.PolicyWhen{Path: \"input.scope\", Op: \"=\", Value: \"MediaPlacement\"}}"
            ),
            "expected finance->FINANCIAL atom gated on input.scope == MediaPlacement in:\n{out}"
        );
        // CRITICAL: the regression — it must NOT be turned into a deny.
        assert!(
            !out.contains("{Namespace: \"predicate\", Name: \"deny\"}"),
            "REGRESSION: legitimate conditional policy was DENIED:\n{out}"
        );
        // The two references are OR'd at top level.
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"or\"}"),
            "expected the two refs OR'd in:\n{out}"
        );
    }

    // SECURITY (preserved) — a conditional comma policy where ONE referenced
    // category genuinely does not exist must STILL fail CLOSED (deny). Only a
    // list of fully-RESOLVABLE refs is exempt from the deny.
    #[test]
    fn conditional_comma_policy_with_unresolvable_ref_still_denies() {
        use lazuli_ir::{Policies, PolicyCategory, PolicyRef};

        let mut feature = base_feature("billing_config");
        feature.policies = Policies {
            categories: vec![PolicyCategory {
                name: "admin".into(),
                atoms: vec!["@role.ADMIN".into()],
                conditional_atoms: vec![],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };

        let mut cmd = base_command("create_billing_type");
        cmd.input = CommandInput::Typed(vec![typed_slot("scope", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        // `finance` is NOT declared — the whole conditional ref must fail closed.
        cmd.policy = PolicyRef::Atom(
            "policy.admin when input.scope == \"Production\", @policy.finance when input.scope == \"MediaPlacement\"".into(),
        );
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("{Namespace: \"predicate\", Name: \"deny\"}"),
            "SECURITY: a conditional policy with an unresolvable ref must deny:\n{out}"
        );
    }

    // No-regression — a RESOLVABLE same-feature named policy must still emit its
    // real atoms and must NOT be turned into a deny.
    #[test]
    fn resolvable_same_feature_policy_does_not_emit_deny() {
        use lazuli_ir::{Policies, PolicyCategory, PolicyRef};

        let mut feature = base_feature("customer_management");
        feature.policies = Policies {
            categories: vec![PolicyCategory {
                name: "manage".into(),
                atoms: vec!["@role.ADMIN".into()],
                conditional_atoms: vec![],
                previous_names: vec![],
                when_denied: None,
                when_denied_route: None,
            }],
            fields: Vec::new(),
            span_ref: None,
        };

        let mut cmd = base_command("archive");
        cmd.input = CommandInput::Typed(vec![typed_slot("id", BuiltinType::Text, true)]);
        cmd.effect = CommandEffect::Creates(CreateEffect {
            resource: local_qname("Customer"),
            from_input: true,
            assignments: vec![],
        });
        cmd.policy = PolicyRef::Local("manage".into());
        feature.commands.push(cmd);

        let out = emit(&feature).expect("emits");
        assert!(
            out.contains("{Namespace: \"role\", Name: \"ADMIN\"}"),
            "resolvable policy must emit its real atoms:\n{out}"
        );
        assert!(
            !out.contains("{Namespace: \"predicate\", Name: \"deny\"}"),
            "resolvable policy must NOT be turned into a deny:\n{out}"
        );
    }
