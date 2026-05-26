//! Shared sub-block parsers used by every `query.*` shape — params,
//! scope override, indented capture list, and search.
//!
//! Extracted from the original monolithic `query.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::super::error::ParseError;
use super::super::command::split_command_input_modifiers;
use super::super::field_constraints::extract_field_constraints;

use crate::ast::{CommandInputSlot, QuerySearch, Span};

pub(super) fn parse_query_params_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(Vec<CommandInputSlot>, usize), ParseError> {
    let mut slots: Vec<CommandInputSlot> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "query `params` children use the deepest indentation level",
            ));
        }
        let (name_part, type_part) = trimmed.split_once(':').ok_or_else(|| {
            line_error(
                line,
                "query `params` slots use `<name>: <Type> [required|optional]`",
            )
        })?;
        let name = name_part.trim();
        if name.is_empty() {
            return Err(line_error(
                line,
                "query `params` slot requires a name before `:`",
            ));
        }
        // L0 #3 §10 — query params share the inline-constraint catalog
        // with command inputs / resource fields.
        let (after_constraints, constraints) = extract_field_constraints(line, type_part.trim())?;
        let (type_text, required, optional) =
            split_command_input_modifiers(after_constraints.trim());
        slots.push(CommandInputSlot {
            name: name.to_owned(),
            type_text,
            required,
            optional,
            constraints,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((slots, i))
}

pub(super) fn parse_query_scope_override_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> Result<(Option<String>, Vec<String>, usize), ParseError> {
    let mut reason: Option<String> = None;
    let mut assignments: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`scope override` children use the deepest indentation level",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("reason ") {
            reason = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            assignments.push(trimmed.to_owned());
        }
        i += 1;
    }
    Ok((reason, assignments, i))
}

pub(super) fn parse_query_indented_block(
    lines: &[SourceLine<'_>],
    start: usize,
    grandchild_indent: usize,
) -> (Vec<String>, usize) {
    let mut out: Vec<String> = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        out.push(trimmed.to_owned());
        i += 1;
    }
    (out, i)
}

pub(super) fn parse_query_search(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    grandchild_indent: usize,
) -> Result<(QuerySearch, usize), ParseError> {
    let header = &lines[start];
    let (source, fields) = rest.split_once(" over ").ok_or_else(|| {
        line_error(
            header,
            "`search` requires `<path> over <field>, <field>` (e.g. `search params.search over name, email`)",
        )
    })?;
    let fields: Vec<String> = fields
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();
    let mut mode: Option<String> = None;
    let mut i = start + 1;
    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent < grandchild_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`search` children use the deepest indentation level",
            ));
        }
        if let Some(rest) = trimmed.strip_prefix("mode ") {
            mode = Some(rest.trim().to_owned());
            i += 1;
        } else {
            return Err(line_error(line, "`search` children are `mode <kind>` only"));
        }
    }
    Ok((
        QuerySearch {
            source: source.trim().to_owned(),
            fields,
            mode,
            span: Span::new(header.start, header.end),
        },
        i,
    ))
}
