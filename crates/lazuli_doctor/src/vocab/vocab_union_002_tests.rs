
    use super::*;
    use lazuli_ir::{Defaults, EnumVariant, Policies, QualifiedName};

    fn mk_field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required,
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

    fn mk_enum(name: &str, variants: &[&str]) -> EnumDecl {
        EnumDecl {
            name: name.into(),
            public_contract: None,
            variants: variants
                .iter()
                .map(|v| EnumVariant {
                    name: v.to_string(),
                    storage_value: None,
                    label_key: None,
                    hint_key: None,
                    icon_key: None,
                    previous_names: vec![],
                })
                .collect(),
            previous_names: vec![],
            span_ref: None,
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

    fn mk_feature(enums: Vec<EnumDecl>, resources: Vec<Resource>) -> Feature {
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
            enums,
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

    fn enum_ref(name: &str) -> TypeRef {
        TypeRef::EnumRef(QualifiedName {
            feature: None,
            name: name.into(),
        })
    }

    fn cross_feature_enum_ref(feature: &str, name: &str) -> TypeRef {
        TypeRef::EnumRef(QualifiedName {
            feature: Some(feature.into()),
            name: name.into(),
        })
    }

    fn user_defined(name: &str) -> TypeRef {
        TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: name.into(),
        })
    }

    fn id() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Id)
    }

    fn text() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    // ── positive ─────────────────────────────────────────────────────────────

    #[test]
    fn positive_target_enum_plus_target_id_fires() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field("target", enum_ref("CommentTarget"), true),
                mk_field("target_id", id(), true),
                mk_field("body", text(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Comment");
        assert_eq!(findings[0].discriminator_field, "target");
        assert_eq!(findings[0].fk_field, "target_id");
        assert_eq!(findings[0].enum_name, "CommentTarget");
        assert_eq!(
            findings[0].variants,
            vec!["Issue".to_string(), "Customer".to_string()]
        );
        assert_eq!(Finding::CODE, "VOCAB-UNION-002");
        assert!(findings[0].message().contains("discriminated union"));
    }

    #[test]
    fn positive_subject_enum_plus_subject_id_fires() {
        let resource = mk_resource(
            "Activity",
            vec![
                mk_field("subject", enum_ref("ActivitySubject"), true),
                mk_field("subject_id", id(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("ActivitySubject", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/activity/activity.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].discriminator_field, "subject");
        assert_eq!(findings[0].fk_field, "subject_id");
    }

    // ── negative ─────────────────────────────────────────────────────────────

    #[test]
    fn negative_only_one_variant_does_not_fire() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field("target", enum_ref("CommentTarget"), true),
                mk_field("target_id", id(), true),
            ],
        );
        let feature = mk_feature(vec![mk_enum("CommentTarget", &["Issue"])], vec![resource]);

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_typed_fk_does_not_fire() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field("target", enum_ref("CommentTarget"), true),
                mk_field("target_id", user_defined("Issue"), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_random_name_pair_does_not_fire() {
        let resource = mk_resource(
            "TaggedThing",
            vec![
                mk_field("tag", enum_ref("TagKind"), true),
                mk_field("tag_id", id(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("TagKind", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/tagged/tagged.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_missing_paired_id_does_not_fire() {
        let resource = mk_resource(
            "Comment",
            vec![mk_field("target", enum_ref("CommentTarget"), true)],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn cross_feature_enum_does_not_fire() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field(
                    "target",
                    cross_feature_enum_ref("other", "CommentTarget"),
                    true,
                ),
                mk_field("target_id", id(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert!(findings.is_empty());
    }
