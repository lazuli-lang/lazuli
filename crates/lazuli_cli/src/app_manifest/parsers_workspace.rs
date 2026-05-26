//! Workspace-specific line-level parsers for `parse_app_workspace`.
//! Each helper is `pub(super)` and re-exported from `parsers.rs` so
//! existing `use super::parsers::*` import sites keep compiling
//! verbatim.

use lazuli_ir::{WorkspaceApp, WorkspaceBoundary, WorkspaceGatewayRoute};

use super::parsers_common::{is_identifier, parse_quoted_prefix, unquote};

pub(super) fn workspace_child(trimmed: &str) -> Option<&'static str> {
    match trimmed.split_whitespace().next()? {
        "apps" => Some("apps"),
        "boundaries" => Some("boundaries"),
        "communication" => Some("communication"),
        _ => None,
    }
}

pub(super) fn parse_workspace_app(trimmed: &str) -> Option<WorkspaceApp> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        [name, "at", path] if is_identifier(name) => Some(WorkspaceApp {
            name: (*name).to_owned(),
            kind: "local".to_owned(),
            path: Some(unquote(path).to_owned()),
            contract: None,
        }),
        [name, "external", "contract", contract] if is_identifier(name) => Some(WorkspaceApp {
            name: (*name).to_owned(),
            kind: "external".to_owned(),
            path: None,
            contract: Some(unquote(contract).to_owned()),
        }),
        _ => None,
    }
}

pub(super) fn parse_workspace_boundary(trimmed: &str) -> Option<WorkspaceBoundary> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        [app, direction, pattern]
            if is_identifier(app) && matches!(*direction, "publishes" | "consumes") =>
        {
            Some(WorkspaceBoundary {
                app: (*app).to_owned(),
                direction: (*direction).to_owned(),
                pattern: (*pattern).to_owned(),
            })
        }
        _ => None,
    }
}

pub(super) fn parse_workspace_gateway_route(trimmed: &str) -> Option<WorkspaceGatewayRoute> {
    let rest = trimmed.strip_prefix("route ")?;
    let (path, tail) = parse_quoted_prefix(rest.trim())?;
    let parts: Vec<_> = tail.split_whitespace().collect();
    match parts.as_slice() {
        ["to", target_kind, target] if is_identifier(target) => Some(WorkspaceGatewayRoute {
            path,
            target_kind: (*target_kind).to_owned(),
            target: (*target).to_owned(),
            auth: None,
            tenant: None,
            timeout: None,
            rate_limit: None,
        }),
        _ => None,
    }
}
