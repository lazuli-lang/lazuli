//! `view list` search block — flat-columns or `segmented` mode.
//!
//! The `search` line carries one of two shapes:
//!
//! ```text
//! search title, description           # flat columns mode
//! search segmented                    # opens a child block
//!   field name binds source.name
//!   field tag binds filters.tags
//!   free text into source.q
//! ```
//!
//! `segmented` is mutually exclusive with the inline list — the
//! dispatcher rejects `search segmented foo` outright. Inside
//! `segmented`, each `field` declares a typed binding to either a
//! `filters.<name>`, `source.<name>`, or `selection` scalar. A single
//! optional `free text into <BindingRef>` carries the catch-all
//! input.
//!
//! `parse_binding_ref` is the shared validator for those binding
//! references; it stays inside this module because no other surface
//! sub-construct currently consumes it.

use crate::ast::{BindingRefAst, SearchDeclAst, SearchFieldAst, SearchModeAst, Span};

use super::super::super::common::{
    SourceLine, is_trivia, line_error, line_error_owned, split_lzx_list, strip_inline_comment,
};
use super::super::super::error::ParseError;

pub(super) fn parse_view_search_decl(
    lines: &[SourceLine<'_>],
    start: usize,
    rest: &str,
    body_indent: usize,
) -> Result<(SearchDeclAst, usize), ParseError> {
    let header = &lines[start];
    if rest == "segmented" {
        parse_view_segmented_search(lines, start, body_indent)
    } else if rest.starts_with("segmented ") {
        Err(line_error(
            header,
            "the `segmented` form takes no inline list — use child `field` declarations",
        ))
    } else {
        Ok((
            SearchDeclAst {
                mode: SearchModeAst::Columns(split_lzx_list(rest)),
                fields: Vec::new(),
                free_text_target: None,
                span: Span::new(header.start, header.end),
            },
            start + 1,
        ))
    }
}

fn parse_view_segmented_search(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
) -> Result<(SearchDeclAst, usize), ParseError> {
    let header = &lines[start];
    let child_indent = body_indent + 2;
    let mut fields: Vec<SearchFieldAst> = Vec::new();
    let mut free_text_target = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let raw = line.text.trim_start();
        if is_trivia(raw) {
            i += 1;
            continue;
        }
        if line.indent <= body_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "`search segmented` child lines use one indentation level deeper than `search segmented`",
            ));
        }
        let trimmed = strip_inline_comment(raw).trim_end();
        if let Some(rest) = trimmed.strip_prefix("field ") {
            let field = parse_view_search_field(line, rest.trim())?;
            if fields.iter().any(|existing| existing.key == field.key) {
                return Err(line_error_owned(
                    line,
                    format!(
                        "`search segmented` declares field `{}` more than once",
                        field.key
                    ),
                ));
            }
            fields.push(field);
        } else if let Some(rest) = trimmed.strip_prefix("free text into ") {
            if free_text_target.is_some() {
                return Err(line_error(
                    line,
                    "`search segmented` declares `free text into` at most once",
                ));
            }
            free_text_target = Some(parse_binding_ref(line, rest.trim())?);
        } else {
            return Err(line_error(
                line,
                "`search segmented` children are `field <key> binds <BindingRef>` or `free text into <BindingRef>`",
            ));
        }
        last_end = line.end;
        i += 1;
    }

    Ok((
        SearchDeclAst {
            mode: SearchModeAst::Segmented,
            fields,
            free_text_target,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

fn parse_view_search_field(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<SearchFieldAst, ParseError> {
    let Some((key, target)) = rest.split_once(" binds ") else {
        return Err(line_error(
            line,
            "`search segmented` fields use `field <key> binds <BindingRef>`",
        ));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(line_error(
            line,
            "`search segmented` field key cannot be empty",
        ));
    }
    Ok(SearchFieldAst {
        key: key.to_owned(),
        binds_to: parse_binding_ref(line, target.trim())?,
        span: Span::new(line.start, line.end),
    })
}

fn parse_binding_ref(line: &SourceLine<'_>, raw: &str) -> Result<BindingRefAst, ParseError> {
    if raw == "selection" {
        return Ok(BindingRefAst::SelectionScalar);
    }
    if let Some(name) = raw.strip_prefix("filters.") {
        if !name.is_empty() {
            return Ok(BindingRefAst::Filter {
                name: name.to_owned(),
            });
        }
    }
    if let Some(name) = raw.strip_prefix("source.") {
        if !name.is_empty() {
            return Ok(BindingRefAst::SourceInput {
                name: name.to_owned(),
            });
        }
    }
    Err(line_error(
        line,
        "binding references are `filters.<name>`, `source.<name>`, or `selection`",
    ))
}
