//! `query.lookup` — single-row read keyed by `<field>: <Type>`.
//!
//! Extracted from the original monolithic `query.rs`. Two shapes are
//! accepted (see header comment in `query/mod.rs`):
//! - inline: `query.lookup <name> by <field>: <Type>`
//! - block:  `query.lookup <name>` + `params`/`filters`/`policy` children

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::super::lzx::try_parse_policy_expr;
use super::blocks::{parse_query_indented_block, parse_query_params_block};
use crate::ast::{
    CommandInputSlot, LookupKey, LookupQueryDecl, PolicyExprAst, QueryDecl, Span,
};

pub(super) fn parse_query_lookup_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let rest = rest.trim();
    // Two shapes accepted today:
    //   - inline: `<name> by <field>: <Type>` (Cut A canonical).
    //   - block: `<name>` with `params` / `filters` / `policy` children.
    let (name, inline_key) = if let Some((name, after)) = rest.split_once(" by ") {
        (
            name.trim().to_owned(),
            Some(parse_lookup_key(header, after)?),
        )
    } else {
        (rest.to_owned(), None)
    };
    if name.is_empty() {
        return Err(line_error(header, "`query.lookup` requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    // `filters` lines are captured for cross-check but not lowered to
    // typed keys today; Cut A's contract is `keys` (from `by ...`) so
    // multi-key block lookups keep their filters in the AST sidecar
    // while IR uses `keys` from the inline form.
    let mut filters: Vec<String> = Vec::new();
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
                "`query.lookup` body children use one indentation level deeper than the header",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if trimmed == "params" {
            let (parsed, next) = parse_query_params_block(lines, i, grandchild_indent)?;
            params = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "filters" {
            let (collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            filters = collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "`query.lookup` children are `policy`, `params`, `filters`, or `gate behind/quota plan.*`",
            ));
        }
    }
    // Build the IR-facing keys list. Inline form contributes one
    // explicit key; block form synthesises a key per param so the IR
    // shape stays consistent.
    let keys: Vec<LookupKey> = if let Some(key) = inline_key {
        vec![key]
    } else {
        params
            .iter()
            .map(|p| LookupKey {
                name: p.name.clone(),
                type_text: p.type_text.clone(),
                span: p.span,
            })
            .collect()
    };
    Ok((
        QueryDecl::Lookup(LookupQueryDecl {
            name,
            public_contract: None,
            policy,
            policy_expr,
            keys,
            filters,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}

fn parse_lookup_key(line: &SourceLine<'_>, rest: &str) -> Result<LookupKey, ParseError> {
    let (name, type_text) = rest.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires `<field>: <Type>`",
        )
    })?;
    let name = name.trim();
    if name.is_empty() {
        return Err(line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires a field name before `:`",
        ));
    }
    let type_text = type_text.trim();
    if type_text.is_empty() {
        return Err(line_error(
            line,
            "`query.lookup ... by <field>: <Type>` requires a type after `:`",
        ));
    }
    Ok(LookupKey {
        name: name.to_owned(),
        type_text: type_text.to_owned(),
        span: Span::new(line.start, line.end),
    })
}
