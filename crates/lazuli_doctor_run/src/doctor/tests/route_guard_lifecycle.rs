    // Doctor route-guard + lifecycle-gate + auth-refresh rule tests
    // Split from crates/lazuli_cli/src/doctor/tests.rs.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    const ROUTE_GUARD_HAPPY_LZI: &str = include_str!("../../../tests/fixtures/route-guard/happy.lzi");
    const ROUTE_GUARD_HAPPY_LZX: &str = include_str!("../../../tests/fixtures/route-guard/happy.lzx");
    const ROUTE_GUARD_UNGUARDED_LZX: &str =
        include_str!("../../../tests/fixtures/route-guard/view_unguarded_with_gated_backend.lzx");
    const ROUTE_GUARD_LAXER_LZX: &str =
        include_str!("../../../tests/fixtures/route-guard/view_laxer_than_backend.lzx");
    const ROUTE_GUARD_REDIRECT_LZX: &str =
        include_str!("../../../tests/fixtures/route-guard/redirect_unreachable.lzx");
    const ROUTE_GUARD_MISSING_ACTOR_LZI: &str =
        include_str!("../../../tests/fixtures/route-guard/missing_actor_query.lzi");
    const ROUTE_GUARD_MISSING_ACTOR_LZX: &str =
        include_str!("../../../tests/fixtures/route-guard/missing_actor_query.lzx");
    const ROUTE_GUARD_AUDIENCE_LZX: &str =
        include_str!("../../../tests/fixtures/route-guard/audience_runtime_disagreement.lzx");
    const LIFECYCLE_GATE_HAPPY_LZI: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/happy.lzi");
    const LIFECYCLE_GATE_HAPPY_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/happy.lzx");
    const LIFECYCLE_GATE_UNKNOWN_RESOURCE_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/unknown_resource.lzx");
    const LIFECYCLE_GATE_UNKNOWN_STATE_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/unknown_state.lzx");
    const LIFECYCLE_GATE_MISSING_STATE_COVERAGE_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/missing_state_coverage.lzx");
    const LIFECYCLE_GATE_EXTRA_STATE_ARM_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/extra_state_arm.lzx");
    const LIFECYCLE_GATE_WILDCARD_OVERUSE_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/wildcard_overuse.lzx");
    const LIFECYCLE_GATE_REDIRECT_CYCLE_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/redirect_cycle.lzx");
    const LIFECYCLE_GATE_RESUME_RESOURCE_MISMATCH_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/resume_resource_mismatch.lzx");
    const LIFECYCLE_GATE_WRONG_QUERY_KIND_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/wrong_query_kind.lzx");
    const LIFECYCLE_GATE_WITHOUT_ACTOR_GATE_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/lifecycle_without_actor_gate.lzx");
    const LIFECYCLE_GATE_CROSS_FEATURE_LZX: &str =
        include_str!("../../../tests/fixtures/lifecycle-gate/cross_feature_resume.lzx");

    fn route_guard_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("ROUTE-GUARD-"))
            .collect()
    }

    fn lifecycle_gate_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("LIFECYCLE-GATE-"))
            .collect()
    }

    fn route_guard_fixture(lzx: &str) -> DoctorPackage {
        package_from_sources(vec![
            ("route_guard.lzi", ROUTE_GUARD_HAPPY_LZI),
            ("route_guard.lzx", lzx),
        ])
    }

    fn lifecycle_gate_fixture(lzx: &str) -> DoctorPackage {
        package_from_sources(vec![
            ("lifecycle_gate.lzi", LIFECYCLE_GATE_HAPPY_LZI),
            ("lifecycle_gate.lzx", lzx),
        ])
    }


    const AUTH_REFRESH_HAPPY: &str = include_str!("../../../tests/fixtures/auth-refresh/happy.lzi");
    const AUTH_REFRESH_001: &str =
        include_str!("../../../tests/fixtures/auth-refresh/missing_secret_provider.lzi");
    const AUTH_REFRESH_002: &str =
        include_str!("../../../tests/fixtures/auth-refresh/grace_exceeds_refresh_ttl.lzi");
    const AUTH_REFRESH_003: &str =
        include_str!("../../../tests/fixtures/auth-refresh/schema_missing_columns.lzi");
    const AUTH_REFRESH_004: &str =
        include_str!("../../../tests/fixtures/auth-refresh/revoke_user_missing_user_fk.lzi");
    const AUTH_REFRESH_005: &str =
        include_str!("../../../tests/fixtures/auth-refresh/refresh_ttl_long.lzi");
    const AUTH_REFRESH_006: &str =
        include_str!("../../../tests/fixtures/auth-refresh/missing_on_refresh_failure.lzi");
    const AUTH_REFRESH_007: &str =
        include_str!("../../../tests/fixtures/auth-refresh/auto_promotion_applied.lzi");
    const AUTH_REFRESH_008: &str =
        include_str!("../../../tests/fixtures/auth-refresh/auto_refresh_not_surfaced.lzi");
    const AUTH_REFRESH_009: &str =
        include_str!("../../../tests/fixtures/auth-refresh/cookie_domain_missing.lzi");

    fn auth_refresh_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("AUTH-REFRESH-"))
            .collect()
    }

    fn assert_auth_refresh_fixture(source: &str, expected_code: &str) -> Vec<DoctorDiagnostic> {
        let package = package_from_sources(vec![("auth_refresh.lzi", source)]);
        let diagnostics = package.diagnostics();
        let auth_refresh = auth_refresh_diags(&diagnostics);
        assert_eq!(
            auth_refresh.len(),
            1,
            "expected exactly one AUTH-REFRESH diagnostic ({expected_code}); got {:?}",
            auth_refresh
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
        assert_eq!(auth_refresh[0].code, expected_code);
        diagnostics
    }

    #[test]
    fn route_guard_happy_fixture_fires_no_route_guard_diagnostics() {
        let package = package_from_sources(vec![
            ("happy.lzi", ROUTE_GUARD_HAPPY_LZI),
            ("happy.lzx", ROUTE_GUARD_HAPPY_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert!(
            route_guard.is_empty(),
            "happy route guard fixtures must emit zero ROUTE-GUARD-* diagnostics; got: {:?}",
            route_guard
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn route_guard_001_fires_for_unguarded_gated_backend_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_UNGUARDED_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-001"]
        );
    }

    #[test]
    fn route_guard_002_fires_for_laxer_view_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_LAXER_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-002"]
        );
    }

    #[test]
    fn route_guard_003_fires_for_unreachable_redirect_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_REDIRECT_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-003"]
        );
    }

    #[test]
    fn route_guard_004_fires_as_warning_for_missing_actor_query_fixture() {
        let package = package_from_sources(vec![
            ("missing_actor_query.lzi", ROUTE_GUARD_MISSING_ACTOR_LZI),
            ("missing_actor_query.lzx", ROUTE_GUARD_MISSING_ACTOR_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert_eq!(route_guard.len(), 1, "got {route_guard:?}");
        assert_eq!(route_guard[0].code, "ROUTE-GUARD-004");
        assert_eq!(route_guard[0].severity, DoctorSeverity::Warning);
    }

    #[test]
    fn route_guard_005_fires_as_info_for_runtime_audience_disagreement_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_AUDIENCE_LZX);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert_eq!(route_guard.len(), 1, "got {route_guard:?}");
        assert_eq!(route_guard[0].code, "ROUTE-GUARD-005");
        assert_eq!(route_guard[0].severity, DoctorSeverity::Info);
    }

    #[test]
    fn lifecycle_gate_happy_fixture_fires_no_lifecycle_gate_diagnostics() {
        let package = package_from_sources(vec![
            ("happy.lzi", LIFECYCLE_GATE_HAPPY_LZI),
            ("happy.lzx", LIFECYCLE_GATE_HAPPY_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
        assert!(
            lifecycle_gate.is_empty(),
            "happy lifecycle gate fixtures must emit zero LIFECYCLE-GATE-* diagnostics; got: {:?}",
            lifecycle_gate
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn lifecycle_gate_fixtures_emit_exactly_the_documented_code() {
        for (source, expected) in [
            (LIFECYCLE_GATE_UNKNOWN_RESOURCE_LZX, "LIFECYCLE-GATE-001"),
            (LIFECYCLE_GATE_UNKNOWN_STATE_LZX, "LIFECYCLE-GATE-002"),
            (
                LIFECYCLE_GATE_MISSING_STATE_COVERAGE_LZX,
                "LIFECYCLE-GATE-003",
            ),
            (LIFECYCLE_GATE_EXTRA_STATE_ARM_LZX, "LIFECYCLE-GATE-004"),
            (LIFECYCLE_GATE_WILDCARD_OVERUSE_LZX, "LIFECYCLE-GATE-005"),
            (LIFECYCLE_GATE_REDIRECT_CYCLE_LZX, "LIFECYCLE-GATE-006"),
            (
                LIFECYCLE_GATE_RESUME_RESOURCE_MISMATCH_LZX,
                "LIFECYCLE-GATE-007",
            ),
            (LIFECYCLE_GATE_WRONG_QUERY_KIND_LZX, "LIFECYCLE-GATE-008"),
            (LIFECYCLE_GATE_WITHOUT_ACTOR_GATE_LZX, "LIFECYCLE-GATE-009"),
        ] {
            let package = lifecycle_gate_fixture(source);
            let diagnostics = package.diagnostics();
            let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
            assert_eq!(
                lifecycle_gate
                    .iter()
                    .map(|d| d.code.as_str())
                    .collect::<Vec<_>>(),
                vec![expected],
                "expected exactly {expected}; got {:?}",
                lifecycle_gate
                    .iter()
                    .map(|d| (&d.code, &d.message))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn lifecycle_gate_cross_feature_resume_resolves_through_uses() {
        let package = lifecycle_gate_fixture(LIFECYCLE_GATE_CROSS_FEATURE_LZX);
        let diagnostics = package.diagnostics();
        let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
        assert!(
            lifecycle_gate.is_empty(),
            "qualified @resume account.account_onboarding must resolve through host.uses account; got {:?}",
            lifecycle_gate
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn auth_refresh_happy_fixture_has_zero_diagnostics() {
        let package = package_from_sources(vec![("auth_refresh.lzi", AUTH_REFRESH_HAPPY)]);
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                // `VOCAB-*` rules are vocabulary-fitness lints
                // (aspirational, not correctness errors). The vocab
                // wiring follow-up (2026-05-27) closed the deferred
                // dispatch cell from
                // `docs/proposals/doctor-vocabulary-lints.md`, so the
                // auth_refresh happy fixture now legitimately surfaces
                // VOCAB-DERIVED-READ-001 against fields the auth
                // system populates outside user commands. Filter the
                // whole vocabulary family so this test keeps its
                // "happy = zero correctness diagnostics" contract.
                !d.code.starts_with("VOCAB-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            diagnostics.is_empty(),
            "happy auth-refresh fixture must emit zero diagnostics; got {:?}",
            diagnostics
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn auth_refresh_fixtures_trigger_exact_codes() {
        for (source, code) in [
            (AUTH_REFRESH_001, "AUTH-REFRESH-001"),
            (AUTH_REFRESH_002, "AUTH-REFRESH-002"),
            (AUTH_REFRESH_003, "AUTH-REFRESH-003"),
            (AUTH_REFRESH_004, "AUTH-REFRESH-004"),
            (AUTH_REFRESH_005, "AUTH-REFRESH-005"),
            (AUTH_REFRESH_006, "AUTH-REFRESH-006"),
            (AUTH_REFRESH_007, "AUTH-REFRESH-007"),
            (AUTH_REFRESH_008, "AUTH-REFRESH-008"),
            (AUTH_REFRESH_009, "AUTH-REFRESH-009"),
        ] {
            assert_auth_refresh_fixture(source, code);
        }
    }

    #[test]
    fn auth_refresh_003_fires_for_incomplete_column_set() {
        let diagnostics = assert_auth_refresh_fixture(AUTH_REFRESH_003, "AUTH-REFRESH-003");
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "AUTH-REFRESH-003")
            .expect("AUTH-REFRESH-003 present");
        assert!(
            diag.message.contains("parent_session_id"),
            "missing-column message should name the incomplete column set: {}",
            diag.message
        );
    }

    #[test]
    fn auth_refresh_007_message_surfaces_resolved_defaults() {
        let diagnostics = assert_auth_refresh_fixture(AUTH_REFRESH_007, "AUTH-REFRESH-007");
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "AUTH-REFRESH-007")
            .expect("AUTH-REFRESH-007 present");
        assert!(diag.message.contains("refresh_ttl 14d"), "{}", diag.message);
        assert!(
            diag.message.contains("rotation_grace 1m"),
            "{}",
            diag.message
        );
        assert!(
            diag.message
                .contains("theft_detection_action revoke_session_family"),
            "{}",
            diag.message
        );
    }

    #[test]
    fn auth_refresh_info_diagnostics_are_non_blocking() {
        for (source, code) in [
            (AUTH_REFRESH_006, "AUTH-REFRESH-006"),
            (AUTH_REFRESH_007, "AUTH-REFRESH-007"),
            (AUTH_REFRESH_008, "AUTH-REFRESH-008"),
            (AUTH_REFRESH_009, "AUTH-REFRESH-009"),
        ] {
            let diagnostics = assert_auth_refresh_fixture(source, code);
            let diag = diagnostics
                .iter()
                .find(|d| d.code == code)
                .expect("diagnostic present");
            assert_eq!(diag.severity, DoctorSeverity::Info, "{code}");
            assert!(
                diagnostics
                    .iter()
                    .all(|d| d.severity != DoctorSeverity::Error),
                "{code} fixture should not contain error-severity diagnostics"
            );
        }
    }
