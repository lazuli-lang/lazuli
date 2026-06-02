
    use lazuli_ir::{
        Auth, AuthIdentity, AuthSessions, BuiltinType, Defaults, Feature, Field, FieldConstraints,
        FieldRef, Policies, QualifiedName, Resource, SpanRef, TypeRef,
    };

    use super::*;

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
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

    /// A session resource with the given fields and a span (so `offset`
    /// flows into the finding).
    fn mk_session_resource(name: &str, fields: Vec<Field>) -> Resource {
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
            span_ref: Some(SpanRef { start: 17, end: 88 }),
            lifecycle: None,
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    /// The canonical complete session resource (parity with pauta's
    /// `UserSession`): identity FK + token_hash + expires_at + created_at +
    /// revoked_at.
    fn complete_user_session() -> Resource {
        mk_session_resource(
            "UserSession",
            vec![
                mk_field("user", TypeRef::UserDefined(qn("User"))),
                mk_field("org_id", TypeRef::UserDefined(qn("Org"))),
                mk_field("token_hash", TypeRef::Builtin(BuiltinType::Text)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
                mk_field("created_at", TypeRef::Builtin(BuiltinType::DateTime)),
                mk_field("revoked_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
        )
    }

    /// A feature whose `auth` block names `session_resource` as the session
    /// resource and `identity_resource` as the auth identity domain.
    fn feature_with(
        identity_resource: &str,
        session_resource: &str,
        resources: Vec<Resource>,
    ) -> Feature {
        Feature {
            name: "account".to_owned(),
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
            auth: Some(Auth {
                identity: AuthIdentity {
                    field: FieldRef {
                        resource: qn(identity_resource),
                        field: "email".to_owned(),
                    },
                    public_contract: None,
                },
                password: None,
                sessions: Some(AuthSessions {
                    resource: qn(session_resource),
                    ttl: "7 days".to_owned(),
                    refresh: false,
                    extra_columns: vec![],
                    access_ttl: None,
                    rotation: None,
                    cookie: None,
                }),
                mfa: None,
                oauth: vec![],
                span_ref: None,
            }),
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

    #[test]
    fn code_constants_are_stable() {
        assert_eq!(Finding::CODE, "auth_session_canonical_columns_001");
        assert_eq!(Finding::KEBAB_CODE, "auth-session-canonical-columns");
    }

    #[test]
    fn negative_complete_session_resource_is_clean() {
        // Parity with pauta's UserSession — no false positive.
        let feature = feature_with("User", "UserSession", vec![complete_user_session()]);
        let findings = check(&feature, Path::new("account.lzi"));
        assert!(findings.is_empty(), "got: {findings:?}");
    }

    #[test]
    fn positive_missing_expires_at_fires_naming_it() {
        // The W2-5 example: a session resource missing `expires_at`.
        let session = mk_session_resource(
            "UserSession",
            vec![
                mk_field("user", TypeRef::UserDefined(qn("User"))),
                mk_field("token_hash", TypeRef::Builtin(BuiltinType::Text)),
                // expires_at deliberately absent.
            ],
        );
        let feature = feature_with("User", "UserSession", vec![session]);
        let findings = check(&feature, Path::new("account.lzi"));
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert_eq!(findings[0].missing, MissingColumn::ExpiresAt);
        assert_eq!(findings[0].session_resource, "UserSession");
        assert_eq!(findings[0].offset, Some(17));
        assert!(findings[0].message().contains("expires_at"));
        assert!(findings[0].message().contains("403"));
    }

    #[test]
    fn positive_missing_identity_fk_fires_naming_identity_resource() {
        // A session resource carrying expires_at + token_hash but no FK to
        // the identity resource `User` — the resolver has no actor to
        // return.
        let session = mk_session_resource(
            "UserSession",
            vec![
                mk_field("token_hash", TypeRef::Builtin(BuiltinType::Text)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
        );
        let feature = feature_with("User", "UserSession", vec![session]);
        let findings = check(&feature, Path::new("account.lzi"));
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert_eq!(
            findings[0].missing,
            MissingColumn::IdentityRef {
                identity_resource: "User".to_owned()
            }
        );
        assert!(findings[0].message().contains("User"));
    }

    #[test]
    fn identity_fk_is_name_agnostic_customer_satisfies_customer_identity() {
        // full-capsule shape: identity is `Customer`, FK field is `customer`
        // (not `user`). Must not false-positive on the identity axis. The
        // resource carries expires_at too, so it is fully clean.
        let session = mk_session_resource(
            "CustomerSession",
            vec![
                mk_field("customer", TypeRef::UserDefined(qn("Customer"))),
                mk_field("refresh_token_hash", TypeRef::Builtin(BuiltinType::Text)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
        );
        let feature = feature_with("Customer", "CustomerSession", vec![session]);
        let findings = check(&feature, Path::new("auth.lzi"));
        assert!(findings.is_empty(), "got: {findings:?}");
    }

    #[test]
    fn rotation_only_resource_without_plain_token_hash_is_clean() {
        // production-grade / user-auth shape: carries only
        // `refresh_token_hash` (no plain `token_hash`). The rule must NOT
        // require the credential-hash column, so this is clean.
        let session = mk_session_resource(
            "UserSession",
            vec![
                mk_field("user", TypeRef::UserDefined(qn("User"))),
                mk_field("refresh_token_hash", TypeRef::Builtin(BuiltinType::Text)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
        );
        let feature = feature_with("User", "UserSession", vec![session]);
        let findings = check(&feature, Path::new("auth.lzi"));
        assert!(findings.is_empty(), "got: {findings:?}");
    }

    #[test]
    fn omitting_id_and_created_at_is_clean_framework_synthesized() {
        // A session resource with only the two genuinely-required author
        // columns (identity FK + expires_at). `id`/`created_at` are
        // framework-synthesized, so their absence is fine.
        let session = mk_session_resource(
            "UserSession",
            vec![
                mk_field("user", TypeRef::UserDefined(qn("User"))),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
        );
        let feature = feature_with("User", "UserSession", vec![session]);
        let findings = check(&feature, Path::new("auth.lzi"));
        assert!(findings.is_empty(), "got: {findings:?}");
    }

    #[test]
    fn both_columns_missing_fires_twice() {
        // A degenerate session resource missing BOTH required columns.
        let session = mk_session_resource(
            "UserSession",
            vec![mk_field("token_hash", TypeRef::Builtin(BuiltinType::Text))],
        );
        let feature = feature_with("User", "UserSession", vec![session]);
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 2, "got: {findings:?}");
        assert!(
            findings
                .iter()
                .any(|f| f.missing == MissingColumn::ExpiresAt)
        );
        assert!(findings.iter().any(|f| matches!(
            &f.missing,
            MissingColumn::IdentityRef { identity_resource } if identity_resource == "User"
        )));
    }

    #[test]
    fn edge_no_auth_block_does_not_fire() {
        let mut feature = feature_with("User", "UserSession", vec![complete_user_session()]);
        feature.auth = None;
        let findings = check(&feature, Path::new("auth.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn edge_no_sessions_block_does_not_fire() {
        let mut feature = feature_with("User", "UserSession", vec![complete_user_session()]);
        if let Some(auth) = feature.auth.as_mut() {
            auth.sessions = None;
        }
        let findings = check(&feature, Path::new("auth.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn edge_unknown_session_resource_defers_to_sibling_rule() {
        // The binding names a resource the feature does not declare —
        // auth_sessions_resource_unknown_001 owns that case; this rule
        // stays silent so the two never double-fire.
        let feature = feature_with("User", "MissingSession", vec![complete_user_session()]);
        let findings = check(&feature, Path::new("auth.lzi"));
        assert!(findings.is_empty(), "got: {findings:?}");
    }
