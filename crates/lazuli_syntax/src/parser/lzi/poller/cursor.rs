//! `poller.cursor` sub-block — `eligible_when <a>, <b>` + `attempts <field>`.
//!
//! Extracted from the original monolithic `poller.rs`. See
//! `poller/mod.rs` for the orchestrating walker.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::types::PollerCursorAst;
use crate::ast::Span;

pub(super) fn parse_poller_cursor(
    lines: &[SourceLine<'_>],
    start: usize,
    child_indent: usize,
) -> Result<(PollerCursorAst, usize), ParseError> {
    let header = &lines[start];
    let grandchild_indent = child_indent + 2;
    let mut next_at_field: Option<String> = None;
    let mut resolved_at_field: Option<String> = None;
    let mut attempts_field: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= child_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`cursor` body uses one indentation level deeper than the header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("eligible_when ") {
            if next_at_field.is_some() {
                return Err(line_error(
                    line,
                    "`cursor` declares `eligible_when` at most once",
                ));
            }
            let mut parts = rest.split(',').map(str::trim);
            let na = parts.next().unwrap_or("");
            let ra = parts.next().unwrap_or("");
            if na.is_empty() || ra.is_empty() || parts.next().is_some() {
                return Err(line_error(
                    line,
                    "`eligible_when` requires two field names: \
                     `eligible_when <next_at_field>, <resolved_at_field>`",
                ));
            }
            next_at_field = Some(na.to_owned());
            resolved_at_field = Some(ra.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("attempts ") {
            if attempts_field.is_some() {
                return Err(line_error(
                    line,
                    "`cursor` declares `attempts` at most once",
                ));
            }
            let val = rest.trim();
            if val.is_empty() {
                return Err(line_error(line, "`attempts` requires a field name"));
            }
            attempts_field = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        return Err(line_error(
            line,
            "`cursor` body accepts only `eligible_when <a>, <b>` and `attempts <field>`",
        ));
    }

    let next_at_field = next_at_field
        .ok_or_else(|| line_error(header, "`cursor` requires an `eligible_when` child"))?;
    let resolved_at_field = resolved_at_field.expect("resolved_at parsed alongside next_at");
    let attempts_field = attempts_field
        .ok_or_else(|| line_error(header, "`cursor` requires an `attempts <field>` child"))?;

    Ok((
        PollerCursorAst {
            next_at_field,
            resolved_at_field,
            attempts_field,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
