//! Parser for `workspace.lzi` — the distributed-system contract.
//!
//! `workspace.lzi` is optional. When present it declares the apps that make
//! up a distributed system (`apps`), the cross-app exchanges they perform
//! (`boundaries`), the broker/propagation defaults (`communication`), an
//! optional shared registry (`shared_registry`), and the public-edge
//! gateways with their route table (`gateway <name>` + `route ...`).
//!
//! Per `docs/invariants.md`, this file does NOT promote `workspace.lzi`
//! into mandatory territory: it just translates the surface into the
//! shipped `AppWorkspace` IR. Single-app projects skip the file entirely
//! and the loader never invokes this parser.
//!
//! See: `lazuli_ir::nodes::app_manifest::AppWorkspace`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

use lazuli_ir::{AppWorkspace, WorkspaceCommunication, WorkspaceGateway};

use super::parsers::{
    is_identifier, leading_spaces, parse_workspace_app, parse_workspace_boundary,
    parse_workspace_gateway_route, split_items, unquote, workspace_child,
};

/// Parse a `workspace.lzi` body and lift it into an [`AppWorkspace`].
///
/// Returns `None` only if the source lacks the top-level `workspace
/// <name>` header — single-app projects skip the file entirely and the
/// loader never calls this. Inner state machine handles `apps`,
/// `boundaries`, `communication`, `shared_registry`, and `gateway`
/// blocks (with their `route` children at indent-4 / indent-6).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::app_manifest::workspace::parse_app_workspace;
///
/// let src = "workspace the canonical pilot\n  shared_registry \"acme\"\n";
/// let ws = parse_app_workspace(src).expect("workspace header");
/// assert_eq!(ws.name, "the canonical pilot");
/// assert_eq!(ws.shared_registry.as_deref(), Some("acme"));
/// ```
pub fn parse_app_workspace(source: &str) -> Option<AppWorkspace> {
    let lines: Vec<_> = source.lines().collect();
    let start = lines.iter().position(|line| {
        leading_spaces(line) == 0 && line.trim_start().starts_with("workspace ")
    })?;
    let header = lines[start].trim_start();
    let name = header.split_whitespace().nth(1)?.to_owned();

    let mut workspace = AppWorkspace {
        name,
        apps: Vec::new(),
        shared_registry: None,
        boundaries: Vec::new(),
        communication: None,
        gateways: Vec::new(),
        span_ref: None,
    };
    let mut current_child: Option<&str> = None;
    let mut current_gateway: Option<usize> = None;
    let mut current_gateway_route: Option<usize> = None;

    for line in lines.iter().skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            break;
        }

        match leading_spaces(line) {
            2 => {
                current_gateway = None;
                current_gateway_route = None;
                if let Some(rest) = trimmed.strip_prefix("shared_registry ") {
                    workspace.shared_registry = Some(unquote(rest.trim()).to_owned());
                    current_child = None;
                } else if let Some(name) = trimmed.strip_prefix("gateway ") {
                    if is_identifier(name.trim()) {
                        workspace.gateways.push(WorkspaceGateway {
                            name: name.trim().to_owned(),
                            routes: Vec::new(),
                        });
                        current_gateway = workspace.gateways.len().checked_sub(1);
                        current_child = Some("gateway");
                    } else {
                        current_child = None;
                    }
                } else {
                    current_child = workspace_child(trimmed);
                }
            }
            4 => match current_child {
                Some("apps") => {
                    if let Some(app) = parse_workspace_app(trimmed) {
                        workspace.apps.push(app);
                    }
                }
                Some("boundaries") => {
                    if let Some(boundary) = parse_workspace_boundary(trimmed) {
                        workspace.boundaries.push(boundary);
                    }
                }
                Some("communication") => {
                    let communication = workspace
                        .communication
                        .get_or_insert_with(WorkspaceCommunication::default);
                    let parts: Vec<_> = trimmed.split_whitespace().collect();
                    match parts.as_slice() {
                        ["propagate", rest @ ..] => {
                            communication.propagate.extend(split_items(&rest.join(" ")));
                        }
                        ["default", "sync", rest @ ..] if !rest.is_empty() => {
                            communication.sync_default = Some(rest.join(" "));
                        }
                        ["default", "async", rest @ ..] if !rest.is_empty() => {
                            communication.async_default = Some(rest.join(" "));
                        }
                        _ => {}
                    }
                }
                Some("gateway") => {
                    let Some(gateway_index) = current_gateway else {
                        continue;
                    };
                    if let Some(route) = parse_workspace_gateway_route(trimmed) {
                        workspace.gateways[gateway_index].routes.push(route);
                        current_gateway_route = workspace.gateways[gateway_index]
                            .routes
                            .len()
                            .checked_sub(1);
                    } else {
                        current_gateway_route = None;
                    }
                }
                _ => {}
            },
            6 => {
                if current_child != Some("gateway") {
                    continue;
                }
                let Some(gateway_index) = current_gateway else {
                    continue;
                };
                let Some(route_index) = current_gateway_route else {
                    continue;
                };
                let route = &mut workspace.gateways[gateway_index].routes[route_index];
                if let Some(rest) = trimmed.strip_prefix("auth ") {
                    route.auth = Some(rest.trim().to_owned());
                } else if let Some(rest) = trimmed.strip_prefix("tenant ") {
                    route.tenant = Some(rest.trim().to_owned());
                } else if let Some(rest) = trimmed.strip_prefix("timeout ") {
                    route.timeout = Some(unquote(rest.trim()).to_owned());
                } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
                    route.rate_limit = Some(unquote(rest.trim()).to_owned());
                }
            }
            _ => {}
        }
    }

    Some(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_without_header() {
        assert!(parse_app_workspace("# nothing\n").is_none());
    }

    #[test]
    fn captures_shared_registry() {
        let src = "workspace acme\n  shared_registry \"acme\"\n";
        let ws = parse_app_workspace(src).expect("workspace header");
        assert_eq!(ws.name, "acme");
        assert_eq!(ws.shared_registry.as_deref(), Some("acme"));
    }
}
