    use super::*;

    use lazuli_ir::{
        BuiltinType, Command, CommandEffect, CommandInput, CommandKind, Defaults, Field,
        FieldConstraints, Module, Policies, PolicyRef, Record, Resource, TypedSlot,
    };

    fn builtin_field(name: &str, ty: BuiltinType, required: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(ty),
            required,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
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
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        }
    }

    fn mk_record(name: &str, fields: Vec<Field>) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields,
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn mk_command_with_typed_input(name: &str, slots: Vec<TypedSlot>) -> Command {
        Command {
            name: name.to_owned(),
            public_contract: None,
            kind: CommandKind::Returns,
            route: vec![],
            input: CommandInput::Typed(slots),
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

    fn slot(name: &str, ty: BuiltinType) -> TypedSlot {
        TypedSlot {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(ty),
            required: false,
            constraints: FieldConstraints::default(),
        validate_skip: false,
        }
    }

    fn mk_feature(
        name: &str,
        resources: Vec<Resource>,
        records: Vec<Record>,
        commands: Vec<Command>,
    ) -> Feature {
        Feature {
            name: name.to_owned(),
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
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands,
            apis: vec![],
            records,
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

    fn mk_module(features: Vec<Feature>) -> Module {
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

    #[test]
    fn two_resources_sharing_a_cluster_fire() {
        let order = mk_resource(
            "Order",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
                builtin_field("country", BuiltinType::Text, true),
            ],
        );
        let return_label = mk_resource(
            "ReturnLabel",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
                builtin_field("country", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("shipping", vec![order, return_label], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/shipping/shipping.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shared_fields.len(), 5);
        assert_eq!(findings[0].left.name, "Order");
        assert_eq!(findings[0].right.name, "ReturnLabel");
        assert_eq!(Finding::CODE, "VOCAB-SHADOW-RECORD-001");
        let msg = findings[0].message();
        assert!(msg.contains("Order"));
        assert!(msg.contains("ReturnLabel"));
        assert!(msg.contains("doctor:allow"));
        assert!(msg.contains("— reason"));
    }

    #[test]
    fn three_way_intersection_emits_three_pairwise_findings() {
        let a = mk_resource(
            "A",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let b = mk_resource(
            "B",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let c = mk_resource(
            "C",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![a, b, c], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert_eq!(findings.len(), 3, "three-way pairwise = 3 findings");
    }

    #[test]
    fn cluster_too_small_does_not_fire() {
        let a = mk_resource(
            "A",
            vec![
                builtin_field("name", BuiltinType::Text, true),
                builtin_field("description", BuiltinType::Text, false),
                builtin_field("created_at", BuiltinType::DateTime, true),
            ],
        );
        let b = mk_resource(
            "B",
            vec![
                builtin_field("name", BuiltinType::Text, true),
                builtin_field("description", BuiltinType::Text, false),
                builtin_field("created_at", BuiltinType::DateTime, true),
            ],
        );
        let feature = mk_feature("x", vec![a, b], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        // After filtering created_at, only 2 fields remain — below N=4.
        assert!(findings.is_empty());
    }

    #[test]
    fn ratio_below_50_percent_does_not_fire() {
        // A has 10 fields, B has 4 fields. Intersection is 4. 4/10=40%,
        // 4/4=100%. Left ratio 40% < 50% — must not fire.
        let mut a_fields = vec![
            builtin_field("street", BuiltinType::Text, true),
            builtin_field("city", BuiltinType::Text, true),
            builtin_field("state", BuiltinType::Text, true),
            builtin_field("postal_code", BuiltinType::Text, true),
        ];
        for i in 0..6 {
            a_fields.push(builtin_field(
                &format!("extra_{i}"),
                BuiltinType::Text,
                false,
            ));
        }
        let a = mk_resource("WideResource", a_fields);
        let b = mk_resource(
            "Narrow",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![a, b], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn resource_vs_command_input_fires() {
        let order = mk_resource(
            "Order",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let update = mk_command_with_typed_input(
            "update_order_address",
            vec![
                slot("street", BuiltinType::Text),
                slot("city", BuiltinType::Text),
                slot("state", BuiltinType::Text),
                slot("postal_code", BuiltinType::Text),
            ],
        );
        let feature = mk_feature("orders", vec![order], vec![], vec![update]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/orders/orders.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(matches!(
            findings[0].right.kind,
            DeclarationKind::CommandInput
        ));
    }

    #[test]
    fn record_vs_resource_fires() {
        let record = mk_record(
            "Address",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let property = mk_resource(
            "Property",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("realty", vec![property], vec![record], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/realty/realty.lzi"));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn view_projection_record_is_excluded() {
        // PendingReviewEntry ends in Entry + has review_id: ID required —
        // matches view-projection filter. Even if its fields mirror a
        // resource, it must not produce a finding.
        let view = mk_record(
            "PendingReviewEntry",
            vec![
                builtin_field("review_id", BuiltinType::Id, true),
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let resource = mk_resource(
            "Order",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![resource], vec![view], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn empty_declarations_do_not_produce_division_by_zero() {
        let empty_resource = mk_resource("Empty", vec![]);
        let other = mk_resource(
            "Other",
            vec![
                builtin_field("a", BuiltinType::Text, true),
                builtin_field("b", BuiltinType::Text, true),
                builtin_field("c", BuiltinType::Text, true),
                builtin_field("d", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![empty_resource, other], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn config_override_lowers_ratio_threshold() {
        // Same setup as ratio_below_50_percent_does_not_fire, but lower
        // the threshold and confirm the rule then fires.
        let mut a_fields = vec![
            builtin_field("street", BuiltinType::Text, true),
            builtin_field("city", BuiltinType::Text, true),
            builtin_field("state", BuiltinType::Text, true),
            builtin_field("postal_code", BuiltinType::Text, true),
        ];
        for i in 0..6 {
            a_fields.push(builtin_field(
                &format!("extra_{i}"),
                BuiltinType::Text,
                false,
            ));
        }
        let a = mk_resource("WideResource", a_fields);
        let b = mk_resource(
            "Narrow",
            vec![
                builtin_field("street", BuiltinType::Text, true),
                builtin_field("city", BuiltinType::Text, true),
                builtin_field("state", BuiltinType::Text, true),
                builtin_field("postal_code", BuiltinType::Text, true),
            ],
        );
        let feature = mk_feature("x", vec![a, b], vec![], vec![]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check_with_config(
            &feature,
            &module,
            Path::new("features/x/x.lzi"),
            DEFAULT_MIN_CLUSTER_FIELDS,
            0.30,
        );
        assert_eq!(findings.len(), 1);
    }

    /// spec 0012 true-positive guard: SHADOW-RECORD is NOT a false
    /// positive. The create/update command-input overlap (hostpoint
    /// `create_host` / `update_host` share ~10 Host fields; pauta
    /// `customer_management`) is a REAL ~120-line duplication whose fix is
    /// the shared input `record` primitive owned by specs 0003/0015 — NOT
    /// a rule relaxation. Proves spec 0012 did NOT silence the rule to
    /// zero out the waivers; the waivers are retained with a backlog
    /// pointer (lazuli-ops docs/language-backlog.md).
    #[test]
    fn still_fires_on_create_update_overlap() {
        let resource = mk_resource(
            "Host",
            vec![
                builtin_field("display_name", BuiltinType::Text, true),
                builtin_field("bio", BuiltinType::Text, false),
                builtin_field("phone", BuiltinType::Text, false),
                builtin_field("address", BuiltinType::Text, false),
                builtin_field("city", BuiltinType::Text, false),
            ],
        );
        let create = mk_command_with_typed_input(
            "create_host",
            vec![
                slot("display_name", BuiltinType::Text),
                slot("bio", BuiltinType::Text),
                slot("phone", BuiltinType::Text),
                slot("address", BuiltinType::Text),
                slot("city", BuiltinType::Text),
            ],
        );
        let update = mk_command_with_typed_input(
            "update_host",
            vec![
                slot("display_name", BuiltinType::Text),
                slot("bio", BuiltinType::Text),
                slot("phone", BuiltinType::Text),
                slot("address", BuiltinType::Text),
                slot("city", BuiltinType::Text),
            ],
        );
        let feature = mk_feature("host", vec![resource], vec![], vec![create, update]);
        let module = mk_module(vec![feature.clone()]);
        let findings = check(&feature, &module, Path::new("features/host/host.lzi"));
        assert!(
            !findings.is_empty(),
            "create/update input + resource overlap is a TRUE positive and must keep \
             firing (shared input record owned by specs 0003/0015)"
        );
    }
