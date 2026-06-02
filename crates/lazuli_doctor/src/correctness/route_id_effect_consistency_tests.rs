    use super::*;
    use lazuli_ir::{
        Assignment, BuiltinType, Command, CommandEffect, CommandKind, Defaults, DeleteEffect, Expr,
        FieldConstraints, Path as IrPath, Policies, PolicyRef, QualifiedName, RouteSlot,
        RouteSlotKind, TypeRef, TypedSlot, UpdateEffect,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn builtin(b: BuiltinType) -> TypeRef {
        TypeRef::Builtin(b)
    }

    fn mk_route(name: &str, ty: BuiltinType, from: Option<&str>) -> RouteSlot {
        RouteSlot {
            name: name.to_owned(),
            type_ref: builtin(ty),
            from: from.map(|s| s.to_owned()),
            kind: RouteSlotKind::Plain,
        }
    }

    fn mk_typed_slot(name: &str, ty: BuiltinType) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref: builtin(ty),
            required: true,
            constraints: FieldConstraints::default(),
        validate_skip: false,
        }
    }

    fn mk_cmd(
        name: &str,
        route: Vec<RouteSlot>,
        input: CommandInput,
        effect: CommandEffect,
    ) -> Command {
        let kind = match &effect {
            CommandEffect::Updates(_) => CommandKind::Update,
            CommandEffect::Deletes(_) => CommandKind::Delete,
            CommandEffect::Creates(_) => CommandKind::Create,
            _ => CommandKind::Returns,
        };
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind,
            route,
            input,
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
            tests: None,
            triggers: vec![],
            synthesized_from_cap_file: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
            derived_from: None,
        }
    }

    fn mk_feature(command: Command) -> Feature {
        Feature {
            name: "customer".into(),
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
            commands: vec![command],
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

    fn updates_effect() -> CommandEffect {
        CommandEffect::Updates(UpdateEffect {
            resource: qn("Customer"),
            assignments: vec![Assignment {
                field: "tier".to_owned(),
                value: Expr::Path(IrPath::from_segments(["input", "tier"])),
            }],
            where_clause: Vec::new(),
        })
    }

    fn deletes_effect() -> CommandEffect {
        CommandEffect::Deletes(DeleteEffect {
            resource: qn("Customer"),
            where_clause: Vec::new(),
        })
    }

    #[test]
    fn silent_when_input_omits_route_slot_because_codegen_a1_emits_field() {
        // Post-cell-A1: codegen-go emits every `command.route` slot as
        // an input-struct field directly. Authors don't need to repeat
        // the route slot inside the typed input block. The diagnostic
        // stays silent — A1 owns the codegen guarantee; A3 only catches
        // SHADOWING (route slot + same-name input slot with different
        // type), tested below.
        let cmd = mk_cmd(
            "update_tier",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Typed(vec![mk_typed_slot("tier", BuiltinType::Text)]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        let findings = check(&feature, Path::new("features/customer/customer.lzi"));

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }

    #[test]
    fn silent_when_route_id_with_empty_input_post_a1() {
        // `delete_X route id: ID` lowers to `CommandInput::Empty` (no
        // body fields). Codegen A1 still emits a synthetic input struct
        // carrying the route slot, so the runtime UPDATE/DELETE Effect's
        // `FromInput("ID")` binding resolves. Diagnostic stays silent.
        let cmd = mk_cmd(
            "delete_customer",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Empty,
            deletes_effect(),
        );
        let feature = mk_feature(cmd);

        let findings = check(&feature, Path::new("f.lzi"));

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }

    #[test]
    fn silent_when_composite_route_input_partially_redeclares_post_a1() {
        // `route customer_id: ID + tag_id: ID` with input redeclaring
        // only `customer_id` (same type). A1 emits both struct fields.
        // Same-name same-type partial redeclare is a no-op shadow.
        let cmd = mk_cmd(
            "untag_customer",
            vec![
                mk_route("customer_id", BuiltinType::Id, None),
                mk_route("tag_id", BuiltinType::Id, None),
            ],
            CommandInput::Typed(vec![mk_typed_slot("customer_id", BuiltinType::Id)]),
            deletes_effect(),
        );
        let feature = mk_feature(cmd);

        let findings = check(&feature, Path::new("f.lzi"));

        assert!(findings.is_empty(), "expected no findings, got {findings:?}");
    }

    #[test]
    fn positive_shadow_route_id_with_different_typed_input_slot_fires() {
        // True shadow bug: `route id: ID` but the typed input ALSO has
        // an `id` slot with a different type (e.g. Text). Codegen would
        // emit two fields with the same name — illegal Go — OR silently
        // pick one and drop the other. Either way it's a bug worth
        // surfacing.
        let cmd = mk_cmd(
            "shadow_update",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Typed(vec![mk_typed_slot("id", BuiltinType::Text)]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        let findings = check(&feature, Path::new("f.lzi"));

        assert_eq!(findings.len(), 1, "expected shadow finding, got {findings:?}");
        assert_eq!(findings[0].param_name, "id");
        assert_eq!(Finding::CODE, "ROUTE-ID-UNUSED-IN-EFFECT-001");
    }

    #[test]
    fn negative_updates_with_route_id_consumed_by_typed_input_passes() {
        // Healthy shape: `route id: ID` + the typed input also lists
        // `id` (so the Go input struct has an `ID` field). No finding.
        let cmd = mk_cmd(
            "update_tier",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Typed(vec![
                mk_typed_slot("id", BuiltinType::Id),
                mk_typed_slot("tier", BuiltinType::Text),
            ]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_route_slot_with_ctx_default_passes() {
        // `route customer_id: ID from ctx.user.id` — the runtime
        // sources the value from context, not from the input struct,
        // so the input is allowed to omit it.
        let cmd = mk_cmd(
            "update_self",
            vec![mk_route(
                "customer_id",
                BuiltinType::Id,
                Some("ctx.user.id"),
            )],
            CommandInput::Typed(vec![mk_typed_slot("tier", BuiltinType::Text)]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_short_input_covering_route_slot_passes() {
        // `input id, tier` (short form) lists `id` — once expanded by
        // the analyzer it'll be a typed slot for `id`. No finding.
        let cmd = mk_cmd(
            "update_tier",
            vec![mk_route("id", BuiltinType::Id, None)],
            CommandInput::Short(vec!["id".to_owned(), "tier".to_owned()]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_creates_command_with_route_slot_does_not_fire() {
        // Creates effect doesn't use a WHERE binding — the route slot
        // shape on a `create` command is unusual but not the bug class
        // this diagnostic guards against.
        let cmd = mk_cmd(
            "create_customer",
            vec![mk_route("tenant_id", BuiltinType::Id, None)],
            CommandInput::Typed(vec![mk_typed_slot("name", BuiltinType::Text)]),
            CommandEffect::Creates(lazuli_ir::CreateEffect {
                resource: qn("Customer"),
                from_input: true,
                assignments: vec![],
            }),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_command_without_route_does_not_fire() {
        let cmd = mk_cmd(
            "update_tier",
            vec![],
            CommandInput::Typed(vec![mk_typed_slot("tier", BuiltinType::Text)]),
            updates_effect(),
        );
        let feature = mk_feature(cmd);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn pascal_case_handles_id_acronym() {
        assert_eq!(pascal_case("id"), "ID");
        assert_eq!(pascal_case("customer_id"), "CustomerID");
        assert_eq!(pascal_case("tag_id"), "TagID");
    }
