    // Doctor ownership + workspace + external-contracts tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_reports_app_service_ownership_gaps() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    customer
    billing
  targets
    backend go
  services
    service crm
      owns customer
      exposes
        query billing.query.invoice_by_id

    service finance
      owns customer, billing
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
"#,
            ),
            (
                "billing.lzi",
                r#"
feature billing
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "APP-SVC-001"
                && diagnostic
                    .message
                    .contains("feature `customer` is owned by multiple app services")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "APP-SVC-003"
                && diagnostic
                    .message
                    .contains("service `crm` exposes `billing.query.invoice_by_id`")
        }));
    }

    #[test]
    fn doctor_validates_workspace_contract_edges() {
        let valid = package_from_sources(vec![(
            "workspace.lzi",
            r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"
  boundaries
    crm publishes customer.*
    ai consumes customer.*
  communication
    propagate actor, tenant, trace_id, request_id
    default sync internal rpc
    default async event_bus
  gateway public_api
    route "/api/customers/*" to app crm
      auth propagate
      tenant propagate
"#,
        )]);

        let leftover: Vec<_> = valid
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected valid workspace contract to pass doctor: {:#?}",
            leftover
        );

        let invalid = package_from_sources(vec![(
            "workspace.lzi",
            r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
  boundaries
    ai consumes ai.*
  communication
    propagate actor
  gateway public_api
    route "/api/ai/*" to app ai
"#,
        )]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("WS-BOUNDARY-001"));
        assert!(codes.contains("WS-EVENT-001"));
        assert!(codes.contains("WS-GW-002"));
        assert!(codes.contains("WS-GW-003"));
        assert!(codes.contains("WS-GW-004"));
        assert!(codes.contains("WS-COMM-001"));
    }

    #[test]
    fn doctor_validates_external_contracts() {
        let valid = package_from_sources(vec![
            (
                "workspace.lzi",
                r#"
workspace AcmeERP
  apps
    ai external contract "acme.ai.v1"
"#,
            ),
            (
                "contracts/ai.lzi",
                r#"
contract acme.ai.v1
  import openapi "./contracts/ai.openapi.json"
  record CustomerSummaryRequest
    customer_id: ID required
  record CustomerSummaryResult
    summary: Text required
  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    timeout "10s"
  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
"#,
            ),
        ]);

        let leftover: Vec<_> = valid
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            leftover.is_empty(),
            "expected external contract to pass doctor: {:#?}",
            leftover
        );

        let invalid = package_from_sources(vec![
            (
                "workspace.lzi",
                r#"
workspace AcmeERP
  apps
    ai external contract "acme.ai.v2"
"#,
            ),
            (
                "contracts/ai.lzi",
                r#"
contract acme.ai.v1
  operation summarize_customer
    transport http
"#,
            ),
        ]);

        let diagnostics = invalid.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("CONTRACT-OP-002"));
        assert!(codes.contains("CONTRACT-OP-003"));
        assert!(codes.contains("CONTRACT-OP-004"));
        assert!(codes.contains("WS-CONTRACT-001"));
    }

    // -------------------------------------------------------------------------
    // Cut A — cross-feature diagnostics (§5.3 snapshot pattern)
    // -------------------------------------------------------------------------

    // `codes` and `temp_project` helpers live in `tests/test_support_core.rs`
    // and are re-exported via the `use test_support_core::*;` at the top of
    // this module.

    // Z2 — `design-custom-*` doctor integration (3 rules over `design.lzi` IR).
