//! Tests for `parse_app_contracts` — external contract files declaring
//! records, operations, and events for cross-system interop. Lives
//! alongside `contracts.rs`.

#![cfg(test)]

use super::parse_app_contracts;

#[test]
fn parses_external_contract() {
    let source = r#"
contract acme.ai.v1
  purpose "AI inference service."
  compatibility backward
  import openapi "./contracts/ai.openapi.json"

  record CustomerSummaryRequest
    customer_id: ID required
    email: @semantic.Email @pii.contact optional

  record CustomerSummaryResult
    summary: Text required
    generated_at: DateTime required

  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    auth service
    timeout "10s"

  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
      summary: Text required
"#;

    let contracts = parse_app_contracts(source);
    let contract = &contracts[0];

    assert_eq!(contract.name, "acme.ai.v1");
    assert_eq!(contract.purpose.as_deref(), Some("AI inference service."));
    assert_eq!(contract.compatibility.as_deref(), Some("backward"));
    assert_eq!(contract.imports[0].format, "openapi");
    assert_eq!(contract.records[0].name, "CustomerSummaryRequest");
    assert_eq!(contract.records[0].fields[1].type_name, "@semantic.Email");
    assert_eq!(contract.records[0].fields[1].markers, ["@pii.contact"]);
    assert_eq!(contract.operations[0].transport.as_deref(), Some("http"));
    assert_eq!(
        contract.operations[0].path.as_deref(),
        Some("/v1/customer-summary")
    );
    assert_eq!(
        contract.events[0].topic.as_deref(),
        Some("ai.summary_ready")
    );
    assert_eq!(contract.events[0].payload[0].name, "customer_id");
}
