
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
