//! `report <name>` feature child — generated-document vocabulary.
//!
//! A report projects a `query.*` source through a closed column catalog
//! into one or more output formats (csv, xlsx, pdf, ...). The parser
//! captures the surface; lowering resolves the column types and runtime
//! delivers via the requested storage / visibility / signed-URL policy.
//!
//! ## Grammar (closed)
//!
//! ```text
//! report <name>
//!   source <query_ref>           # required
//!   columns
//!     <name> from row.<field> [label "..."] [format "..."]
//!     <name> from @fn.<name>(arg, arg) [label "..."] [format "..."]
//!   formats csv, xlsx, pdf       # required (non-empty)
//!   storage <s3|fs|...>
//!   visibility <public|signed|...>
//!   signed_ttl <duration>
//!   filename "report.csv"
//!   policy @policy.<name>
//!   rate_limit "<literal>" [in dev, staging, ...]
//!   audit actor, ctx.now         # subjects required
//! ```
//!
//! ## Column source grammar
//!
//! - `row.<field>` — direct row projection.
//! - `@fn.<name>(arg, arg)` — derived via a registered function. Args
//!   are captured verbatim (comma-separated, no nested parens).
//!
//! Trailing modifiers (`label`, `format`) accept `"..."` literals.
//!
//! ## Cross-module borrows
//!
//! Reuses `parse_command_audit`, `parse_rate_limit_line_body`,
//! `fold_rate_limit_line`, plus the shared lexer helpers
//! `split_first_token` / `take_identifier` / `take_quoted_string` from
//! the parent module (all promoted to `pub(super)` so this sub-file can
//! call them).
//!
//! ## See also
//!
//! - `docs/proposals/report-vocab.md` v0.2 §Linguagem.
//! - `lazuli_ir::nodes::report` — typed lowering target.

use super::super::common::{
    SourceLine, is_trivia, line_error, strip_inline_comment, unquote_lzx_value,
};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::command::parse_command_audit;
use super::{
    AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, fold_rate_limit_line,
    parse_rate_limit_line_body, split_first_token, take_identifier, take_quoted_string,
};

use crate::ast::{
    CommandAudit, PolicyExprAst, RateLimitSpecAst, ReportColumnAst, ReportColumnSourceAst,
    ReportDecl, Span,
};

// -----------------------------------------------------------------------------
// Report vocab — `report <name>` block parser.
//
// Header at AGENT_INDENT_FEATURE_CHILD (indent 2). Children at
// AGENT_INDENT_AGENT_CHILD (indent 4). The `columns` and `audit` blocks
// have grandchildren at AGENT_INDENT_GRANDCHILD (indent 6).
//
// See `docs/proposals/report-vocab.md` v0.2 §Linguagem.
// -----------------------------------------------------------------------------

