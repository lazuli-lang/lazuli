
    use super::*;
    use lazuli_ir::{Defaults, EnumVariant, Policies, QualifiedName, Resource};

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
                .map(|variant| EnumVariant {
                    name: variant.to_string(),
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

    fn json() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Json)
    }

    fn text() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    #[test]
    fn positive_quiz_questions_json_with_orphan_question_type_enum_fires() {
        let feature = mk_feature(
            vec![mk_enum(
                "QuizQuestionType",
                &["MultipleChoice", "TrueFalse", "FillIn"],
            )],
            vec![mk_resource(
                "Quiz",
                vec![
                    mk_field("title", text(), true),
                    mk_field("questions", json(), true),
                ],
            )],
        );

        let findings = check(&feature, Path::new("features/quiz/quiz.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Quiz");
        assert_eq!(findings[0].json_field, "questions");
        assert_eq!(findings[0].orphan_enum, "QuizQuestionType");
        assert_eq!(Finding::CODE, "VOCAB-JSON-TYPED-001");
    }

    #[test]
    fn negative_enum_referenced_elsewhere_does_not_fire() {
        let feature = mk_feature(
            vec![mk_enum(
                "QuizQuestionType",
                &["MultipleChoice", "TrueFalse", "FillIn"],
            )],
            vec![
                mk_resource(
                    "Quiz",
                    vec![
                        mk_field("title", text(), true),
                        mk_field("questions", json(), true),
                    ],
                ),
                mk_resource(
                    "Question",
                    vec![
                        mk_field("text", text(), true),
                        mk_field("kind", enum_ref("QuizQuestionType"), true),
                    ],
                ),
            ],
        );

        let findings = check(&feature, Path::new("features/quiz/quiz.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_json_field_without_matching_enum_does_not_fire() {
        let feature = mk_feature(
            vec![mk_enum("WorkflowStatus", &["Draft", "Live"])],
            vec![mk_resource(
                "Article",
                vec![
                    mk_field("title", text(), true),
                    mk_field("metadata", json(), true),
                ],
            )],
        );

        let findings = check(&feature, Path::new("features/article/article.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_resource_with_only_json_field_does_not_fire() {
        let feature = mk_feature(
            vec![mk_enum("PayloadType", &["A", "B"])],
            vec![mk_resource(
                "Payload",
                vec![mk_field("payload", json(), true)],
            )],
        );

        let findings = check(&feature, Path::new("features/payload/payload.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn multiple_resources_each_fire_independently() {
        let feature = mk_feature(
            vec![
                mk_enum("QuizQuestionType", &["MultipleChoice", "TrueFalse"]),
                mk_enum("SurveyAnswerType", &["Scale", "FreeText"]),
            ],
            vec![
                mk_resource(
                    "Quiz",
                    vec![
                        mk_field("title", text(), true),
                        mk_field("questions", json(), true),
                    ],
                ),
                mk_resource(
                    "Survey",
                    vec![
                        mk_field("name", text(), true),
                        mk_field("answers", json(), true),
                    ],
                ),
            ],
        );

        let findings = check(&feature, Path::new("features/forms/forms.lzi"));

        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].resource, "Quiz");
        assert_eq!(findings[1].resource, "Survey");
    }
