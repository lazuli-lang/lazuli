    // Doctor observability + deprecated replacement / sunset tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_observability_source_001_fires_on_unknown_token() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app crm
  observability
    error_source dev,qa
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("OBSERVABILITY-SOURCE-001"),
            "expected OBSERVABILITY-SOURCE-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_observability_panic_001_warns_when_recover_disabled_outside_dev() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app crm
  environments
    prod
  observability
    panic_recover false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("OBSERVABILITY-PANIC-001"),
            "expected OBSERVABILITY-PANIC-001 in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // OpenAPI bucket cycle (row 48) — deprecation diagnostics on
    // `Command.deprecated` / `Api.deprecated` typed lifts.
    // =========================================================================

    const OPENAPI_REPLACEMENT_UNKNOWN_FIXTURE: &str =
        include_str!("../../../tests/fixtures/openapi/deprecated_replacement_unknown.lzi");
    const OPENAPI_SUNSET_DATE_INVALID_FIXTURE: &str =
        include_str!("../../../tests/fixtures/openapi/deprecated_sunset_date_invalid.lzi");
    const OPENAPI_SUNSET_IN_PAST_FIXTURE: &str =
        include_str!("../../../tests/fixtures/openapi/deprecated_sunset_in_past.lzi");
    const OPENAPI_TEXT_PATTERN_API_FIXTURE: &str =
        include_str!("../../../tests/fixtures/openapi/text_pattern_api_block.lzi");

    #[test]
    fn deprecated_replacement_unknown_fires() {
        let package = package_from_sources(vec![("x.lzi", OPENAPI_REPLACEMENT_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-replacement-unknown"),
            "expected deprecated-replacement-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_sunset_date_invalid_fires() {
        let package = package_from_sources(vec![("x.lzi", OPENAPI_SUNSET_DATE_INVALID_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated_sunset_date_invalid"),
            "expected deprecated_sunset_date_invalid in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_sunset_in_past_fires() {
        let package = package_from_sources(vec![("x.lzi", OPENAPI_SUNSET_IN_PAST_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-sunset-past"),
            "expected deprecated-sunset-past in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_no_replacement_fires_for_command() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  command legacy_update
    policy @policy.update
    deprecated
    creates Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-no-replacement"),
            "expected deprecated-no-replacement in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_no_replacement_fires_for_api() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-no-replacement"),
            "expected deprecated-no-replacement in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_no_replacement_skips_when_replacement_resolves() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  command legacy_update
    policy @policy.update
    deprecated replacement command.update_v2
    creates Customer

  command update_v2
    policy @policy.update
    creates Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("deprecated-no-replacement"),
            "did not expect deprecated-no-replacement in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn api_deprecated_replacement_unknown_fires() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated
      replacement api.export_v2
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-replacement-unknown"),
            "expected deprecated-replacement-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn deprecated_replacement_unknown_fires_for_cross_feature_api() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated replacement billing.api.export_v2
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("deprecated-replacement-unknown"),
            "expected deprecated-replacement-unknown in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn api_deprecated_sunset_past_fires_info() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated
      replacement api.export_v2
      sunset "2024-01-01"

  api export_v2
    method GET
    path "/api/customers/export-v2"
    output [Customer]
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "deprecated-sunset-past")
            .collect();
        assert_eq!(hits.len(), 1, "diagnostics: {diagnostics:?}");
        assert_eq!(hits[0].severity, DoctorSeverity::Info);
    }

    #[test]
    fn deprecated_sunset_future_does_not_fire() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  command legacy_update
    policy @policy.update
    deprecated
      replacement command.update_v2
      sunset "2027-01-01"
    creates Customer

  command update_v2
    policy @policy.update
    creates Customer
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("deprecated-sunset-past"),
            "did not expect deprecated-sunset-past in {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =========================================================================
    // i18n bucket cycle (row 54) — 5 critical doctor diagnostics anchored
    // on `app.locale` / `Translation` / `LocaleNegotiate` IR. The full
    // 15-diagnostic catalog (`translation_locale_*`, `rule_message_ref_*`,
    // `locale_negotiate_*`, `app_locale_*`, `cldr_plural_arm_invalid`)
    // is covered by the `i18n_diagnostics` walk; this set exercises the
    // top-5 most-likely authoring mistakes from the proposal.
    // =========================================================================
