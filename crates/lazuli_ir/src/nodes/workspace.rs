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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceApp {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceBoundary {
    pub app: String,
    pub direction: String,
    pub pattern: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceCommunication {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub propagate: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_default: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub async_default: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceGateway {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<WorkspaceGatewayRoute>,
}

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