pub(super) fn parse_report_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(ReportDecl, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("report ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "report header must be `report <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "report header requires a name"));
    }

    let mut source: Option<String> = None;
    let mut columns: Vec<ReportColumnAst> = Vec::new();
    let mut formats: Vec<String> = Vec::new();
    let mut storage: Option<String> = None;
    let mut visibility: Option<String> = None;
    let mut signed_ttl: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut audit: Option<CommandAudit> = None;
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
                "`report` body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            source = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "columns" {
            let (parsed, next) = parse_report_columns_block(lines, i)?;
            columns = parsed;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("formats ") {
            formats = rest
                .split(',')
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("storage ") {
            storage = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("visibility ") {
            visibility = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("signed_ttl ") {
            signed_ttl = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("filename ") {
            filename = Some(unquote_lzx_value(rest.trim()).to_owned());
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
        } else if let Some(rest) = trimmed.strip_prefix("audit ") {
            let (parsed, next) = parse_command_audit(lines, i, rest)?;
            audit = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if trimmed == "audit" {
            return Err(line_error(
                line,
                "`report audit` requires at least one subject (e.g. `audit actor, ctx.now`)",
            ));
        } else {
            return Err(line_error(
                line,
                "`report` children are `source`, `columns`, `formats`, `storage`, `visibility`, `signed_ttl`, `filename`, `policy`, `rate_limit`, or `audit`",
            ));
        }
    }

    let source = source.ok_or_else(|| {
        line_error(
            header,
            "`report` requires a `source <query_ref>` declaration",
        )
    })?;
    if formats.is_empty() {
        return Err(line_error(
            header,
            "`report` requires a `formats <id>, ...` declaration (e.g. `formats csv, xlsx`)",
        ));
    }

    Ok((
        ReportDecl {
            name,
            source,
            columns,
            formats,
            storage,
            visibility,
            signed_ttl,
            filename,
            policy,
            policy_expr,
            rate_limit,
            audit,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Parse the `columns` block of a `report`. Header at indent 4, column
/// rows at indent 6. Each row: `<name> from <column_source> [label "..."] [format "..."]`.
fn parse_report_columns_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Vec<ReportColumnAst>, usize), ParseError> {
    let header = &lines[start];
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let mut columns: Vec<ReportColumnAst> = Vec::new();
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
                "`report columns` rows use six-space indentation",
            ));
        }

        columns.push(parse_report_column(line)?);
        i += 1;
    }

    Ok((columns, i))
}

/// Parse one column row inside `report.columns`.
/// Grammar: `<name> from <column_source> [label "..."] [format "..."]`.
/// `column_source` is `row.<field>` or `@fn.<name>(arg, arg)`.
fn parse_report_column(line: &SourceLine<'_>) -> Result<ReportColumnAst, ParseError> {
    let trimmed = strip_inline_comment(line.text.trim_start()).trim_end();
    // Split off the column name first.
    let (name, rest) = split_first_token(trimmed).ok_or_else(|| {
        line_error(
            line,
            "report column row must be `<name> from <source> [label \"...\"] [format \"...\"]`",
        )
    })?;
    if name.is_empty() {
        return Err(line_error(line, "report column requires a name"));
    }

    let rest = rest.trim_start();
    let after_from = rest.strip_prefix("from ").ok_or_else(|| {
        line_error(
            line,
            "report column requires `from row.<field>` or `from @fn.<name>(args)`",
        )
    })?;

    // The column source extends through `row.<field>` or
    // `@fn.<name>(args...)`. Find the end of the source token, then
    // scan the tail for optional `label "..."` / `format "..."`.
    let (source, tail) = parse_report_column_source(after_from.trim_start(), line)?;

    let mut label: Option<String> = None;
    let mut format: Option<String> = None;
    let mut tail = tail.trim_start();
    while !tail.is_empty() {
        if let Some(rest) = tail.strip_prefix("label ") {
            let (value, next) = take_quoted_string(rest.trim_start(), line)?;
            label = Some(value);
            tail = next.trim_start();
        } else if let Some(rest) = tail.strip_prefix("format ") {
            let (value, next) = take_quoted_string(rest.trim_start(), line)?;
            format = Some(value);
            tail = next.trim_start();
        } else {
            return Err(line_error(
                line,
                "report column trailing modifiers are `label \"...\"` and `format \"...\"`",
            ));
        }
    }

    Ok(ReportColumnAst {
        name: name.to_owned(),
        source,
        label,
        format,
        span: Span::new(line.start, line.end),
    })
}

/// Parse a column source token: `row.<field>` or `@fn.<name>(arg, arg)`.
/// Returns the parsed source plus the remaining tail of the line.
fn parse_report_column_source<'a>(
    input: &'a str,
    line: &SourceLine<'_>,
) -> Result<(ReportColumnSourceAst, &'a str), ParseError> {
    if let Some(rest) = input.strip_prefix("row.") {
        let (field, tail) = take_identifier(rest);
        if field.is_empty() {
            return Err(line_error(
                line,
                "report column source `row.` requires a field identifier",
            ));
        }
        return Ok((ReportColumnSourceAst::RowField(field.to_owned()), tail));
    }

    if let Some(rest) = input.strip_prefix("@fn.") {
        let (name, after_name) = take_identifier(rest);
        if name.is_empty() {
            return Err(line_error(
                line,
                "report column source `@fn.` requires a function name",
            ));
        }
        let after_name = after_name.trim_start();
        let after_paren = after_name.strip_prefix('(').ok_or_else(|| {
            line_error(
                line,
                "report column source `@fn.<name>(args)` requires parentheses",
            )
        })?;
        let close_idx = after_paren.find(')').ok_or_else(|| {
            line_error(
                line,
                "report column source `@fn.<name>(...)` is missing the closing parenthesis",
            )
        })?;
        let args_text = &after_paren[..close_idx];
        let tail = &after_paren[close_idx + 1..];
        let args: Vec<String> = args_text
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok((
            ReportColumnSourceAst::FnCall {
                name: name.to_owned(),
                args,
            },
            tail,
        ));
    }

    Err(line_error(
        line,
        "report column source must be `row.<field>` or `@fn.<name>(args)`",
    ))
}
