    // Doctor authored-event trace + subscriber + trigger-trace namespace tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_rejects_authored_event_trace_agent_run() {
        // `event.trace agent_run` is reserved by the IR — authoring
        // it as a domain event must fail with the reserved-name
        // diagnostic.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace agent_run
      payload
        agent_id: ID
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_subscriber_referencing_unknown_payload_field() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job aggregate_costs
    trigger event.trace agent_run
      fictional_field = payload.fictional_field
      cost_usd = payload.cost_usd
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("agent_run_subscriber_payload_drift_diagnostics"),
            "expected agent_run_subscriber_payload_drift_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_subscriber_with_canonical_fields_only() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job aggregate_costs
    trigger event.trace agent_run
      cost_usd = payload.cost_usd
      tokens_total = payload.tokens_total
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("agent_run_subscriber_payload_drift_diagnostics"),
            "canonical fields must not drift; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 35 — the 3 new reserved trace
    // event names (`command_run`, `job_run`, `webhook_run`) must
    // reuse the same `event_trace_reserved_name_diagnostics` path as
    // the Cut A.8 `agent_run` case. Authoring any of them is rejected.

    #[test]
    fn doctor_rejects_authored_event_trace_command_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace command_run
      payload
        cmd: Text
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for command_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_authored_event_trace_job_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace job_run
      payload
        job_id: ID
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for job_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_authored_event_trace_webhook_run() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace webhook_run
      payload
        url: Text
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("event_trace_reserved_name_diagnostics"),
            "expected event_trace_reserved_name_diagnostics for webhook_run; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Observability bucket cycle row 35 — `trigger_trace_unknown`. A
    // subscriber referencing `@trace.<name>` or
    // `trigger event.trace <name>` must resolve to a built-in trace
    // event or an authored `event.trace <name>` in the same file.

    #[test]
    fn doctor_rejects_trigger_trace_unknown_namespace_form() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job dangling
    trigger @trace.fictional_event
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "expected trigger_trace_unknown_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_trigger_trace_namespace_for_built_in() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  job collect
    trigger @trace.agent_run
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "built-in @trace.agent_run must resolve; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_trigger_trace_namespace_for_authored_event() {
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    event.trace customer_authored
      payload
        id: ID
  job collect
    trigger @trace.customer_authored
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("trigger_trace_unknown_diagnostics"),
            "authored event.trace in same file must satisfy @trace.<name>; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

