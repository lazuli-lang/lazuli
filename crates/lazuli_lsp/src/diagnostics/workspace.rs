//! Diagnostics for the `workspace` family.
//!
//! A `workspace <Name>` contract is the distributed-system header: it
//! declares the participating apps (`apps`), allowed event publication /
//! consumption (`boundaries`), default sync/async transport
//! (`communication`), the shared registry (`shared_registry`), and any
//! `gateway <name>` blocks that route external traffic to specific apps.
//!
//! `workspace.lzi` remains optional — single-app projects never need it —
//! and the rules here enforce only what the canonical shape requires when
//! a file does declare one.
//!
//! ## Producer
//!
//! [`workspace_contract_diagnostics`] is the single entry-point dispatched
//! from `crate::dispatch`. The `validate_workspace_*` helpers and
//! [`quoted_prefix`] are module-private — re-exported only so any future
//! cross-catalog caller keeps its `crate::*` path.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    is_identifier, is_quoted_lzx_literal, is_type_name, leading_spaces,
    simple_canonical_diagnostic, split_items,
};

pub(crate) fn workspace_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_workspace = false;
    let mut current_child: Option<&'static str> = None;
    let mut in_gateway = false;
    let mut in_gateway_route = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            in_workspace = trimmed.starts_with("workspace ");
            current_child = None;
            in_gateway = false;
            in_gateway_route = false;
            if in_workspace {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() != 2 || !is_type_name(parts[1]) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "workspace-contract",
                        "workspace contracts use `workspace <Name>` as the distributed-system header.",
                    ));
                }
            }
            continue;
        }

        if !in_workspace {
            continue;
        }

        match leading {
            2 => {
                in_gateway = false;
                in_gateway_route = false;
                if let Some(rest) = trimmed.strip_prefix("shared_registry ") {
                    current_child = None;
                    if !is_quoted_lzx_literal(rest.trim()) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "workspace-registry-contract",
                            "shared registries use `shared_registry \"./registry.lzi\"`.",
                        ));
                    }
                } else if let Some(name) = trimmed.strip_prefix("gateway ") {
                    current_child = Some("gateway");
                    in_gateway = true;
                    if !is_identifier(name.trim()) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "workspace-gateway-contract",
                            "workspace gateways use `gateway <name>`.",
                        ));
                    }
                } else {
                    current_child = match trimmed {
                        "apps" => Some("apps"),
                        "boundaries" => Some("boundaries"),
                        "communication" => Some("communication"),
                        _ => {
                            diagnostics.push(simple_canonical_diagnostic(
                                line_index,
                                line,
                                DiagnosticSeverity::WARNING,
                                "workspace-contract",
                                "workspace blocks use `apps`, `shared_registry`, `boundaries`, `communication`, and `gateway <name>`.",
                            ));
                            None
                        }
                    };
                }
            }
            4 => match current_child {
                Some("apps") => validate_workspace_app_line(&mut diagnostics, line_index, line),
                Some("boundaries") => {
                    validate_workspace_boundary_line(&mut diagnostics, line_index, line)
                }
                Some("communication") => {
                    validate_workspace_communication_line(&mut diagnostics, line_index, line)
                }
                Some("gateway") if in_gateway => {
                    in_gateway_route =
                        validate_workspace_gateway_route_line(&mut diagnostics, line_index, line);
                }
                _ => diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "workspace-contract",
                    "four-space workspace declarations must belong to `apps`, `boundaries`, `communication`, or a `gateway` route.",
                )),
            },
            6 => {
                if in_gateway && in_gateway_route {
                    validate_workspace_gateway_route_child(
                        &mut diagnostics,
                        line_index,
                        line,
                        trimmed,
                    );
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "workspace-gateway-contract",
                        "six-space workspace declarations are only valid under a gateway route.",
                    ));
                }
            }
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "workspace-contract",
                "workspace declarations use two, four, or six spaces of indentation.",
            )),
        }
    }

    diagnostics
}

pub(crate) fn validate_workspace_app_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let valid = matches!(
        parts.as_slice(),
        [name, "at", path] if is_identifier(name) && is_quoted_lzx_literal(path)
    ) || matches!(
        parts.as_slice(),
        [name, "external", "contract", contract]
            if is_identifier(name) && is_quoted_lzx_literal(contract)
    );

    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-app-contract",
            "workspace apps use `<name> at \"./app.lzi\"` or `<name> external contract \"name.version\"`.",
        ));
    }
}

pub(crate) fn validate_workspace_boundary_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if !matches!(
        parts.as_slice(),
        [app, direction, pattern]
            if is_identifier(app) && matches!(*direction, "publishes" | "consumes") && !pattern.is_empty()
    ) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-boundary-contract",
            "workspace boundaries use `<app> publishes <event_pattern>` or `<app> consumes <event_pattern>`.",
        ));
    }
}

pub(crate) fn validate_workspace_communication_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let valid = matches!(
        parts.as_slice(),
        ["propagate", rest @ ..]
            if !rest.is_empty()
                && split_items(&rest.join(" ")).iter().all(|item| {
                    matches!(
                        item.as_str(),
                        "actor" | "tenant" | "trace_id" | "request_id" | "locale"
                    )
                })
    ) || matches!(
        parts.as_slice(),
        ["default", "sync", "internal", value] if matches!(*value, "rpc" | "http" | "in_process")
    ) || matches!(
        parts.as_slice(),
        ["default", "async", value] if matches!(*value, "event_bus" | "in_process")
    );

    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-communication-contract",
            "workspace communication uses `propagate ...`, `default sync internal rpc|http|in_process`, or `default async event_bus|in_process`.",
        ));
    }
}

pub(crate) fn validate_workspace_gateway_route_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) -> bool {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.strip_prefix("route ") else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-gateway-contract",
            "workspace gateway routes use `route \"/path/*\" to app <name>`.",
        ));
        return false;
    };
    let Some((_path, tail)) = quoted_prefix(rest.trim()) else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-gateway-contract",
            "workspace gateway route paths must be quoted.",
        ));
        return false;
    };
    let parts: Vec<_> = tail.split_whitespace().collect();
    let valid = matches!(parts.as_slice(), ["to", "app", target] if is_identifier(target));
    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-gateway-contract",
            "workspace gateway routes currently target apps with `to app <name>`.",
        ));
    }
    valid
}

pub(crate) fn validate_workspace_gateway_route_child(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let valid = matches!(parts.as_slice(), ["auth", "propagate"])
        || matches!(parts.as_slice(), ["tenant", "propagate"])
        || matches!(parts.as_slice(), ["timeout", value] if is_quoted_lzx_literal(value))
        || matches!(parts.as_slice(), ["rate_limit", value] if is_quoted_lzx_literal(value));

    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "workspace-gateway-contract",
            "gateway route children use `auth propagate`, `tenant propagate`, `timeout \"...\"`, or `rate_limit \"...\"`.",
        ));
    }
}

pub(crate) fn quoted_prefix(value: &str) -> Option<(&str, &str)> {
    let rest = value.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some((&rest[..end], rest[end + 1..].trim()))
}
