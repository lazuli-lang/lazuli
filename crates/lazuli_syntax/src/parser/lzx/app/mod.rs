//! `.lzx` **app-surface dialect** — the `app.lzx` top-level grammar.
//!
//! ## What lives here
//!
//! - **`parse_lzx_document`**: the public entry point. Walks the whole
//!   source emitting `app` / `route` / `experience` / `surface`
//!   sub-blocks. Re-exported from `lzx/mod.rs` so external callers
//!   keep using `crate::parser::lzx::parse_lzx_document`.
//! - **`parse_lzx_app`**: parses the `app <name>` manifest with its
//!   `title` / `version` / `targets` / `default_locale` /
//!   `auth_failed_redirect` / `route_guard` / `error_page` /
//!   `uses` children.
//! - **`parse_lzx_route_guard_defaults`**: the `route_guard` block
//!   inside `app` (default policy, redirects, skeleton).
//! - **`parse_lzx_view_guard`** + helpers (`parse_lzx_forbid_when`,
//!   `parse_lzx_view_guard_policy`, `parse_lzx_redirect_clause`,
//!   `parse_lzx_requires_lifecycle`, `parse_lzx_optional_substep_tail`,
//!   `parse_lzx_on_lifecycle_pending`): the per-`route` / per-`view`
//!   `policy` block. Shared with route and experience parsers; carry
//!   `pub(super)` so siblings in `lzx` can call them.
//! - **`attach_lzx_requires_lifecycle`** + **`attach_lzx_on_lifecycle_pending`**:
//!   guard-mutating helpers used by experience-view parsers.
//! - **`parse_lzx_error_page`**: the `error_page <status>` block.

use crate::ast::{LzxApp, LzxDocument, Span};

use super::super::common::{SourceLine, is_trivia, line_error, split_lzx_list, unquote_lzx_value};
use super::super::error::ParseError;

use super::{parse_lzx_experience, parse_lzx_route, parse_lzx_surface};

mod view_guard;
pub(super) use view_guard::{
    attach_lzx_on_lifecycle_pending, attach_lzx_requires_lifecycle, parse_lzx_on_lifecycle_pending,
    parse_lzx_optional_substep_tail, parse_lzx_requires_lifecycle, parse_lzx_route_guard_defaults,
    parse_lzx_view_guard,
};

mod error_page;
use error_page::parse_lzx_error_page;

pub fn parse_lzx_document(source: &str) -> Result<LzxDocument, ParseError> {
    let lines = super::super::common::source_lines(source);
    let mut app = None;
    let mut routes = Vec::new();
    let mut experiences = Vec::new();
    let mut surfaces = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            index += 1;
            continue;
        }

        if line.indent != 0 {
            return Err(line_error(
                line,
                "top-level `.lzx` declarations are not indented",
            ));
        }

        if trimmed.starts_with("app ") {
            if app.is_some() {
                return Err(line_error(
                    line,
                    "`.lzx` files can declare only one `app` manifest",
                ));
            }
            let (parsed_app, next) = parse_lzx_app(&lines, index)?;
            app = Some(parsed_app);
            index = next;
        } else if trimmed.starts_with("route ") {
            let (route, next) = parse_lzx_route(&lines, index)?;
            routes.push(route);
            index = next;
        } else if trimmed.starts_with("experience ") {
            let (experience, next) = parse_lzx_experience(&lines, index)?;
            experiences.push(experience);
            index = next;
        } else if trimmed.starts_with("surface ") {
            let (surface, next) = parse_lzx_surface(&lines, index)?;
            surfaces.push(surface);
            index = next;
        } else {
            return Err(line_error(
                line,
                "expected `app <name>`, `route <name>`, `experience <name>`, or `surface <experience> <platform>`",
            ));
        }
    }

    Ok(LzxDocument {
        app,
        routes,
        experiences,
        surfaces,
        span: Span::new(0, source.len()),
    })
}

fn parse_lzx_app(lines: &[SourceLine<'_>], start: usize) -> Result<(LzxApp, usize), ParseError> {
    let header = &lines[start];
    let parts: Vec<_> = header.text.trim_start().split_whitespace().collect();
    if parts.len() != 2 {
        return Err(line_error(header, "app manifests use `app <name>`"));
    }

    let mut title = None;
    let mut version = None;
    let mut targets = Vec::new();
    let mut default_locale = None;
    let mut default_timezone = None;
    let mut auth_failed_redirect = None;
    let mut route_guard = None;
    let mut actor_query = None;
    let mut not_found = None;
    let mut error_pages = Vec::new();
    let mut uses = Vec::new();
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
            return Err(line_error(
                line,
                "app manifest children use two-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("title ") {
            title = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("version ") {
            version = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if trimmed == "targets" {
            index += 1;
            while index < lines.len() {
                let target_line = &lines[index];
                let target_trimmed = target_line.text.trim_start();
                if is_trivia(target_trimmed) {
                    index += 1;
                    continue;
                }
                if target_line.indent <= 2 {
                    break;
                }
                if target_line.indent != 4 {
                    return Err(line_error(
                        target_line,
                        "app targets use four-space indentation",
                    ));
                }
                targets.push(target_trimmed.to_owned());
                index += 1;
            }
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("targets ") {
            targets.extend(split_lzx_list(rest));
        } else if let Some(rest) = trimmed.strip_prefix("default_locale ") {
            default_locale = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("default_timezone ") {
            default_timezone = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("auth_failed_redirect ") {
            auth_failed_redirect = Some(rest.trim().to_owned());
        } else if trimmed == "route_guard" {
            if route_guard.is_some() {
                return Err(line_error(
                    line,
                    "app manifest declares `route_guard` at most once",
                ));
            }
            let (parsed, next) = parse_lzx_route_guard_defaults(lines, index, 2)?;
            route_guard = Some(parsed);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("actor_query ") {
            actor_query = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("not_found ") {
            not_found = Some(rest.trim().to_owned());
        } else if trimmed.starts_with("error_page ") {
            let (error_page, next) = parse_lzx_error_page(lines, index)?;
            error_pages.push(error_page);
            index = next;
            continue;
        } else if let Some(rest) = trimmed.strip_prefix("uses ") {
            uses = split_lzx_list(rest);
        } else {
            return Err(line_error(
                line,
                "app manifest children are `title`, `version`, `targets`, `default_locale`, `default_timezone`, `auth_failed_redirect`, `route_guard`, `actor_query`, `not_found`, `error_page <status>`, or `uses` declarations",
            ));
        }
        index += 1;
    }

    Ok((
        LzxApp {
            name: parts[1].to_owned(),
            title,
            version,
            targets,
            default_locale,
            default_timezone,
            auth_failed_redirect,
            route_guard,
            actor_query,
            not_found,
            error_pages,
            uses,
            span: Span::new(header.start, lines[index.saturating_sub(1)].end),
        },
        index,
    ))
}
