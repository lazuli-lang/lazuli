
    use super::*;

    fn lower(source: &str) -> Feature {
        let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
        lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
    }

    #[test]
    fn mutation_with_matching_lookup_does_not_fire() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

    query.lookup lookup_my_customer by id: ID

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "expected no diagnostic when a matching lookup exists, got: {:?}",
            findings
        );
    }

    #[test]
    fn mutation_without_any_read_query_fires_warning() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "expected one finding, got: {:?}",
            findings
        );
        let f = &findings[0];
        assert_eq!(f.command, "update_customer");
        assert_eq!(f.effect_kind, "updates");
        assert_eq!(f.resource_snake, "customer");
        assert_eq!(f.resource_display, "Customer");
        assert_eq!(Finding::CODE, "MUTATION-WITHOUT-READBACK-001");
        assert_eq!(Finding::ID, "@correctness.mutation_without_readback");
        let msg = f.message();
        assert!(msg.contains("command 'update_customer'"), "msg: {msg}");
        assert!(msg.contains("resource 'Customer'"), "msg: {msg}");
        assert!(msg.contains("lookup_my_customer"), "msg: {msg}");
        assert!(msg.contains("mine_customers"), "msg: {msg}");
    }

    #[test]
    fn list_query_with_matching_resource_counts_as_readback() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

    query.list mine_customers

  command create_customer
    creates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert!(
            findings.is_empty(),
            "list query should satisfy readback: {:?}",
            findings
        );
    }

    #[test]
    fn delete_without_readback_fires() {
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command delete_customer
    route id: ID
    deletes Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].effect_kind, "deletes");
    }

    #[test]
    fn returns_command_without_query_does_not_fire() {
        // `command compute_total returns Money` is a pure read shape; it has
        // no `creates/updates/deletes` effect, so the rule does not apply.
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command compute_total
    returns Money
"#,
        );

        assert!(check(&feature, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn cross_feature_lookup_satisfies_readback() {
        let owner = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let reader = lower(
            r#"
feature crm
  uses billing
  domain
    query.lookup lookup_my_customer by id: ID
"#,
        );

        let findings = check_with_neighbors(&owner, Path::new("billing.lzi"), [&reader]);
        assert!(
            findings.is_empty(),
            "cross-feature lookup_my_customer should satisfy readback: {:?}",
            findings
        );
    }

    #[test]
    fn sql_query_does_not_satisfy_readback() {
        // SQL queries are not cache-reachable for `invalidates` wiring;
        // they intentionally do not count toward the readback test.
        let feature = lower(
            r#"
feature billing
  domain
    resource Customer
      id: ID required

    query.sql search_customers
      returns CustomerSearchHit
      sql "./queries/search.sql"

  command update_customer
    route id: ID
    updates Customer
"#,
        );

        let findings = check(&feature, Path::new("billing.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "sql query must NOT satisfy readback: {:?}",
            findings
        );
    }

    // ── waiver wiring (spec 0028) ────────────────────────────────────────────
    //
    // The rule's own message advertises
    // `@doctor.allow(MUTATION-WITHOUT-READBACK-001, reason: "…")` as the escape
    // hatch. These tests write the source to a real on-disk `.lzi` so `check`'s
    // `file_contains_doctor_allow(path, CODE)` scan observes the waiver — the
    // pre-fix gap was that this scan was never consulted, so the opt-out was
    // inert.

    /// The bare no-readback feature WITHOUT a waiver, written to a real file,
    /// still fires — guards that the on-disk read path itself doesn't suppress.
    #[test]
    fn on_disk_without_waiver_still_fires() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("billing.lzi");
        let source = r#"
feature billing
  domain
    resource Customer
      id: ID required

  command update_customer
    route id: ID
    updates Customer
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower(source);
        let findings = check(&feature, &path);
        assert_eq!(
            findings.len(),
            1,
            "no waiver present → finding must stand: {findings:?}"
        );
    }

    /// A `@doctor.allow(MUTATION-WITHOUT-READBACK-001, …)` node on the same file
    /// suppresses the finding.
    #[test]
    fn node_form_doctor_allow_suppresses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("billing.lzi");
        let source = r#"
feature billing
  domain
    resource Customer
      id: ID required

  @doctor.allow(MUTATION-WITHOUT-READBACK-001, reason: "fire-and-forget audit log")
  command update_customer
    route id: ID
    updates Customer
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower(source);
        let findings = check(&feature, &path);
        assert!(
            findings.is_empty(),
            "@doctor.allow(MUTATION-WITHOUT-READBACK-001, …) must suppress: {findings:?}"
        );
    }

    /// The legacy `# doctor:allow MUTATION-WITHOUT-READBACK-001` comment form
    /// also suppresses (back-compat bridge).
    #[test]
    fn legacy_comment_form_doctor_allow_suppresses() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("billing.lzi");
        let source = r#"
feature billing
  domain
    resource Customer
      id: ID required

  # doctor:allow MUTATION-WITHOUT-READBACK-001 — reason "fire-and-forget audit log"
  command update_customer
    route id: ID
    updates Customer
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower(source);
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
        let path = dir.path().join("billing.lzi");
        let source = r#"
feature billing
  domain
    resource Customer
      id: ID required

  @doctor.allow(SOME-OTHER-RULE-001, reason: "unrelated")
  command update_customer
    route id: ID
    updates Customer
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower(source);
        let findings = check(&feature, &path);
        assert_eq!(
            findings.len(),
            1,
            "a waiver for a different code must not suppress: {findings:?}"
        );
    }

    /// The CLI dispatch entry point (`check_from_facts`) honors the waiver too,
    /// not just the LSP `check` path.
    #[test]
    fn check_from_facts_honors_waiver() {
        use lazuli_ir::CommandEffect;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("billing.lzi");
        let source = r#"
feature billing
  domain
    resource Customer
      id: ID required

  @doctor.allow(MUTATION-WITHOUT-READBACK-001, reason: "fire-and-forget audit log")
  command update_customer
    route id: ID
    updates Customer
"#;
        std::fs::write(&path, source).expect("write fixture");
        let feature = lower(source);

        // Project the lowered feature into the (commands, queries) facts shape.
        let commands: Vec<_> = feature.commands.clone();
        let queries: Vec<_> = feature.queries.clone();
        // Sanity: the fixture really does carry a mutating command with no read.
        assert!(
            commands
                .iter()
                .any(|c| matches!(c.effect, CommandEffect::Updates(_))),
            "fixture must contain an updates command"
        );

        let findings = check_from_facts(&feature.name, &commands, &queries, &[], &path);
        assert!(
            findings.is_empty(),
            "check_from_facts must honor the waiver: {findings:?}"
        );

        // And without the file on disk (guaranteed-missing path), the same facts
        // DO fire — proving the suppression came from the waiver, not the shape.
        let missing = dir.path().join("nonexistent.lzi");
        let fires = check_from_facts(&feature.name, &commands, &queries, &[], &missing);
        assert_eq!(
            fires.len(),
            1,
            "without the on-disk waiver, check_from_facts fires: {fires:?}"
        );
    }
