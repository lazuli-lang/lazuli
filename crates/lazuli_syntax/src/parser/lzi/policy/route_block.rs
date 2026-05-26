//! `when_denied_route` sub-block parser + the route-redirect-target
//! value parser. Extracted from the original monolithic `policy.rs`.

use super::super::super::common::{
    SourceLine, is_lzx_bare_ident, is_lzx_resume_ref, is_trivia, line_error, split_lzx_arrow,
    unquote_lzx_value,
};
use super::super::super::error::ParseError;
use crate::ast::{RoleMismatchArmAst, RouteRedirectTargetAst, Span, WhenDeniedRouteAst};

pub(super) fn parse_when_denied_route_block(
    lines: &[SourceLine<'_>],
    start: usize,
    header_indent: usize,
    arm_indent: usize,
) -> Result<(WhenDeniedRouteAst, usize), ParseError> {
    let header = &lines[start];
    if header.indent != header_indent || header.text.trim_start() != "when_denied_route" {
        return Err(line_error(
            header,
            "policy route denial blocks use `when_denied_route`",
        ));
    }

    let mut unauthenticated: Option<RouteRedirectTargetAst> = None;
    let mut role_mismatch: Vec<RoleMismatchArmAst> = Vec::new();
    let mut default: Option<RouteRedirectTargetAst> = None;
    let mut seen_roles = std::collections::BTreeSet::new();
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != arm_indent {
            return Err(line_error(
                line,
                "`when_denied_route` arms use one indentation level deeper than the block",
            ));
        }
        let Some((left, right)) = split_lzx_arrow(trimmed) else {
            return Err(line_error(
                line,
                "`when_denied_route` arms use `<case> -> view <name>` or `<case> -> path \"...\"`",
            ));
        };
        let left = left.trim();
        let target = parse_route_redirect_target(line, right.trim())?;
        if left == "unauthenticated" {
            if unauthenticated.is_some() {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares `unauthenticated` at most once",
                ));
            }
            unauthenticated = Some(target);
        } else if left == "default" {
            if default.is_some() {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares `default` at most once",
                ));
            }
            default = Some(target);
        } else if let Some(role) = left.strip_prefix("role_mismatch ") {
            let role = role.trim();
            if !is_lzx_bare_ident(role) {
                return Err(line_error(
                    line,
                    "`role_mismatch` requires a bare role identifier",
                ));
            }
            if !seen_roles.insert(role.to_owned()) {
                return Err(line_error(
                    line,
                    "`when_denied_route` declares each `role_mismatch <role>` at most once",
                ));
            }
            role_mismatch.push(RoleMismatchArmAst {
                role: role.to_owned(),
                target,
                span: Span::new(line.start, line.end),
            });
        } else {
            return Err(line_error(
                line,
                "`when_denied_route` arms are `unauthenticated`, `role_mismatch <role>`, or `default`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    if unauthenticated.is_none() && role_mismatch.is_empty() && default.is_none() {
        return Err(line_error(
            header,
            "`when_denied_route` requires at least one arm",
        ));
    }

    Ok((
        WhenDeniedRouteAst {
            unauthenticated,
            role_mismatch,
            default,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_route_redirect_target(
    line: &SourceLine<'_>,
    value: &str,
) -> Result<RouteRedirectTargetAst, ParseError> {
    if let Some(view) = value.strip_prefix("view ") {
        let view = view.trim();
        if !is_lzx_resume_ref(view) {
            return Err(line_error(
                line,
                "`view` redirect targets use `<view>` or `<feature>.<view>`",
            ));
        }
        return Ok(RouteRedirectTargetAst::View(view.to_owned()));
    }
    if let Some(path) = value.strip_prefix("path ") {
        let path = path.trim();
        if !(path.starts_with('"') && path.ends_with('"')) {
            return Err(line_error(
                line,
                "`path` redirect targets must be quoted string literals",
            ));
        }
        return Ok(RouteRedirectTargetAst::Path(
            unquote_lzx_value(path).to_owned(),
        ));
    }
    Err(line_error(
        line,
        "`when_denied_route` targets use `view <name>` or `path \"...\"`",
    ))
}
