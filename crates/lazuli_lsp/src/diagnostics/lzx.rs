//! Diagnostics for the LZX surface contract family.
//!
//! `.lzx` files declare experiences and per-stack surface projections
//! (`*.web.lzx`, `*.mobile.lzx`, etc.). This module owns the file-local
//! shape checks on:
//!
//! * `experience` / `surface` headers and audience scopes
//!   ([`lzx_contract_diagnostics`]).
//! * Top-level `app` `route <name>: <Path>` declarations
//!   ([`lzx_route_contract_diagnostics`] + [`lzx_app_route_diagnostics`]).
//! * `view <kind> <name> for route.<slot>` declarations on `.lzx`
//!   surfaces ([`lzx_route_view_diagnostics`]).
//! * File-name → platform / audience pairing
//!   ([`lzx_filename_diagnostics`], [`lzx_platform_from_file_name`]).
//!
//! ## Shared helpers
//!
//! The tiny LZX lexers ([`unquote_lzx_literal`], [`is_quoted_lzx_literal`],
//! [`split_items`], [`route_slot_name`], [`lzx_route_references`],
//! [`path_references`], [`first_lzx_surface_header`],
//! [`lzx_declared_path_params`]) live here too — they are heavily
//! consumed by other catalog modules (api, cache, profile, workspace,
//! external, app, …) and ride the standard
//! `pub(crate) use diagnostics::lzx::*;` re-export so every existing
//! `crate::*` import keeps resolving.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

use crate::{is_identifier, leading_spaces, simple_canonical_diagnostic};

pub(crate) fn lzx_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_surface: Option<(usize, String, bool, Option<String>)> = None;
    let mut in_audience = false;
    let mut in_view_extension = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.contains("+=") || trimmed.contains("-=") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "lzx-no-partial-override",
                "`.lzx` forbids partial overrides such as `+=`/`-=`. Redeclare the whole view for this audience/tenant so the block remains a local truth.",
            ));
        }

        if leading_spaces(line) == 4
            && let Some(target) = trimmed.strip_prefix("opens ")
            && !target.contains('(')
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-open-binding",
                "view navigation should bind route arguments explicitly, e.g. `opens detail(id: row.id)`, so generators do not infer row identity.",
            ));
        }

        if leading_spaces(line) == 6
            && let Some(target) = trimmed.strip_prefix("submit ")
            && is_identifier(target.trim())
        {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-submit-target",
                "platform form submits should use an explicit command reference such as `command.create` or `customer.command.capture_lead`, not a bare verb.",
            ));
        }

        if leading_spaces(line) == 0 {
            in_view_extension = false;

            if let Some((surface_line, surface_header, has_uses_experience, _)) =
                current_surface.take()
                && !has_uses_experience
            {
                diagnostics.push(simple_canonical_diagnostic(
                    surface_line,
                    &surface_header,
                    DiagnosticSeverity::ERROR,
                    "lzx-surface-dependency",
                    "concrete `.lzx` surfaces must declare `uses experience <name>`; platform views project the abstract experience instead of calling `.lzi` directly.",
                ));
            }

            in_audience = false;

            if trimmed.starts_with("surface ") {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() == 2 && matches!(parts.get(1), Some(&"web" | &"mobile")) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-surface-header",
                        "put the experience name before the platform: `surface <experience> web` or `surface <experience> mobile`.",
                    ));
                } else if parts.len() < 3 {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-surface-header",
                        "concrete `.lzx` surfaces use `surface <experience> <platform>`, with protected platforms `web` or `mobile`.",
                    ));
                } else {
                    if matches!(parts.get(1), Some(&"web" | &"mobile")) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::ERROR,
                            "lzx-surface-header",
                            "put the experience name before the platform: `surface <experience> web` or `surface <experience> mobile`.",
                        ));
                    }
                    if !matches!(parts.get(2), Some(&"web" | &"mobile")) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::ERROR,
                            "lzx-platform",
                            "canonical `.lzx` platform suffixes are protected: use `web` or `mobile` in the surface header; product axes belong in `audience`/`tenant` blocks.",
                        ));
                    }
                }
                current_surface = Some((
                    line_index,
                    line.to_owned(),
                    false,
                    parts.get(2).map(|platform| (*platform).to_owned()),
                ));
            }

            continue;
        }

        if leading_spaces(line) == 2 {
            in_view_extension = trimmed.starts_with("extends @anchor.");
        } else if leading_spaces(line) < 2 {
            in_view_extension = false;
        }

        if in_view_extension && leading_spaces(line) == 4 && trimmed.starts_with("block ") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "lzx-extension-slot",
                "view extensions should place blocks under an explicit slot, e.g. `slot aside` then `block @client.tag_editor`, so composition order and placement are deterministic.",
            ));
        }

        if let Some((_, _, has_uses_experience, platform)) = current_surface.as_mut() {
            if leading_spaces(line) == 2 {
                if trimmed.starts_with("uses experience ") {
                    *has_uses_experience = true;
                    in_audience = false;
                    continue;
                }

                if trimmed.starts_with("audience ") {
                    in_audience = true;
                    continue;
                }

                if trimmed.starts_with("view ") {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "lzx-audience-required",
                        "concrete platform views live under `audience ...` blocks. Product axes are source syntax, not filename-only convention.",
                    ));
                }
            } else if leading_spaces(line) <= 2 {
                in_audience = false;
            } else if trimmed.starts_with("view ") && !in_audience {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "lzx-audience-required",
                    "concrete platform views live under `audience ...` blocks.",
                ));
            } else if in_audience
                && leading_spaces(line) == 4
                && platform.as_deref() == Some("mobile")
                && let Some(view_type) = trimmed.split_whitespace().nth(2)
                && matches!(view_type, "Table" | "SidePanel")
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "lzx-mobile-primitive",
                    "mobile projections should use mobile-native primitives such as `List`, `Screen`, or `Sheet` instead of web-oriented `Table`/`SidePanel`.",
                ));
            }
        }
    }

    if let Some((surface_line, surface_header, has_uses_experience, _)) = current_surface
        && !has_uses_experience
    {
        diagnostics.push(simple_canonical_diagnostic(
            surface_line,
            &surface_header,
            DiagnosticSeverity::ERROR,
            "lzx-surface-dependency",
            "concrete `.lzx` surfaces must declare `uses experience <name>`; platform views project the abstract experience instead of calling `.lzi` directly.",
        ));
    }

    diagnostics
}

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

