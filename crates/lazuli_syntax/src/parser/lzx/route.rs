//! `.lzx` **`route`** declaration parser.
//!
//! Parses a single `route <name>` block. Children include `path`,
//! typed route params (`route <name>: <Type>`), `to`, `surface`,
//! `audience`, `lazy`, `prerender`, `loader <feature>.<query>`,
//! `pending_view`, `error_view`, `parent`, and the per-route
//! `policy` guard. Called from `parse_lzx_document` in `app.rs`.

use crate::ast::{LzxRoute, Span};

use super::super::common::{
    SourceLine, is_trivia, line_error, parse_lzx_bool, split_lzx_list, unquote_lzx_value,
};
use super::super::error::ParseError;

use super::app::parse_lzx_view_guard;

pub(super) fn parse_lzx_route(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(LzxRoute, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "routes use `route <name>`"));
    }

    let mut path = None;
    let mut routes = Vec::new();
    let mut to = None;
    let mut surface = None;
    let mut audience = None;
    let mut lazy = None;
    let mut prerender = None;
    let mut guard = None;
    let mut loaders: Vec<crate::ast::LzxRouteLoader> = Vec::new();
    let mut pending_view: Option<String> = None;
    let mut error_view: Option<String> = None;
    let mut parent: Option<String> = None;
    let mut route_params: Vec<crate::ast::RouteParamAst> = Vec::new();
    let mut index = start + 1;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent == 0 {
            break;
        }

        if line.indent != 2 {
            return Err(line_error(line, "route children use two-space indentation"));
        }

        if let Some(rest) = trimmed.strip_prefix("path ") {
            path = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            // Wave §2 (2026-05-24) — disambiguate two child shapes:
            //   `route <name>: <Type>`  →  typed path-param declaration
            //   `route <name>[, <name>]` → legacy routes-list (still
            //                              accepted, drops to the
            //                              vestigial `routes: Vec<String>`
            //                              field; no live consumer).
            let rest = rest.trim();
            if let Some((name_part, type_part)) = rest.split_once(':') {
                let name = name_part.trim();
                let type_ref = type_part.trim();
                if name.is_empty() || type_ref.is_empty() {
                    return Err(line_error(
                        line,
                        "route param declaration uses `route <name>: <Type>` (both name and type must be non-empty)",
                    ));
                }
                route_params.push(crate::ast::RouteParamAst {
                    name: name.to_owned(),
                    type_ref: type_ref.to_owned(),
                    span: Span::new(line.start, line.end),
                });
            } else {
                routes.extend(split_lzx_list(rest));
            }
        } else if let Some(rest) = trimmed.strip_prefix("to ") {
            to = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("surface ") {
            surface = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("audience ") {
            audience = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("lazy ") {
            lazy =
                Some(parse_lzx_bool(rest.trim()).ok_or_else(|| {
                    line_error(line, "route lazy uses `lazy true` or `lazy false`")
                })?);
        } else if let Some(rest) = trimmed.strip_prefix("prerender ") {
            prerender = Some(rest.trim().to_owned());
        } else if trimmed.starts_with("policy ") {
            if guard.is_some() {
                return Err(line_error(line, "route declares `policy` at most once"));
            }
            let (parsed, next) = parse_lzx_view_guard(lines, index, 2)?;
            guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("loader ") {
            // router-w5 — `loader <feature>.<query>`. Multiple
            // declarations supported (codegen emits Promise.all).
            let qualified = rest.trim();
            let (feature, query) = qualified.split_once('.').ok_or_else(|| {
                line_error(
                    line,
                    "`loader` references must be `<feature>.<query>` (e.g. `loader host.lookup_my_host`)",
                )
            })?;
            loaders.push(crate::ast::LzxRouteLoader {
                feature: feature.trim().to_owned(),
                query: query.trim().to_owned(),
                span: Span::new(line.start, line.end),
            });
        } else if let Some(rest) = trimmed.strip_prefix("pending_view ") {
            // router-w6 — `pending_view <component_key>`.
            if pending_view.is_some() {
                return Err(line_error(
                    line,
                    "route declares `pending_view` at most once",
                ));
            }
            pending_view = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("error_view ") {
            // router-w6 — `error_view <component_key>`.
            if error_view.is_some() {
                return Err(line_error(line, "route declares `error_view` at most once"));
            }
            error_view = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("parent ") {
            // router-w8 — `parent <route_name>` nests this route
            // under another route's tree.
            if parent.is_some() {
                return Err(line_error(line, "route declares `parent` at most once"));
            }
            parent = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "route children are `path`, `route <name>: <Type>`, `to`, `surface`, `audience`, `lazy`, `prerender`, `loader <feature>.<query>`, `pending_view <key>`, `error_view <key>`, or `policy` declarations",
            ));
        }
        index += 1;
    }

    Ok((
        LzxRoute {
            name: parts[1].to_owned(),
            path,
            routes,
            to,
            surface,
            audience,
            lazy,
            prerender,
            guard,
            loaders,
            pending_view,
            error_view,
            parent,
            route_params,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}
