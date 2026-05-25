//! `record <Name>` feature child — DTO / view-model vocabulary.
//!
//! A record is a closed struct type used for command inputs, query
//! results, event payloads, and inter-feature DTO shapes — anywhere a
//! resource is too heavy or domain-bound. Records have no lifecycle,
//! no persistence, no policies of their own: they're inert data shapes.
//!
//! ## Grammar (closed)
//!
//! ```text
//! record <Name>
//!   <field>: <Type> [required|optional] [default <value>]
//!   <field>: <Type> [discriminator]
//! ```
//!
//! Field rows reuse `parse_resource_field_decl` from the parent module
//! (`pub(super)` re-export) so record and resource share one syntax for
//! typed fields. The analyzer rejects resource-only modifiers
//! (`unique`, `slug`, `@full_text`, etc.) downstream.
//!
//! A `discriminator` modifier marks the variant tag of a closed-enum
//! record; the analyzer pairs it with the corresponding enum lookup.
//!
//! ## See also
//!
//! - `docs/canonical-semantics.md` — `record` grammar.
//! - `lazuli_ir::nodes::record` — typed lowering target.

use super::super::common::{SourceLine, is_trivia, line_error};
use super::super::error::ParseError;
use super::parse_resource_field_decl;

use crate::ast::{RecordDecl, ResourceFieldDecl, Span};

pub(super) fn parse_record_decl(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(RecordDecl, usize), ParseError> {
    let header = &lines[start];
    let trimmed = header.text.trim_start();
    let name = trimmed
        .strip_prefix("record ")
        .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
        .ok_or_else(|| line_error(header, "record header must be `record <Name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "record header requires a name"));
    }
    let header_indent = header.indent;
    let child_indent = header_indent + 2;
    let grandchild_indent = header_indent + 4;

    let mut fields: Vec<ResourceFieldDecl> = Vec::new();
    let mut discriminator_field: Option<String> = None;
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
                "`record` body children use one indentation level deeper than the header",
            ));
        }
        if trimmed.contains(':') {
            let (field, next) = parse_resource_field_decl(lines, i, grandchild_indent)?;
            if field.type_text.contains("discriminator") {
                discriminator_field = Some(field.name.clone());
            }
            fields.push(field);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else {
            return Err(line_error(
                line,
                "`record` children are `<field>: <Type>` lines only",
            ));
        }
    }

    Ok((
        RecordDecl {
            name,
            public_contract: None,
            fields,
            discriminator_field,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