pub(crate) fn lzx_declared_path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let bytes = path.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b':' {
            let start = index + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || matches!(bytes[end], b'_'))
            {
                end += 1;
            }
            if end > start {
                params.push(path[start..end].to_owned());
            }
            index = end;
            continue;
        }

        if bytes[index] == b'[' {
            let start = index + 1;
            if let Some(close_offset) = path[start..].find(']') {
                let raw = &path[start..start + close_offset];
                let name = raw.trim_start_matches("...");
                if is_identifier(name) {
                    params.push(name.to_owned());
                }
                index = start + close_offset + 1;
                continue;
            }
        }

        index += 1;
    }

    params
}

pub(crate) fn unquote_lzx_literal(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
}

pub(crate) fn is_quoted_lzx_literal(value: &str) -> bool {
    value.starts_with('"') && value.ends_with('"') && value.len() >= 2
}

pub(crate) fn split_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
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

pub(crate) fn route_slot_name(route: &str) -> Option<&str> {
    route
        .split_once(':')
        .map(|(name, _)| name.trim())
        .or_else(|| route.split_whitespace().next())
        .filter(|name| is_identifier(name))
}

pub(crate) fn lzx_route_references(source: &str) -> Vec<&str> {
    path_references(source, "route.")
}

pub(crate) fn path_references<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
    let mut references = Vec::new();
    let mut rest = source;

    while let Some(start) = rest.find(prefix) {
        let after = &rest[start + prefix.len()..];
        let len = after
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        if len > 0 {
            references.push(&after[..len]);
        }
        rest = &after[len..];
    }

    references
}

pub(crate) fn lzx_filename_diagnostics(uri: &Url, source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let Some(file_name) = uri
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .map(str::to_owned)
    else {
        return diagnostics;
    };

    let expected_platform = lzx_platform_from_file_name(&file_name);
    let surface_header = first_lzx_surface_header(source);

    match (expected_platform, surface_header) {
        (Some(platform), Some((line_index, line, header_platform))) => {
            if header_platform != platform {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "lzx-file-header-mismatch",
                    &format!(
                        "`{file_name}` is a `{platform}` projection, so its header should use `surface <experience> {platform}`."
                    ),
                ));
            }
        }
        (Some(platform), None) => {
            diagnostics.push(simple_canonical_diagnostic(
                0,
                source.lines().next().unwrap_or(""),
                DiagnosticSeverity::ERROR,
                "lzx-file-header-mismatch",
                &format!(
                    "`{file_name}` is a `{platform}` projection, so it should declare `surface <experience> {platform}`."
                ),
            ));
        }
        (None, Some((line_index, line, _))) if file_name.ends_with(".lzx") => {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "lzx-file-header-mismatch",
                "abstract `.lzx` files declare `experience <name>`; concrete surfaces belong in `.web.lzx` or `.mobile.lzx` files.",
            ));
        }
        _ => {}
    }

    diagnostics
}

pub(crate) fn lzx_platform_from_file_name(file_name: &str) -> Option<&'static str> {
    if file_name.ends_with(".web.lzx") {
        Some("web")
    } else if file_name.ends_with(".mobile.lzx") {
        Some("mobile")
    } else {
        None
    }
}

pub(crate) fn first_lzx_surface_header(source: &str) -> Option<(usize, &str, &str)> {
    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if leading_spaces(line) != 0 || !trimmed.starts_with("surface ") {
            continue;
        }
        let platform = trimmed.split_whitespace().nth(2)?;
        return Some((line_index, line, platform));
    }

    None
}
