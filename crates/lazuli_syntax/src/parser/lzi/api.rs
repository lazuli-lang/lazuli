//! `api <name>` block parser — HTTP-facing query/command surface.
//!
//! The header sits at feature child indent (2 spaces); body children at
//! agent child indent (4 spaces). The block lifts the read-side HTTP
//! contract above plain `query` / `command` declarations so an LLM can
//! cold-read an api as "method + path + output + policy" without
//! reverse-engineering route bindings.
//!
//! ## Closed catalog
//!
//! - `method <VERB>` — GET / POST / PUT / PATCH / DELETE (required).
//! - `path "<url>"` — route pattern (required).
//! - `output <Type>` — typed response shape (required).
//! - `route <slot>: <Type>` — path-bound slots (lifecycle routes).
//! - `input` — block of typed body slots (shared with command).
//! - `policy <expr>` — gate the call (typed via `parse_policy_expr`).
//! - `rate_limit "<spec>" [in <env>, ...]` — folded into a
//!   `RateLimitSpecAst` via the shared `parse_rate_limit_line_body`
//!   + `fold_rate_limit_line` helpers.
//! - `handler "<path>"` — Go-side body shim.
//! - `locale_negotiate` — block delegating to `lzi/locale.rs`.
//! - `deprecated` — block or single-line `deprecated since "<date>"`.
//! - `gate behind/quota plan.*` — tolerated; lifted via side-channel.
//!
//! ## See also
//!
//! - `docs/proposals/api-vocab.md`
//! - `lazuli_ir::nodes::api` — typed lowering target.

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::locale::parse_locale_negotiate_decl;
use super::numerics::{fold_rate_limit_line, parse_rate_limit_line_body};
use super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, parse_command_deprecated,
    parse_command_input_block, parse_command_route_slot, parse_deprecated_block,
};
use crate::ast::{
    ApiDecl, CommandDeprecatedDecl, CommandInputDecl, CommandRouteSlot, HttpMethod,
    LocaleNegotiateDecl, PolicyExprAst, RateLimitSpecAst, Span,
};

pub(super) fn parse_api_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(ApiDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("api ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "api header must be `api <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "api header requires a name"));
    }
    let mut method: Option<HttpMethod> = None;
    let mut path: Option<String> = None;
    let mut output: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut handler: Option<String> = None;
    let mut locale_negotiate: Option<LocaleNegotiateDecl> = None;
    let mut route: Vec<CommandRouteSlot> = Vec::new();
    let mut input: Option<CommandInputDecl> = None;
    let mut deprecated: Option<CommandDeprecatedDecl> = None;
    let mut last_end = header.end;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }
        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "`api` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("method ") {
            let token = rest.trim();
            method = Some(HttpMethod::from_token(token).ok_or_else(|| {
                line_error(
                    line,
                    "`api method` requires GET, POST, PUT, PATCH, or DELETE",
                )
            })?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("path ") {
            path = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("output ") {
            output = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            handler = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("route ") {
            route.push(parse_command_route_slot(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed == "input" {
            let (parsed, next) = parse_command_input_block(lines, i)?;
            input = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "locale_negotiate" {
            if locale_negotiate.is_some() {
                return Err(line_error(
                    line,
                    "`api` may declare at most one `locale_negotiate` block",
                ));
            }
            let (parsed, next) = parse_locale_negotiate_decl(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            locale_negotiate = Some(parsed);
            i = next;
        } else if trimmed == "deprecated" {
            let (parsed, next) = parse_deprecated_block(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            deprecated = Some(parsed);
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("deprecated ") {
            deprecated = Some(parse_command_deprecated(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass; tolerate here.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`api` children are `method`, `path`, `route`, `input`, `output`, `policy`, `rate_limit`, `handler`, `locale_negotiate`, `deprecated`, or `gate behind/quota plan.*`",
            ));
        }
    }
    let method =
        method.ok_or_else(|| line_error(header, "`api` requires a `method <VERB>` declaration"))?;
    let path =
        path.ok_or_else(|| line_error(header, "`api` requires a `path \"<route>\"` declaration"))?;
    let output = output.ok_or_else(|| {
        line_error(
            header,
            "`api` requires an `output <Type>` declaration (e.g. `output @cap.File(...)`)",
        )
    })?;
    Ok((
        ApiDecl {
            name,
            method,
            path,
            output,
            policy,
            policy_expr,
            rate_limit,
            handler,
            locale_negotiate,
            route,
            input,
            deprecated,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
