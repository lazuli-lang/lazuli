//! `workspace.lzi` IR — the distributed-system contract.
//!
//! `workspace.lzi` is optional; a single-app project never authors
//! one. When present it names the apps in the system, declares their
//! boundaries (allowed events, RPC patterns), wires shared synchronous
//! and asynchronous propagation defaults, and pins gateway routes
//! that bridge apps to the outside world.
//!
//! Pure data — no runtime / transport mechanics live here. The
//! runtime adapter (Lazuli Go) translates `AppWorkspace.boundaries`
//! into authorization rules and `gateway.routes` into proxy /
//! transport / rate-limit configuration.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// Root IR node for a `workspace.lzi` declaration — names the apps
/// in a distributed system and pins their boundaries + shared
/// gateway routing. Optional: single-app projects never author one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppWorkspace {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<WorkspaceApp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_registry: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<WorkspaceBoundary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub communication: Option<WorkspaceCommunication>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gateways: Vec<WorkspaceGateway>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One member app inside an [`AppWorkspace`]. Carries the app's name,
/// kind (`service` / `frontend` / `worker` / etc.), its on-disk path,
/// and the named contract it exposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceApp {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

/// One boundary entry declaring an allowed direction + pattern between
/// an app and the rest of the workspace. The runtime denies any
/// boundary crossing not declared here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBoundary {
    pub app: String,
    pub direction: String,
    pub pattern: String,
}

/// Shared workspace-level defaults for synchronous + asynchronous
/// inter-app communication. Individual apps may override via their
/// own `communication { ... }` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceCommunication {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub propagate: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub async_default: Option<String>,
}

/// One named gateway declaration — the workspace-level edge that
/// bridges external HTTP clients to internal apps. Carries the
/// gateway-scoped routes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceGateway {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<WorkspaceGatewayRoute>,
}

/// One route inside a [`WorkspaceGateway`]. Pins the external path,
/// the target app/feature, and the auth/tenancy/timeout/rate-limit
/// decorators the gateway applies at the edge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceGatewayRoute {
    pub path: String,
    pub target_kind: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_workspace_round_trips_minimal() {
        let w = AppWorkspace {
            name: "main".into(),
            apps: vec![],
            shared_registry: None,
            boundaries: vec![],
            communication: None,
            gateways: vec![],
            span_ref: None,
        };
        let s = serde_json::to_string(&w).unwrap();
        let back: AppWorkspace = serde_json::from_str(&s).unwrap();
        assert_eq!(w, back);
    }
}
