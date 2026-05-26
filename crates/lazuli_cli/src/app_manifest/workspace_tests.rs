//! Tests for `parse_app_workspace` — multi-app workspace contracts with
//! gateways, shared registries, and cross-app boundaries. Lives
//! alongside `workspace.rs`.

#![cfg(test)]

use super::parse_app_workspace;

#[test]
fn parses_workspace_contract() {
    let source = r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"
  shared_registry "./registry.lzi"
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
      timeout "5s"
"#;

    let workspace = parse_app_workspace(source).unwrap();

    assert_eq!(workspace.name, "AcmeERP");
    assert_eq!(workspace.apps[0].name, "crm");
    assert_eq!(workspace.apps[0].kind, "local");
    assert_eq!(
        workspace.apps[0].path.as_deref(),
        Some("./apps/crm/app.lzi")
    );
    assert_eq!(workspace.apps[1].name, "ai");
    assert_eq!(workspace.apps[1].kind, "external");
    assert_eq!(workspace.apps[1].contract.as_deref(), Some("acme.ai.v1"));
    assert_eq!(workspace.shared_registry.as_deref(), Some("./registry.lzi"));
    assert_eq!(workspace.boundaries[0].direction, "publishes");
    assert_eq!(
        workspace
            .communication
            .as_ref()
            .and_then(|communication| communication.sync_default.as_deref()),
        Some("internal rpc")
    );
    assert_eq!(workspace.gateways[0].name, "public_api");
    assert_eq!(workspace.gateways[0].routes[0].path, "/api/customers/*");
    assert_eq!(workspace.gateways[0].routes[0].target, "crm");
    assert_eq!(
        workspace.gateways[0].routes[0].auth.as_deref(),
        Some("propagate")
    );
}
