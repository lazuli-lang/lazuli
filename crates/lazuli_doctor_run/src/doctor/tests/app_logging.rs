    // Doctor app urls + CORS env + logging + tracing + audit emit tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;
    use crate::doctor::aggregators::cross_feature::APP_URLS_MISSING_MESSAGE;


    const APP_URLS_MISSING_FIXTURE: &str = "app MyApp\n";

    #[test]
    fn doctor_warns_when_app_urls_missing_or_empty() {
        for source in [APP_URLS_MISSING_FIXTURE, "app MyApp\n  urls\n"] {
            let package = package_from_sources(vec![("app.lzi", source)]);
            let diagnostics = package.diagnostics();
            let diagnostic = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.code == "app_urls_missing")
                .unwrap_or_else(|| {
                    panic!(
                        "expected app_urls_missing; got {:?}",
                        diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
                    )
                });

            assert_eq!(diagnostic.severity, DoctorSeverity::Warning);
            assert_eq!(diagnostic.message, APP_URLS_MISSING_MESSAGE);
        }
    }

    #[test]
    fn doctor_rejects_cors_origin_in_unknown_environment() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  environments
    local
    production

  cors
    allow_origins staging "https://staging.example.com"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("cors_unknown_environment_diagnostics"),
            "expected cors_unknown_environment_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 36 — `app.logging` / `app.tracing`
    // closed catalogs + sample-rate range + exporter binding.

    #[test]
    fn doctor_rejects_app_logging_level_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    level verbose
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_level_invalid_diagnostics"),
            "expected app_logging_level_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_format_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    format yaml
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_format_invalid_diagnostics"),
            "expected app_logging_format_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_redact_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    redact secrets
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_redact_unknown_diagnostics"),
            "expected app_logging_redact_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_logging_sample_rate_above_one() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    sample_rate 2.5
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_logging_sample_rate_range_diagnostics"),
            "expected app_logging_sample_rate_range_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_tracing_sample_rate_below_zero() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  tracing
    sample_rate -0.1
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_tracing_sample_rate_range_diagnostics"),
            "expected app_tracing_sample_rate_range_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_app_tracing_exporter_unbound() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  tracing
    exporter mystery
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_tracing_exporter_unbound_diagnostics"),
            "expected app_tracing_exporter_unbound_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 37 — audit emit_to, event.trace
    // level, and health probe path shape.

    #[test]
    fn doctor_rejects_audit_emit_to_unknown_stream() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    audit actor, target.id
      emit_to nonexistent_stream
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("audit_emit_to_unknown_diagnostics"),
            "expected audit_emit_to_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_audit_emit_to_reserved_audit_log() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  command archive
    audit actor, target.id
      emit_to audit_log
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("audit_emit_to_unknown_diagnostics"),
            "reserved stream `audit_log` must resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_audit_emit_to_authored_event_group() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  event_group customer_audit *
  command archive
    audit actor, target.id
      emit_to customer_audit
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("audit_emit_to_unknown_diagnostics"),
            "authored event_group must resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // -------------------------------------------------------------------------
    // SCOPE-OWNER-COLUMN-001 — warn when @scope.owner / @scope.same_org
    // policy is declared but the targeted resource has no matching column.
    // Mirrors the codegen-side silent-skip so authors see the gap at design
    // time. (the canonical pilot 2026-05-17 evidence.)
    // -------------------------------------------------------------------------

