    use super::*;

    use lazuli_ir::{
        BuiltinType, Defaults, Field, FieldConstraints, Module, Policies, Resource, TypeRef,
    };

    fn field(name: &str) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
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
            soft_delete_actor: false,
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
            restrict_on_delete: Vec::new(),
            append_only: false,
        }
    }

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "x".into(),
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

    fn mk_module(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            doctor_allows: Vec::new(),
            features: vec![feature],
        }
    }

    fn order_with_shipping_cluster() -> Resource {
        let names = [
            "customer",
            "status",
            "total_cents",
            "notes",
            "shipping_street",
            "shipping_city",
            "shipping_state",
            "shipping_postal_code",
            "shipping_country",
            "billing_country",
            "name",
        ];
        mk_resource("Order", names.iter().map(|n| field(n)).collect())
    }

    #[test]
    fn leading_token_cluster_fires() {
        let feature = mk_feature(vec![order_with_shipping_cluster()]);
        let module = mk_module(feature.clone());
        let findings = check(&feature, &module, Path::new("features/orders/orders.lzi"));
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.token, "shipping");
        assert!(matches!(f.token_position, TokenPosition::Leading));
        assert_eq!(f.cluster_fields.len(), 5);
        let msg = f.message();
        assert!(msg.contains("Order"));
        assert!(msg.contains("leading token"));
        assert!(msg.contains("shipping"));
        assert!(msg.contains("— reason"));
    }

    #[test]
    fn trailing_token_cluster_fires_when_author_renamed_to_suffix() {
        // The "renamed to dodge prefix detection" case. Five fields share
        // the trailing token `shipping`.
        let resource = mk_resource(
            "Order",
            vec![
                field("customer"),
                field("status"),
                field("notes"),
                field("total_cents"),
                field("street_for_shipping"),
                field("city_for_shipping"),
                field("state_for_shipping"),
                field("postal_code_for_shipping"),
                field("country_for_shipping"),
                field("priority"),
                field("notes_extra"),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let module = mk_module(feature.clone());
        let findings = check(&feature, &module, Path::new("features/orders/orders.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].token, "shipping");
        assert!(matches!(
            findings[0].token_position,
            TokenPosition::Trailing
        ));
        assert_eq!(findings[0].cluster_fields.len(), 5);
    }

    #[test]
    fn small_resource_does_not_fire() {
        let resource = mk_resource(
            "Tiny",
            vec![
                field("shipping_street"),
                field("shipping_city"),
                field("shipping_state"),
                field("shipping_country"),
                field("notes"),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let module = mk_module(feature.clone());
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty(), "5 fields < K=10");
    }

    #[test]
    fn cluster_too_small_does_not_fire() {
        // 11 fields total, but the largest shared-prefix cluster is 3.
        let resource = mk_resource(
            "Order",
            vec![
                field("customer"),
                field("status"),
                field("notes"),
                field("total"),
                field("priority"),
                field("currency"),
                field("country"),
                field("description"),
                field("shipping_street"),
                field("shipping_city"),
                field("shipping_state"),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let module = mk_module(feature.clone());
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty(), "largest cluster is 3 < M=4");
    }

    #[test]
    fn excluded_tokens_do_not_cluster() {
        // 11 trailing `_at` fields with mixed leading tokens (no shared
        // leading cluster) would still hit the trailing `at` token if the
        // exclusion list didn't apply.
        let names = vec![
            "created_at",
            "updated_at",
            "deleted_at",
            "shipped_at",
            "delivered_at",
            "cancelled_at",
            "refunded_at",
            "archived_at",
            "approved_at",
            "rejected_at",
            "expired_at",
        ];
        let resource = mk_resource("Logs", names.into_iter().map(field).collect());
        let feature = mk_feature(vec![resource]);
        let module = mk_module(feature.clone());
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(
            findings.is_empty(),
            "trailing `at` token must not cluster (it's in the exclusion list); also created_at/updated_at/deleted_at are universal columns and filtered before the cluster pass — only 8 fields remain"
        );
    }

    #[test]
    fn single_letter_tokens_are_ineligible() {
        // Token `a` (length 1) is ineligible — would otherwise cluster on
        // `a_x`, `a_y`, `a_z`, `a_w` from naive code generators.
        let names = vec![
            "customer",
            "status",
            "notes",
            "total",
            "priority",
            "currency",
            "description",
            "a_x",
            "a_y",
            "a_z",
            "a_w",
        ];
        let resource = mk_resource("Order", names.into_iter().map(field).collect());
        let feature = mk_feature(vec![resource]);
        let module = mk_module(feature.clone());
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn largest_cluster_wins_when_two_present() {
        // Resource has both `shipping_*` (5 fields) and `billing_*` (4
        // fields). Rule surfaces only the largest.
        let resource = mk_resource(
            "Order",
            vec![
                field("customer"),
                field("status"),
                field("notes"),
                field("shipping_street"),
                field("shipping_city"),
                field("shipping_state"),
                field("shipping_postal_code"),
                field("shipping_country"),
                field("billing_street"),
                field("billing_city"),
                field("billing_state"),
                field("billing_country"),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let module = mk_module(feature.clone());
        let findings = check(&feature, &module, Path::new("features/x/x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].token, "shipping");
        assert_eq!(findings[0].cluster_fields.len(), 5);
    }

    #[test]
    fn config_override_changes_thresholds() {
        // Smaller resource (8 fields) with 4 shipping_* — would not fire at
        // default K=10; should fire at K=6.
        let resource = mk_resource(
            "Order",
            vec![
                field("customer"),
                field("status"),
                field("notes"),
                field("total"),
                field("shipping_street"),
                field("shipping_city"),
                field("shipping_state"),
                field("shipping_country"),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let module = mk_module(feature.clone());
        assert!(check(&feature, &module, Path::new("features/x/x.lzi")).is_empty());
        let findings = check_with_config(
            &feature,
            &module,
            Path::new("features/x/x.lzi"),
            6,
            DEFAULT_MIN_CLUSTER_FIELDS,
            DEFAULT_EXCLUDED_TOKENS,
        );
        assert_eq!(findings.len(), 1);
    }
