
    use super::*;
    use lazuli_ir::{BuiltinType, Defaults, Field, FieldConstraints, Policies, Resource, TypeRef};
    use std::fs;
    use tempfile::TempDir;

    // ── IR fixture builders ──────────────────────────────────────────────

    fn mk_field(name: &str) -> Field {
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
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>, timestamps: Option<bool>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps,
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
            name: "account".into(),
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

    /// Write `dist/go/migrations/<name>` under `root`, creating parents.
    fn write_migration(root: &Path, name: &str, body: &str) -> PathBuf {
        let dir = root.join("dist").join("go").join("migrations");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        fs::write(&p, body).unwrap();
        p
    }

    // ── positive case ────────────────────────────────────────────────────

    #[test]
    fn positive_ir_adds_column_no_alter_migration_fires() {
        let tmp = TempDir::new().unwrap();
        // Baseline CREATE without `updated_at`. IR has timestamps=true.
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL\n);\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        match &findings[0].kind {
            FindingKind::MissingAlter { missing, .. } => {
                assert!(missing.contains(&"updated_at".to_string()), "{missing:?}");
            }
            other => panic!("expected MissingAlter, got {other:?}"),
        }
        assert_eq!(Finding::CODE, "MIGRATION-ALTER-MISSING-001");
        assert_eq!(Finding::default_severity(), DoctorSeverity::Warning);
    }

    // ── baseline already has column → silent ────────────────────────────

    #[test]
    fn negative_baseline_contains_column_silent() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL,\n  updated_at TIMESTAMPTZ NOT NULL\n);\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── ALTER migration adds the column → silent ────────────────────────

    #[test]
    fn negative_alter_exists_silent() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL,\n  created_at TIMESTAMPTZ NOT NULL\n);\n",
        );
        write_migration(
            tmp.path(),
            "005_account_user_add_updated_at.sql",
            "ALTER TABLE \"user\" ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── multi-column ALTER → UnrecognisedMigration, not false-fire ──────

    #[test]
    fn negative_unrecognised_alter_form_does_not_fire() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL\n);\n",
        );
        write_migration(
            tmp.path(),
            "005_account_user_add_misc.sql",
            "ALTER TABLE \"user\" ADD COLUMN a INT, ADD COLUMN b TEXT;\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name"), mk_field("a"), mk_field("b")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert_eq!(findings.len(), 1, "{findings:?}");
        match &findings[0].kind {
            FindingKind::UnrecognisedMigration { snippet, .. } => {
                assert!(
                    snippet.to_ascii_lowercase().contains("alter table"),
                    "{snippet}"
                );
            }
            other => panic!("expected UnrecognisedMigration, got {other:?}"),
        }
    }

    // ── allow-comment opt-out silences the rule ─────────────────────────

    #[test]
    fn negative_allow_comment_silences() {
        let tmp = TempDir::new().unwrap();
        write_migration(
            tmp.path(),
            "004_account_user.sql",
            "CREATE TABLE \"user\" (\n  id BIGSERIAL PRIMARY KEY,\n  name TEXT NOT NULL\n);\n",
        );
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(
            &lzi,
            "# doctor:allow MIGRATION-ALTER-MISSING-001 — reason \"manual verify\"\nfeature account\n",
        )
        .unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── no migrations on disk → sibling rule's job, silent here ─────────

    #[test]
    fn negative_no_migrations_silent() {
        let tmp = TempDir::new().unwrap();
        let mut feature = mk_feature(vec![mk_resource(
            "User",
            vec![mk_field("name")],
            Some(true),
        )]);
        feature.defaults.timestamps = true;
        let lzi = tmp.path().join("account.lzi");
        fs::write(&lzi, "feature account\n").unwrap();

        let findings = check(&feature, &lzi, tmp.path());
        assert!(findings.is_empty(), "{findings:?}");
    }

    // ── ALTER parser unit tests ─────────────────────────────────────────

    #[test]
    fn alter_parser_picks_single_column_add() {
        let r = parse_alter_add_columns(
            "ALTER TABLE \"user\" ADD COLUMN updated_at TIMESTAMPTZ NOT NULL;",
            "user",
        );
        match r {
            AlterParseResult::Parsed(cols) => {
                assert_eq!(cols, vec!["updated_at".to_string()]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn alter_parser_recognises_if_not_exists() {
        let r = parse_alter_add_columns(
            "ALTER TABLE user ADD COLUMN IF NOT EXISTS phone TEXT;",
            "user",
        );
        match r {
            AlterParseResult::Parsed(cols) => assert_eq!(cols, vec!["phone".to_string()]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn alter_parser_ignores_other_tables() {
        let r = parse_alter_add_columns("ALTER TABLE other_table ADD COLUMN foo INT;", "user");
        match r {
            AlterParseResult::Parsed(cols) => assert!(cols.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn alter_parser_flags_multi_column_as_unrecognised() {
        let r = parse_alter_add_columns(
            "ALTER TABLE \"user\" ADD COLUMN a INT, ADD COLUMN b TEXT;",
            "user",
        );
        assert!(matches!(r, AlterParseResult::Unrecognised(_)), "{r:?}");
    }
