
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Feature, Field, FieldConstraints, Policies, Resource, TypeRef,
    };

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: false,
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

    fn mk_resource(name: &str, fields: &[&str]) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields: fields.iter().map(|f| mk_field(f)).collect(),
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

    fn mk_feature(name: &str, resources: Vec<Resource>) -> Feature {
        Feature {
            name: name.into(),
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

    /// Temp dir + `<feature>.lzi` so the co-located `<feature>.ctx.md`
    /// resolves the way the doctor walker sees it. Returns (dir, lzi_path).
    fn temp_setup(feature: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tmpdir");
        let lzi = dir.path().join(format!("{feature}.lzi"));
        std::fs::write(&lzi, "feature dummy\n").expect("seed lzi");
        (dir, lzi)
    }

    fn write_ctx(dir: &tempfile::TempDir, feature: &str, body: &str) {
        std::fs::write(dir.path().join(format!("{feature}.ctx.md")), body).expect("write ctx");
    }

    // ── POSITIVE fixture: a Data-Model table shadowing a resource ────────────

    #[test]
    fn data_model_table_shadowing_resource_fires() {
        let (dir, lzi) = temp_setup("proposals");
        // A markdown table whose header cells are the resource's field names.
        let ctx = "\
# Proposals context

Proposals carry a voting workflow.

## Data Model

| id | title | status | created_at |
|----|-------|--------|------------|
| u1 | Foo   | open   | 2026-01-01 |
| u2 | Bar   | closed | 2026-02-01 |

The status enum is open/closed.
";
        write_ctx(&dir, "proposals", ctx);
        let feature = mk_feature(
            "proposals",
            vec![mk_resource(
                "Proposal",
                &["id", "title", "status", "created_at", "org_id"],
            )],
        );
        let findings = check(&feature, &lzi);
        assert_eq!(findings.len(), 1, "expected one shadowing table finding");
        assert_eq!(findings[0].resource, "Proposal");
        assert_eq!(findings[0].overlap, 4); // id,title,status,created_at
        assert!(findings[0].message().contains("Proposal"));
        assert!(findings[0].message().contains("inspect --expand=context"));
        assert_eq!(Finding::CODE, "VOCAB-CONTEXT-PROSE-SHADOWS-IR-001");
    }

    // ── NEGATIVE fixture: incidental field name in running prose ─────────────

    #[test]
    fn field_name_in_running_prose_stays_silent() {
        let (dir, lzi) = temp_setup("proposals");
        // Mentions field names, but in PROSE — no qualifying table.
        let ctx = "\
# Proposals context

We soft-delete on proposals: the `status` column flips to `archived` and the
`created_at` timestamp is preserved. The `title` is never mutated after
publication.

This is a long-form explanation of intent, not a schema dump.
";
        write_ctx(&dir, "proposals", ctx);
        let feature = mk_feature(
            "proposals",
            vec![mk_resource(
                "Proposal",
                &["id", "title", "status", "created_at"],
            )],
        );
        assert!(
            check(&feature, &lzi).is_empty(),
            "running prose with no table must not fire",
        );
    }

    #[test]
    fn no_sidecar_is_silent() {
        let (_dir, lzi) = temp_setup("proposals");
        let feature = mk_feature("proposals", vec![mk_resource("Proposal", &["id", "title"])]);
        assert!(check(&feature, &lzi).is_empty());
    }

    #[test]
    fn table_below_threshold_does_not_fire() {
        let (dir, lzi) = temp_setup("proposals");
        // Only 2 header cells match the resource fields (id, title) → < 3.
        let ctx = "\
## Some unrelated table

| id | title | author_blurb |
|----|-------|--------------|
| 1  | x     | hi           |
";
        write_ctx(&dir, "proposals", ctx);
        let feature = mk_feature(
            "proposals",
            vec![mk_resource("Proposal", &["id", "title", "status"])],
        );
        assert!(check(&feature, &lzi).is_empty());
    }

    #[test]
    fn fires_regardless_of_heading_text() {
        // Heading says "Random Notes", not "Data Model" — still fires.
        let (dir, lzi) = temp_setup("billing");
        let ctx = "\
## Random Notes

| Amount | Currency | Captured At |
|--------|----------|-------------|
| 100    | USD      | now         |
";
        write_ctx(&dir, "billing", ctx);
        let feature = mk_feature(
            "billing",
            vec![mk_resource("Invoice", &["amount", "currency", "captured_at"])],
        );
        let findings = check(&feature, &lzi);
        assert_eq!(findings.len(), 1, "heading text is free-form; table still fires");
        assert_eq!(findings[0].resource, "Invoice");
    }

    #[test]
    fn no_resources_is_silent() {
        let (dir, lzi) = temp_setup("empty");
        let ctx = "\
| a | b | c |
|---|---|---|
| 1 | 2 | 3 |
";
        write_ctx(&dir, "empty", ctx);
        let feature = mk_feature("empty", vec![]);
        assert!(check(&feature, &lzi).is_empty());
    }

    #[test]
    fn normalization_handles_kebab_space_and_backticks() {
        assert_eq!(normalize("`created_at`"), "created_at");
        assert_eq!(normalize("Created At"), "created_at");
        assert_eq!(normalize("created-at"), "created_at");
        assert_eq!(normalize("  ORG_ID  "), "org_id");
        assert_eq!(normalize("|||"), "");
    }

    #[test]
    fn delimiter_detection() {
        assert!(is_delimiter_row("|---|---|"));
        assert!(is_delimiter_row("| :--- | ---: | :--: |"));
        assert!(!is_delimiter_row("| a | b |"));
        assert!(!is_delimiter_row("just prose"));
    }

    /// Tabled coverage — one row per disposition.
    #[test]
    fn tabled_cases() {
        // (label, ctx_body, resource_fields, expect_finding)
        let cases: &[(&str, &str, &[&str], bool)] = &[
            (
                "three_col_table_fires",
                "| id | name | status |\n|---|---|---|\n| 1 | a | ok |\n",
                &["id", "name", "status", "extra"],
                true,
            ),
            (
                "two_col_table_silent",
                "| id | name |\n|---|---|\n| 1 | a |\n",
                &["id", "name", "status"],
                false,
            ),
            (
                "prose_only_silent",
                "The id and name and status are all important fields.\n",
                &["id", "name", "status"],
                false,
            ),
            (
                "no_delimiter_not_a_table",
                "| id | name | status |\nbut no delimiter row follows\n",
                &["id", "name", "status"],
                false,
            ),
        ];
        for (label, body, fields, expect) in cases {
            let (dir, lzi) = temp_setup(label);
            write_ctx(&dir, label, body);
            let feature = mk_feature(label, vec![mk_resource("R", fields)]);
            let got = !check(&feature, &lzi).is_empty();
            assert_eq!(got, *expect, "case `{label}`: expected {expect}, got {got}");
        }
    }
