    use super::*;

    use lazuli_ir::{
        Defaults, FieldConstraints, Module, Policies,
    };

    fn field(name: &str, type_ref: TypeRef, default: Option<DefaultValue>) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default,
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

    fn builtin(name: &str, ty: BuiltinType) -> Field {
        field(name, TypeRef::Builtin(ty), None)
    }

    fn user_defined(name: &str, target: &str) -> Field {
        field(
            name,
            TypeRef::UserDefined(QualifiedName {
                feature: None,
                name: target.to_owned(),
            }),
            None,
        )
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

    fn mk_record(name: &str, fields: Vec<Field>) -> Record {
        Record {
            name: name.to_owned(),
            public_contract: None,
            fields,
            discriminator_field: None,
            span_ref: None,
        }
    }

    fn mk_feature(
        name: &str,
        uses: Vec<String>,
        tenancy: Option<Tenancy>,
        resources: Vec<Resource>,
    ) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults {
                tenancy,
                timestamps: false,
                policy: None,
                rate_limit: None,
                audit: None,
            },
            uses,
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

    fn mk_module(features: Vec<Feature>) -> Module {
        Module {
            workspace: None,
            contracts: vec![],
            app: None,
            registry: None,
            profiles: vec![],
            design: None,
            rbac: None,
            doctor_allows: Vec::new(),
            features,
        }
    }

    #[test]
    fn timestamps_are_universal() {
        let feature = mk_feature("x", vec![], None, vec![]);
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &builtin("created_at", BuiltinType::DateTime),
            "Foo",
            &feature,
            &module,
        ));
        assert!(is_universal_column(
            &builtin("updated_at", BuiltinType::DateTime),
            "Foo",
            &feature,
            &module,
        ));
        assert!(is_universal_column(
            &builtin("deleted_at", BuiltinType::DateTime),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn id_is_universal_only_with_builtin_id_type() {
        let feature = mk_feature("x", vec![], None, vec![]);
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &builtin("id", BuiltinType::Id),
            "Foo",
            &feature,
            &module,
        ));
        // `id: Text` is unusual but not universal — would be a real field.
        assert!(!is_universal_column(
            &builtin("id", BuiltinType::Text),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn fk_self_reference_is_universal() {
        let foo = mk_resource("Foo", vec![]);
        let feature = mk_feature("x", vec![], None, vec![foo]);
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &builtin("foo_id", BuiltinType::Id),
            "Foo",
            &feature,
            &module,
        ));
        // `bar_id: Id` is NOT a self-reference when declaration is `Foo`.
        assert!(!is_universal_column(
            &builtin("bar_id", BuiltinType::Id),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn fk_to_org_tenancy_is_universal() {
        let org = mk_resource("Org", vec![]);
        let feature = mk_feature(
            "x",
            vec![],
            Some(Tenancy::Org),
            vec![org],
        );
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &user_defined("org", "Org"),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn fk_to_peer_resource_in_same_feature_is_universal() {
        let host = mk_resource("Host", vec![]);
        let feature = mk_feature("x", vec![], None, vec![host]);
        let module = mk_module(vec![feature.clone()]);
        assert!(is_universal_column(
            &user_defined("host", "Host"),
            "Property",
            &feature,
            &module,
        ));
    }

    #[test]
    fn fk_to_peer_resource_in_another_feature_via_uses_is_universal() {
        // catalog uses host; catalog.Property has `host: Host` which is FK
        // to host.Host. Without the cross-feature lookup, the rule would
        // mistakenly include `host` in cluster matching.
        let host_res = mk_resource("Host", vec![]);
        let host_feature = mk_feature("host", vec![], None, vec![host_res]);
        let catalog_feature = mk_feature(
            "catalog",
            vec!["host".to_owned()],
            None,
            vec![],
        );
        let module = mk_module(vec![host_feature, catalog_feature.clone()]);
        assert!(is_universal_column(
            &user_defined("host", "Host"),
            "Property",
            &catalog_feature,
            &module,
        ));
    }

    #[test]
    fn unresolved_user_defined_type_is_not_filtered() {
        // A `field: SomeType` where `SomeType` does not resolve to any
        // resource (perhaps a record or enum) — NOT a peer FK; should
        // remain in cluster matching.
        let feature = mk_feature("x", vec![], None, vec![]);
        let module = mk_module(vec![feature.clone()]);
        assert!(!is_universal_column(
            &user_defined("category", "Category"),
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn aggregation_snapshot_count_is_universal() {
        let feature = mk_feature("x", vec![], None, vec![]);
        let module = mk_module(vec![feature.clone()]);
        let mut count_field = builtin("unread_count", BuiltinType::Integer);
        count_field.default = Some(DefaultValue::Integer(0));
        assert!(is_universal_column(
            &count_field,
            "Foo",
            &feature,
            &module,
        ));
        // Integer field that does NOT end in _count is real data.
        assert!(!is_universal_column(
            &builtin("rating", BuiltinType::Integer),
            "Foo",
            &feature,
            &module,
        ));
        // _count without 0 default is unusual — keep it (likely real).
        let mut count_no_default = builtin("notify_count", BuiltinType::Integer);
        count_no_default.default = Some(DefaultValue::Integer(5));
        assert!(!is_universal_column(
            &count_no_default,
            "Foo",
            &feature,
            &module,
        ));
    }

    #[test]
    fn view_projection_record_with_id_lookup_is_filtered() {
        let view = mk_record(
            "PendingReviewEntry",
            vec![
                builtin("review_id", BuiltinType::Id),
                builtin("rating", BuiltinType::Integer),
            ],
        );
        assert!(is_view_projection_record(&view));
    }

    #[test]
    fn record_without_projection_suffix_is_not_filtered() {
        let address = mk_record(
            "Address",
            vec![
                builtin("street", BuiltinType::Text),
                builtin("city", BuiltinType::Text),
            ],
        );
        assert!(!is_view_projection_record(&address));
    }

    #[test]
    fn record_with_projection_suffix_but_no_id_lookup_is_not_filtered() {
        // `OrderItem` ends in `Item` but carries no `<noun>_id` lookup column;
        // the suffix alone is not enough to call it a projection.
        let item = mk_record(
            "OrderItem",
            vec![
                builtin("sku", BuiltinType::Text),
                builtin("quantity", BuiltinType::Integer),
            ],
        );
        assert!(!is_view_projection_record(&item));
    }
