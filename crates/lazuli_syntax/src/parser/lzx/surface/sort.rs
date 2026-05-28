//! `view list` sort block — `sort by <fields…>` + `default <field> <asc|desc>`.
//!
//! Mounted from `surface/mod.rs`' view dispatcher when the body sees
//! the bare `sort` keyword. The block declares the allowed sort fields
//! plus a mandatory default. The grammar enforces:
//!
//! ```text
//! sort
//!   by created_at, name
//!   default created_at desc
//! ```
//!
//! `by` and `default` are each at most once; the `default` field must
//! appear in `by`. Both lines are required — omission triggers the
//! `requires a *by* line` / `requires a *default* line` diagnostic so
//! doctor / LSP can surface a single, precise message.

use super::super::super::common::{
    SourceLine, is_trivia, line_error, line_error_owned, split_lzx_list, strip_inline_comment,
};
use super::super::super::error::ParseError;
use crate::ast::{SortDeclAst, SortDirAst, Span};

pub(crate) fn parse_view_sort_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
) -> Result<(SortDeclAst, usize, usize), ParseError> {
    let header = &lines[start];
    let child_indent = body_indent + 2;
    let mut index = start + 1;
    let mut allowed: Option<Vec<String>> = None;
    let mut default: Option<(String, SortDirAst)> = None;
    let mut last_end = header.end;

    while index < lines.len() {
        let line = &lines[index];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            index += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`sort` children use one indentation level deeper than `sort`",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("by ") {
            if allowed.is_some() {
                return Err(line_error(line, "`sort` declares `by` at most once"));
            }
            let fields = split_lzx_list(rest);
            if fields.is_empty() {
                return Err(line_error(line, "`sort by` requires at least one field"));
            }
            allowed = Some(fields);
        } else if let Some(rest) = trimmed.strip_prefix("default ") {
            if default.is_some() {
                return Err(line_error(line, "`sort` declares `default` at most once"));
            }
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() != 2 {
                return Err(line_error(
                    line,
                    "`sort default` uses `default <field> <asc|desc>`",
                ));
            }
            default = Some((parts[0].to_owned(), parse_sort_dir(line, parts[1])?));
        } else {
            return Err(line_error(
                line,
                "`sort` children are `by <field>, ...` or `default <field> <asc|desc>`",
            ));
        }
        last_end = line.end;
        index += 1;
    }

    let allowed = allowed.ok_or_else(|| line_error(header, "`sort` requires a `by` line"))?;
    let (default_field, default_dir) =
        default.ok_or_else(|| line_error(header, "`sort` requires a `default` line"))?;
    if !allowed.iter().any(|field| field == &default_field) {
        return Err(line_error_owned(
            header,
            format!(
                "`sort default` field `{}` must be listed in `sort by`",
                default_field
            ),
        ));
    }

    Ok((
        SortDeclAst {
            allowed,
            default_field,
            default_dir,
            span: Span::new(header.start, last_end),
        },
        index,
        last_end,
    ))
}

fn parse_sort_dir(line: &SourceLine<'_>, value: &str) -> Result<SortDirAst, ParseError> {
    match value {
        "asc" => Ok(SortDirAst::Asc),
        "desc" => Ok(SortDirAst::Desc),
        _ => Err(line_error(
            line,
            "`sort default` dir must be `asc` or `desc`",
        )),
    }
}
