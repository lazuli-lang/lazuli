//! `query.list` — multi-row read with the full ergonomics catalog
//! (policy, modifier, params, scope/scope override, filters, search,
//! cache (inline or profile ref), paginate, order).
//!
//! Extracted from the original monolithic `query.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::super::lzx::try_parse_policy_expr;
use super::blocks::{
    parse_query_indented_block, parse_query_params_block, parse_query_scope_override_block,
    parse_query_search,
};
use crate::ast::{CommandInputSlot, ListQueryDecl, PolicyExprAst, QueryDecl, QuerySearch, Span};

pub(super) fn parse_query_list_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let name = rest.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(header, "`query.list` requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut modifier: Option<String> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    let mut scope_override = false;
    let mut scope_reason: Option<String> = None;
    let mut scope_assignments: Vec<String> = Vec::new();
    let mut scope_lines: Vec<String> = Vec::new();
    let mut filters: Vec<String> = Vec::new();
    let mut search: Option<QuerySearch> = None;
    let mut cache: Vec<String> = Vec::new();
    let mut cache_profile_ref: Option<String> = None;
    let mut paginate: Option<u32> = None;
    let mut order: Vec<String> = Vec::new();
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
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`query.list` body children use one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("modifier ") {
            modifier = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope override" {
            scope_override = true;
            let (reason, assignments, next) =
                parse_query_scope_override_block(lines, i, grandchild_indent)?;
            scope_reason = reason;
            scope_assignments = assignments;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "scope" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            scope_lines = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "filters" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            filters = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("search ") {
            let (parsed, next) = parse_query_search(lines, i, rest, grandchild_indent)?;
            search = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "cache" {
            // Inline shape: `cache` followed by indented `key`/`ttl`/...
            if cache_profile_ref.is_some() {
                return Err(line_error(
                    line,
                    "`query.list` may declare either an inline `cache` block or a `cache <profile>` reference, not both",
                ));
            }
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            cache = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("cache ") {
            // Cache bucket cycle (CL.C.3) — `cache <profile_name>` reference
            // form. Single-line shape pointing at a feature-level
            // `cache <name>` profile.
            let name = rest.trim();
            if name.is_empty() {
                return Err(line_error(
                    line,
                    "`cache <profile>` requires a profile name (declare it as a feature-level `cache <name>` block)",
                ));
            }
            if !cache.is_empty() {
                return Err(line_error(
                    line,
                    "`query.list` may declare either an inline `cache` block or a `cache <profile>` reference, not both",
                ));
            }
            if cache_profile_ref.is_some() {
                return Err(line_error(
                    line,
                    "`query.list` may declare `cache <profile>` only once",
                ));
            }
            cache_profile_ref = Some(name.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("paginate ") {
            paginate = Some(rest.trim().parse::<u32>().map_err(|_| {
                line_error(line, "`paginate` requires a positive integer page size")
            })?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("order ") {
            order.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`query.list` children are `policy`, `modifier`, `params`, `scope`/`scope override`, `filters`, `search`, `cache`, `paginate`, `order`, or `gate behind/quota plan.*`",
            ));
        }
    }

    Ok((
        QueryDecl::List(ListQueryDecl {
            name,
            public_contract: None,
            policy,
            policy_expr,
            modifier,
            params,
            scope_override,
            scope_reason,
            scope_assignments,
            scope_lines,
            filters,
            search,
            cache,
            cache_profile_ref,
            paginate,
            order,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}
