
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, E2eeCapability, EncryptedCapability, HashAlgorithm,
        HashedCapability, Policies, Resource, TokenCapability, TokenStore,
    };

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_derived_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            derived_from: Some("score > 80".to_owned()),
            ..mk_field(name, type_ref)
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.into(),
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
            name: "test".into(),
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
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn text() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    fn hashed() -> TypeRef {
        TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
            algorithm: HashAlgorithm::Argon2id,
        }))
    }

    fn encrypted() -> TypeRef {
        TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
            key: "@key.tenant".to_owned(),
        }))
    }

    fn e2ee() -> TypeRef {
        TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
            key: "@key.actor".to_owned(),
        }))
    }

    fn token() -> TypeRef {
        TypeRef::Capability(CapabilityRef::Token(TokenCapability {
            ttl: "1h".to_owned(),
            single_use: true,
            store: TokenStore::Hashed,
        }))
    }

    fn many(inner: TypeRef) -> TypeRef {
        TypeRef::Many(Box::new(inner))
    }

    #[test]
    fn positive_sensitive_pii_without_capability_fires() {
        let resource = mk_resource("Customer", vec![mk_field("email", text())]);
        let finding = check_field(
            &resource,
            &resource.fields[0],
            &["contact"],
            Path::new("features/customer/customer.lzi"),
        )
        .expect("sensitive PII without capability must fire");

        assert_eq!(Finding::CODE, "VOCAB-CAP-MISSING-001");
        assert_eq!(finding.resource, "Customer");
        assert_eq!(finding.field, "email");
        assert_eq!(finding.pii_tag, "contact");
        assert_eq!(
            finding.message(),
            "field `Customer.email` carries `@pii.contact` but no `@cap.Hashed/Encrypted/Token` - sensitive data stored in plaintext"
        );
    }

    #[test]
    fn negative_capability_present_does_not_fire() {
        for type_ref in [hashed(), encrypted(), e2ee(), token()] {
            let resource = mk_resource("Customer", vec![mk_field("secret", type_ref)]);
            assert!(
                check_field(
                    &resource,
                    &resource.fields[0],
                    &["auth_secret"],
                    Path::new("features/customer/customer.lzi")
                )
                .is_none(),
                "capability-protected PII must not fire"
            );
        }
    }

    #[test]
    fn carve_out_derived_pii_does_not_fire() {
        let resource = mk_resource("Customer", vec![mk_field("score", text())]);
        assert!(
            check_field(
                &resource,
                &resource.fields[0],
                &["derived"],
                Path::new("features/customer/customer.lzi")
            )
            .is_none(),
            "@pii.derived is computed/classification-only and must not require a capability"
        );

        let resource = mk_resource("Customer", vec![mk_derived_field("risk", text())]);
        assert!(
            check_field(
                &resource,
                &resource.fields[0],
                &["financial"],
                Path::new("features/customer/customer.lzi")
            )
            .is_none(),
            "`derived from` fields are explicit computed fields"
        );
    }

    #[test]
    fn many_of_capability_does_not_fire() {
        let resource = mk_resource("Session", vec![mk_field("tokens", many(token()))]);
        assert!(
            check_field(
                &resource,
                &resource.fields[0],
                &["auth_secret"],
                Path::new("features/session/session.lzi")
            )
            .is_none(),
            "Many<@cap.Token> carries the capability on the inner type"
        );
    }

    #[test]
    fn golden_source_fixture_fires_for_plaintext_pii_field() {
        let source =
            include_str!("../../../../tests/golden/vocab/cap-missing/pii-contact-plaintext.lzi");

        let findings = check_source(
            source,
            Path::new("tests/golden/vocab/cap-missing/plain.lzi"),
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Customer");
        assert_eq!(findings[0].field, "email");
        assert_eq!(findings[0].pii_tag, "contact");
    }

    #[test]
    fn check_api_remains_noop_until_pii_is_lifted_into_field_ir() {
        let resource = mk_resource("Customer", vec![mk_field("email", text())]);
        let feature = mk_feature(vec![resource]);

        assert!(check(&feature, Path::new("features/customer/customer.lzi")).is_empty());
    }
