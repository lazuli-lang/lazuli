    // Doctor event-trace level + health probe + app_logging canonical tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_rejects_event_trace_level_outside_catalog() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace welcome_email_sent
      level critical
      payload
        email: Text
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_level_invalid_diagnostics"),
            "expected event_trace_level_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_level_on_domain_event() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event customer_created
      level warn
      payload
        id: ID
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_level_on_domain_event_diagnostics"),
            "expected event_trace_level_on_domain_event_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_health_probe_path_without_leading_slash() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  runtime
    unit api
      healthcheck "healthz"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("health_probe_path_invalid_diagnostics"),
            "expected health_probe_path_invalid_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_canonical_health_probes() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  runtime
    unit api
      healthcheck "/healthz"
      readiness "/readyz"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("health_probe_path_invalid_diagnostics"),
            "canonical paths must not fire health probe diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_app_logging_with_canonical_values() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  logging
    level info
    format json
    redact pii
    sample_rate 1.0
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes_set = codes(&diagnostics);
        assert!(
            !codes_set.contains("app_logging_level_invalid_diagnostics"),
            "canonical logging must not fire level diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_format_invalid_diagnostics"),
            "canonical logging must not fire format diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_redact_unknown_diagnostics"),
            "canonical logging must not fire redact diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            !codes_set.contains("app_logging_sample_rate_range_diagnostics"),
            "canonical logging must not fire sample_rate diagnostic; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

