//! `route` and `view` declarations on `.lzx` surfaces.
//!
//! Top-level `route <name>: <Path>` declarations on `app` manifests
//! and `view <kind> <name> for route.<slot>` declarations inside
//! experiences flow through this walker. The path-param contract
//! ensures every `:slot` in the path has a matching `route <slot>:
//! <Type>` declaration; the view contract ensures every `route.<name>`
//! reference and every `action <name> -> <target>(...)` invocation
//! binds explicitly.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

use super::lex::{
    lzx_declared_path_params, lzx_route_references, path_references, route_slot_name,
    unquote_lzx_literal,
};

#[derive(Debug)]
pub(crate) struct LzxRouteViewFacts {
    routes: HashSet<String>,
    references: Vec<(usize, String, String)>,
    unbound_target_actions: Vec<(usize, String)>,
}

#[derive(Debug)]
pub(crate) struct LzxAppRouteFacts {
    line_index: usize,
    line: String,
    has_path: bool,
    has_to: bool,
    has_surface: bool,
    has_audience: bool,
    declared_routes: HashSet<String>,
    path_params: Vec<String>,
    route_references: Vec<(usize, String, String)>,
}

pub(crate) fn lzx_route_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_experience = false;
    let mut current_view: Option<LzxRouteViewFacts> = None;
    let mut current_route: Option<LzxAppRouteFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 {
            if let Some(view) = current_view.take() {
                diagnostics.extend(lzx_route_view_diagnostics(view));
            }
            if let Some(route) = current_route.take() {
                diagnostics.extend(lzx_app_route_diagnostics(route));
            }
            if trimmed.starts_with("route ") {
                current_route = Some(LzxAppRouteFacts {
                    line_index,
                    line: line.to_owned(),
                    has_path: false,
                    has_to: false,
                    has_surface: false,
                    has_audience: false,
                    declared_routes: HashSet::new(),
                    path_params: Vec::new(),
                    route_references: Vec::new(),
                });
            }
            in_experience = trimmed.starts_with("experience ");
            continue;
        }

        if let Some(route) = current_route.as_mut() {
            if leading_spaces(line) == 2 {
                if let Some(path) = trimmed.strip_prefix("path ") {
                    route.has_path = true;
                    route
                        .path_params
                        .extend(lzx_declared_path_params(unquote_lzx_literal(path.trim())));
                } else if let Some(routes) = trimmed.strip_prefix("route ") {
                    for slot in routes
                        .split(',')
                        .filter_map(|part| route_slot_name(part.trim()))
                    {
                        route.declared_routes.insert(slot.to_owned());
                    }
                } else if let Some(target) = trimmed.strip_prefix("to ") {
                    route.has_to = true;
                    for reference in path_references(target, "route.") {
                        route.route_references.push((
                            line_index,
                            line.to_owned(),
                            reference.to_owned(),
                        ));
                    }
                } else if trimmed.starts_with("surface ") {
                    route.has_surface = true;
                } else if trimmed.starts_with("audience ") {
                    route.has_audience = true;
                }
            }
            continue;
        }

        if !in_experience {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("view ") {
            if let Some(view) = current_view.take() {
                diagnostics.extend(lzx_route_view_diagnostics(view));
            }
            current_view = Some(LzxRouteViewFacts {
                routes: HashSet::new(),
                references: Vec::new(),
                unbound_target_actions: Vec::new(),
            });
            continue;
        }

        if leading_spaces(line) <= 2 {
            if let Some(view) = current_view.take() {
                diagnostics.extend(lzx_route_view_diagnostics(view));
            }
            continue;
        }

        let Some(view) = current_view.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 4
            && let Some(route) = trimmed.strip_prefix("route ")
            && let Some(name) = route_slot_name(route)
        {
            view.routes.insert(name.to_owned());
        }

        if leading_spaces(line) == 4
            && let Some((_, target)) = trimmed
                .strip_prefix("action ")
                .and_then(|rest| rest.split_once(" -> "))
            && !target.contains('(')
            && (target.contains(".command.") || target.contains(".workflow."))
        {
            view.unbound_target_actions
                .push((line_index, line.to_owned()));
        }

        for reference in lzx_route_references(trimmed) {
            view.references
                .push((line_index, line.to_owned(), reference.to_owned()));
        }
    }

    if let Some(view) = current_view {
        diagnostics.extend(lzx_route_view_diagnostics(view));
    }
    if let Some(route) = current_route {
        diagnostics.extend(lzx_app_route_diagnostics(route));
    }

    diagnostics
}

pub(crate) fn lzx_app_route_diagnostics(route: LzxAppRouteFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if !route.has_path {
        diagnostics.push(simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::ERROR,
            "lzx-app-route-contract",
            "top-level routes should declare a concrete `path`; `surface <name> web|mobile` decides whether it is a web URL path or mobile route pattern.",
        ));
    }
    if !route.has_to {
        diagnostics.push(simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::ERROR,
            "lzx-app-route-contract",
            "top-level routes should declare `to <experience>.view.<view>(...)` so generated links and navigation have a typed target.",
        ));
    }
    if !route.has_surface || !route.has_audience {
        diagnostics.push(simple_canonical_diagnostic(
            route.line_index,
            &route.line,
            DiagnosticSeverity::ERROR,
            "lzx-app-route-contract",
            "top-level routes should bind `surface <name> web|mobile` and `audience <name>` so authorization and platform routing are explicit.",
        ));
    }

    for path_param in route.path_params {
        if !route.declared_routes.contains(&path_param) {
            diagnostics.push(simple_canonical_diagnostic(
                route.line_index,
                &route.line,
                DiagnosticSeverity::WARNING,
                "lzx-route-param-contract",
                &format!(
                    "route path parameter `{path_param}` should be declared with `route {path_param}: <Type>` so route builders are type-safe.",
                ),
            ));
        }
    }

    for (line_index, line, reference) in route.route_references {
        if !route.declared_routes.contains(&reference) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "lzx-route-param-contract",
                &format!(
                    "route target references `route.{reference}` but the route does not declare `route {reference}: ...`.",
                ),
            ));
        }
    }

    diagnostics
}

pub(crate) fn lzx_route_view_diagnostics(view: LzxRouteViewFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line, route) in view.references {
        if !view.routes.contains(&route) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "lzx-route-contract",
                &format!(
                    "view references `route.{route}` but does not declare `route {route}: ...`; route bindings should be explicit in the abstract experience."
                ),
            ));
        }
    }

    if !view.routes.is_empty() {
        for (line_index, line) in view.unbound_target_actions {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "lzx-action-route-binding",
                "actions in routed views should pass route arguments explicitly, e.g. `action archive -> feature.workflow.name.transition(id: route.id)`.",
            ));
        }
    }

    diagnostics
}
