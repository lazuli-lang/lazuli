//! `query.sql` (inline path) and `query.view` (`@file.<name>.sql`
//! reference) — both lower to `SqlQueryDecl` with a `SqlQueryKind`
//! discriminator.
//!
//! Extracted from the original monolithic `query.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::super::error::ParseError;
use super::super::super::lzx::try_parse_policy_expr;
use super::blocks::{parse_query_indented_block, parse_query_params_block};
use crate::ast::{CommandInputSlot, PolicyExprAst, QueryDecl, Span, SqlQueryDecl, SqlQueryKind};

pub(super) fn parse_query_sql_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    parse_sql_backed_query_decl(lines, start, rest, SqlQueryKind::Sql)
}

pub(super) fn parse_query_view_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
) -> Result<(QueryDecl, usize), ParseError> {
    parse_sql_backed_query_decl(lines, start, rest, SqlQueryKind::View)
}

fn parse_sql_backed_query_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    kind: SqlQueryKind,
) -> Result<(QueryDecl, usize), ParseError> {
    let header = &lines[start];
    let name = rest.trim().to_owned();
    if name.is_empty() {
        return Err(line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a name",
                SqlQueryKind::View => "`query.view` requires a name",
            },
        ));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut params: Vec<CommandInputSlot> = Vec::new();
    let mut scope_lines: Vec<String> = Vec::new();
    let mut returns: Option<String> = None;
    let mut sql_path: Option<String> = None;
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
                match kind {
                    SqlQueryKind::Sql => {
                        "`query.sql` body children use one indentation level deeper than the header"
                    }
                    SqlQueryKind::View => {
                        "`query.view` body children use one indentation level deeper than the header"
                    }
                },
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
        } else if trimmed == "scope" {
            let (lines_collected, next) = parse_query_indented_block(lines, i, grandchild_indent);
            scope_lines = lines_collected;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("returns ") {
            returns = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("sql ") {
            if kind == SqlQueryKind::View {
                return Err(line_error(
                    line,
                    "`query.view` uses `source @file.<name>.sql`; `sql \"./<path>.sql\"` is reserved for `query.sql`",
                ));
            }
            sql_path = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("source ") {
            if kind != SqlQueryKind::View {
                return Err(line_error(
                    line,
                    "`source @file.<name>.sql` is only valid on `query.view`; use `sql \"./<path>.sql\"` on `query.sql`",
                ));
            }
            let source = rest.trim();
            if !source.starts_with("@file.") || !source.ends_with(".sql") {
                return Err(line_error(
                    line,
                    "`query.view source` must be shaped `@file.<name>.sql`",
                ));
            }
            sql_path = Some(source.to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                match kind {
                    SqlQueryKind::Sql => {
                        "`query.sql` children are `policy`, `params`, `scope`, `returns`, `sql`, or `gate behind/quota plan.*`"
                    }
                    SqlQueryKind::View => {
                        "`query.view` children are `policy`, `returns`, `source`, `params`, `scope`, or `gate behind/quota plan.*`"
                    }
                },
            ));
        }
    }

    let returns = returns.ok_or_else(|| {
        line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a `returns <Type>` declaration",
                SqlQueryKind::View => "`query.view` requires a `returns <Type>` declaration",
            },
        )
    })?;
    let sql_path = sql_path.ok_or_else(|| {
        line_error(
            header,
            match kind {
                SqlQueryKind::Sql => "`query.sql` requires a `sql \"./<path>.sql\"` declaration",
                SqlQueryKind::View => {
                    "`query.view` requires a `source @file.<name>.sql` declaration"
                }
            },
        )
    })?;
    Ok((
        QueryDecl::Sql(SqlQueryDecl {
            name,
            kind,
            public_contract: None,
            policy,
            policy_expr,
            params,
            scope_lines,
            returns,
            sql_path,
            span: Span::new(header.start, last_end),
        }),
        i,
    ))
}
