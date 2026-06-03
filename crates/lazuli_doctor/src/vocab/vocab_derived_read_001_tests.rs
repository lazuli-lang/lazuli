    use super::*;

    use lazuli_ir::{
        Assignment, BuiltinType, CapabilityRef, CommandInput, CommandKind, CreateEffect,
        DefaultValue, Defaults, Expr, Feature, Field, HashedCapability, HashAlgorithm,
        Job, JobBody, JobDeclarative, JobTrigger, Policies, QualifiedName, Resource, TypeRef,
    };
    use std::path::Path;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn mk_feature(resources: Vec<Resource>, commands: Vec<ir::Command>, jobs: Vec<Job>) -> Feature {
        Feature {
            name: "test_feature".into(),
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
            commands,
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs,
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

    fn text_field_opt(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
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

    fn text_field_req(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
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

    fn mk_create_cmd(resource: &str, field_names: &[&str]) -> ir::Command {
        let assignments = field_names
            .iter()
            .map(|f| Assignment {
                field: f.to_string(),
                value: Expr::String("value".into()),
            })
            .collect();
        ir::Command {
            name: "create_cmd".into(),
            public_contract: None,
            kind: CommandKind::Create,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Creates(CreateEffect {
                resource: QualifiedName {
                    feature: None,
                    name: resource.into(),
                },
                from_input: false,
                assignments,
            }),
            policy: ir::PolicyRef::None,
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

    fn mk_create_from_input_cmd(resource: &str) -> ir::Command {
        ir::Command {
            name: "create_from_input".into(),
            public_contract: None,
            kind: CommandKind::Create,
            route: vec![],
            input: CommandInput::Empty,
            target: None,
            lets: vec![],
            effect: CommandEffect::Creates(CreateEffect {
                resource: QualifiedName {
                    feature: None,
                    name: resource.into(),
                },
                from_input: true,
                assignments: vec![],
            }),
            policy: ir::PolicyRef::None,
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

    // ── positive ─────────────────────────────────────────────────────────────

    /// Optional field never assigned in any command → fires.
    #[test]
    fn positive_never_written_optional_fires() {
        let resource = mk_resource("Post", vec![text_field_opt("canonical_url")]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Post");
        assert_eq!(findings[0].field, "canonical_url");
        assert_eq!(Finding::CODE, "VOCAB-DERIVED-READ-001");
        assert!(findings[0].message().contains("derived from"));
    }

    // ── negative (i): field assigned in creates ───────────────────────────────

    /// Field explicitly assigned in a `creates` command → no finding.
    #[test]
    fn negative_assigned_in_creates() {
        let resource = mk_resource("Post", vec![text_field_opt("canonical_url")]);
        let cmd = mk_create_cmd("Post", &["canonical_url"]);
        let feature = mk_feature(vec![resource], vec![cmd], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "field with a creates assignment must not fire"
        );
    }

    // ── negative (ii): field has @cap.* tier ─────────────────────────────────

    /// Field typed `@cap.Hashed` → storage semantics imply it is persisted.
    #[test]
    fn negative_cap_field_skipped() {
        let cap_field = Field {
            name: "password_hash".into(),
            type_ref: TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
                algorithm: HashAlgorithm::Argon2id,
            })),
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
        };
        let resource = mk_resource("User", vec![cap_field]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/user/user.lzi"));
        assert!(
            findings.is_empty(),
            "@cap.* field must not trigger VOCAB-DERIVED-READ-001"
        );
    }

    // ── negative (iii): field has default ────────────────────────────────────

    /// Field with an explicit `default` value → storage intent, not derived.
    #[test]
    fn negative_default_value_skipped() {
        let default_field = Field {
            name: "status".into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: Some(DefaultValue::String("active".into())),
            derived_from: None,
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        };
        let resource = mk_resource("Account", vec![default_field]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/account/account.lzi"));
        assert!(
            findings.is_empty(),
            "field with a default value must not fire"
        );
    }

    // ── negative (iv): `creates X from input` suppresses the resource ─────────

    /// When `creates Post from input` is present, all Post fields are
    /// treated as potentially written → no finding even for `canonical_url`.
    #[test]
    fn negative_from_input_suppresses_resource() {
        let resource = mk_resource("Post", vec![text_field_opt("canonical_url")]);
        let cmd = mk_create_from_input_cmd("Post");
        let feature = mk_feature(vec![resource], vec![cmd], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "`creates X from input` must suppress the resource from VOCAB-DERIVED-READ-001"
        );
    }

    // ── negative (v): already derived_from ───────────────────────────────────

    /// Field already annotated with `derived from <expr>` → skip.
    #[test]
    fn negative_already_derived() {
        let derived_field = Field {
            name: "canonical_url".into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: Some("\"https://example.com/p/{{slug}}\"".into()),
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        };
        let resource = mk_resource("Post", vec![derived_field]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "already-derived field must not fire"
        );
    }

    // ── positive: declarative job write site suppresses ──────────────────────

    /// A declarative job that assigns the field counts as a write site.
    #[test]
    fn negative_job_write_site_suppresses() {
        let resource = mk_resource("Post", vec![text_field_opt("canonical_url")]);
        let job = Job {
            name: "rebuild_slug_job".into(),
            trigger: JobTrigger::Schedule {
                cron: "0 * * * *".into(),
            },
            queue: None,
            idempotency: None,
            retry: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            tenant_from: None,
            fanout: None,
            timeout: None,
            external_calls: vec![],
            body: JobBody::Declarative(JobDeclarative {
                target: None,
                lets: vec![],
                effect: CommandEffect::Creates(CreateEffect {
                    resource: QualifiedName {
                        feature: None,
                        name: "Post".into(),
                    },
                    from_input: false,
                    assignments: vec![Assignment {
                        field: "canonical_url".into(),
                        value: Expr::String("computed".into()),
                    }],
                }),
            }),
            emits: vec![],
            previous_names: vec![],
            span_ref: None,
        };
        let feature = mk_feature(vec![resource], vec![], vec![job]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "declarative job write site must suppress VOCAB-DERIVED-READ-001"
        );
    }

    // ── negative: required field skipped ─────────────────────────────────────

    /// Required fields must be set on creates — not a derived-field candidate.
    #[test]
    fn negative_required_field_skipped() {
        let resource = mk_resource("Post", vec![text_field_req("slug")]);
        let feature = mk_feature(vec![resource], vec![], vec![]);
        let findings = check(&feature, Path::new("features/post/post.lzi"));
        assert!(
            findings.is_empty(),
            "required fields must not trigger VOCAB-DERIVED-READ-001"
        );
    }

    // ── waiver wiring (spec 0028) ────────────────────────────────────────────
    //
    // The rule's message advertises
    // `@doctor.allow(VOCAB-DERIVED-READ-001, reason: "…")` for an intentionally
    // read-only/materialized field. These tests write the source to a real
    // on-disk `.lzi` so `check`'s `file_contains_doctor_allow(path, CODE)` scan
    // observes the waiver — the pre-fix gap was that this scan was never
    // consulted, so the opt-out was inert.

    fn lower_from_source(source: &str) -> Feature {
        let skeletons =
            lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
        lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
    }

    /// The never-written optional field fixture WITHOUT a waiver (on disk) fires.
    #[test]
    fn on_disk_without_waiver_still_fires() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("post.lzi");
        let source = r#"
feature post
  domain
    resource Post
      id: ID required
      canonical_url: Text
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower_from_source(source);
        let findings = check(&feature, &path);
        assert_eq!(
            findings.len(),
            1,
            "no waiver present → finding must stand: {findings:?}"
        );
        assert_eq!(findings[0].field, "canonical_url");
    }

    /// A `@doctor.allow(VOCAB-DERIVED-READ-001, …)` node suppresses the finding.
    #[test]
    fn node_form_doctor_allow_suppresses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("post.lzi");
        let source = r#"
@doctor.allow(VOCAB-DERIVED-READ-001, reason: "materialized read column, backfilled by ETL")
feature post
  domain
    resource Post
      id: ID required
      canonical_url: Text
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower_from_source(source);
        let findings = check(&feature, &path);
        assert!(
            findings.is_empty(),
            "@doctor.allow(VOCAB-DERIVED-READ-001, …) must suppress: {findings:?}"
        );
    }

    /// The legacy `# doctor:allow VOCAB-DERIVED-READ-001` comment form also
    /// suppresses (back-compat bridge).
    #[test]
    fn legacy_comment_form_doctor_allow_suppresses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("post.lzi");
        let source = r#"
# doctor:allow VOCAB-DERIVED-READ-001 — reason "materialized read column"
feature post
  domain
    resource Post
      id: ID required
      canonical_url: Text
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower_from_source(source);
        let findings = check(&feature, &path);
        assert!(
            findings.is_empty(),
            "legacy # doctor:allow comment must suppress: {findings:?}"
        );
    }

    /// A waiver for a DIFFERENT code does not suppress this finding.
    #[test]
    fn doctor_allow_for_other_code_does_not_suppress() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("post.lzi");
        let source = r#"
@doctor.allow(SOME-OTHER-RULE-001, reason: "unrelated")
feature post
  domain
    resource Post
      id: ID required
      canonical_url: Text
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower_from_source(source);
        let findings = check(&feature, &path);
        assert_eq!(
            findings.len(),
            1,
            "a waiver for a different code must not suppress: {findings:?}"
        );
    }
