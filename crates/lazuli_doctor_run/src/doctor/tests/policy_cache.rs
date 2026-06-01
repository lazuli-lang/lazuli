    // Doctor missing-policy-on-query + duplicate-name + cache rule tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;


    const MISSING_POLICY_ON_QUERY_HAPPY_FIXTURE: &str =
        include_str!("../../../tests/fixtures/missing-policy-on-query/happy.lzi");
    const MISSING_POLICY_ON_QUERY_MISSING_FIXTURE: &str =
        include_str!("../../../tests/fixtures/missing-policy-on-query/missing.lzi");
    const MISSING_POLICY_ON_QUERY_EXPLICIT_PUBLIC_FIXTURE: &str =
        include_str!("../../../tests/fixtures/missing-policy-on-query/explicit_public.lzi");

    #[test]
    fn missing_policy_on_query_happy_fixture_has_zero_diagnostics() {
        let package =
            package_from_sources(vec![("happy.lzi", MISSING_POLICY_ON_QUERY_HAPPY_FIXTURE)]);
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            diagnostics.is_empty(),
            "expected happy fixture to emit zero diagnostics, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn missing_policy_on_query_missing_fixture_fires_once() {
        let package = package_from_sources(vec![(
            "missing.lzi",
            MISSING_POLICY_ON_QUERY_MISSING_FIXTURE,
        )]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "MISSING-POLICY-ON-QUERY-001"),
            1,
            "expected exactly one MISSING-POLICY-ON-QUERY-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn missing_policy_on_query_explicit_public_fixture_has_zero_diagnostics() {
        let package = package_from_sources(vec![(
            "explicit_public.lzi",
            MISSING_POLICY_ON_QUERY_EXPLICIT_PUBLIC_FIXTURE,
        )]);
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            diagnostics.is_empty(),
            "expected explicit public fixture to emit zero diagnostics, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn duplicate_query_name_author_duplicate_fires_once_through_doctor() {
        let package = package_from_sources(vec![(
            "catalog.lzi",
            r#"
feature catalog
  query.list list_customers
  query.list list_customers
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "DUPLICATE-QUERY-NAME-001")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one DUPLICATE-QUERY-NAME-001; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Error);
        assert!(
            hits[0]
                .message
                .contains("feature 'catalog' declares query 'list_customers' more than once"),
            "message should name the feature and duplicate query; got {}",
            hits[0].message
        );
    }

    // =========================================================================
    // Cache bucket cycle (row 51) — 5 doctor diagnostics on QueryCache /
    // Command.invalidates / registry capabilities.
    // =========================================================================

    const CACHE_INVALIDATES_UNRESOLVED_FIXTURE: &str =
        include_str!("../../../tests/fixtures/cache/invalidates_target_unresolved.lzi");
    const CACHE_NAMESPACE_COLLISION_FIXTURE: &str =
        include_str!("../../../tests/fixtures/cache/namespace_collision.lzi");
    const CACHE_CAPABILITY_UNDECLARED_FIXTURE: &str =
        include_str!("../../../tests/fixtures/cache/capability_undeclared.lzi");
    // CL.C.3 — feature-level `cache <name>` profile diagnostics.
    const CACHE_PROFILE_UNKNOWN_FIXTURE: &str =
        include_str!("../../../tests/fixtures/cache/profile_unknown.lzi");
    const CACHE_TAG_UNKNOWN_FIXTURE: &str =
        include_str!("../../../tests/fixtures/cache/tag_unknown.lzi");
    const CACHE_TTL_CONTRACT_SWR_FIXTURE: &str =
        include_str!("../../../tests/fixtures/cache/ttl_contract_swr_exceeds.lzi");

    #[test]
    fn cache_invalidates_target_unresolved_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_INVALIDATES_UNRESOLVED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_invalidates_target_unresolved"),
            "expected cache_invalidates_target_unresolved in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_namespace_collision_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_NAMESPACE_COLLISION_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_namespace_collision"),
            "expected cache_namespace_collision in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_capability_undeclared_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_CAPABILITY_UNDECLARED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_capability_undeclared"),
            "expected cache_capability_undeclared in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_ttl_unit_invalid_fires_on_empty_quoted_prose() {
        // Direct fact injection — the parser does not let an empty
        // quoted ttl through (`parse_cache_ttl` short-circuits on the
        // empty payload), but the doctor rule still guards the
        // typed-promotion path so it stays defensive against future
        // parser changes.
        let mut package = package_from_sources(vec![]);
        let cache = lazuli_ir::QueryCache {
            key: "k".into(),
            ttl: lazuli_ir::CacheTtl::Quoted("".into()),
            tags: Vec::new(),
            namespace: None,
            profile_ref: None,
        };
        let query = lazuli_ir::Query::List(lazuli_ir::ListQuery {
            name: "list".into(),
            public_contract: None,
            params: Vec::new(),
            scope: Vec::new(),
            scope_override: false,
            filters: Vec::new(),
            order: Vec::new(),
            paginate: None,
            modifier: None,
            cache: Some(cache),
            policy: lazuli_ir::PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: Vec::new(),
            span_ref: None,
            owner_scope_sql: None,
        });
        package.tier3_facts.push(Tier3FeatureFacts {
            feature: "customer".into(),
            path: PathBuf::from("x.lzi"),
            feature_line: 1,
            tenancy_axis: None,
            defaults_policy: None,
            defaults_timestamps: false,
            defaults_rate_limit: false,
            defaults_audit: false,
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            resource_previous_names: Vec::new(),
            field_previous_names: Vec::new(),
            all_resource_names_in_feature: BTreeSet::new(),
            all_field_names_in_feature: BTreeMap::new(),
            job_lines: BTreeMap::new(),
            webhook_lines: BTreeMap::new(),
            notification_lines: BTreeMap::new(),
            tenant_migration_lines: BTreeMap::new(),
            event_group_lines: BTreeMap::new(),
            commands: Vec::new(),
            command_lines: BTreeMap::new(),
            queries: vec![query],
            query_lines: BTreeMap::new(),
            caches: Vec::new(),
            cache_lines: BTreeMap::new(),
            api_names_text_pattern: Vec::new(),
            apis: Vec::new(),
            api_lines: BTreeMap::new(),
            agents: Vec::new(),
            translation: None,
            translation_line: 1,
            records: Vec::new(),
            enums: Vec::new(),
            events: Vec::new(),
            policies_declared: false,
            policies: lazuli_ir::Policies::default(),
            extensions: Vec::new(),
            reports: Vec::new(),
            report_lines: BTreeMap::new(),
            resources: Vec::new(),
            report_decls: Vec::new(),
            aggregates: Vec::new(),
            aggregate_lines: BTreeMap::new(),
            errors: None,
            uses: Vec::new(),
            channels: Vec::new(),
        });
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache_ttl_unit_invalid"),
            "expected cache_ttl_unit_invalid in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // -------------------------------------------------------------------------
    // CL.C.3 — feature-level `cache <name>` profile diagnostics:
    // `cache-profile-unknown`, `cache-tag-unknown`, `cache-ttl-contract`.
    // -------------------------------------------------------------------------

    #[test]
    fn cache_profile_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_PROFILE_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache-profile-unknown"),
            "expected cache-profile-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_tag_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_TAG_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache-tag-unknown"),
            "expected cache-tag-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cache_ttl_contract_swr_exceeds_fires() {
        let package = package_from_sources(vec![("x.lzi", CACHE_TTL_CONTRACT_SWR_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cache-ttl-contract"),
            "expected cache-ttl-contract in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

