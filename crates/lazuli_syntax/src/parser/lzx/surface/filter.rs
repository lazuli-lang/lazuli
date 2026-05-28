//! `view list` filters block + typed filter declarations.
//!
//! Two surfaces live here:
//!
//! ```text
//! filter status, owner_id              # inline (state.filter, untyped)
//! filters
//!   status: list of UserStatus from query     # typed (state.filters)
//!   owner_id: UserId
//! ```
//!
//! The inline `filter <list>` line is handled by the dispatcher in
//! `surface/mod.rs` (see `parse_view_filter_line`) and stores plain
//! strings. The `filters` block, in contrast, accepts typed
//! declarations with optional cardinality (`list of <T>`) and an
//! optional `from query` clause that asks the runtime to URL-sync
//! the value.
//!
//! `parse_filters_block` mutates the shared `ViewBodyState` so it
//! reuses `super::ViewBodyState` directly. `parse_filter_decl` is a
//! pure validator and could in principle be exposed wider, but
//! nothing outside this module currently consumes it.

use super::super::super::common::{
    SourceLine, is_lzx_bare_ident, is_trivia, line_error, line_error_owned, strip_inline_comment,
};
use super::super::super::error::ParseError;
use super::ViewBodyState;
use crate::ast::{FilterCardinalityAst, FilterDeclAst, Span};

pub(crate) fn parse_filters_block(
    lines: &[SourceLine<'_>],
    start: usize,
    body_indent: usize,
    state: &mut ViewBodyState,
) -> Result<(usize, usize), ParseError> {
    let header = &lines[start];
    if state.has_filters_block {
        return Err(line_error(
            header,
            "view list declares `filters` at most once",
        ));
    }
    state.has_filters_block = true;

    let child_indent = body_indent + 2;
    let mut block_filters = Vec::new();
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
                "filters declarations use one indentation level deeper than the `filters` header",
            ));
        }

        let trimmed = strip_inline_comment(raw).trim_end();
        let filter = parse_filter_decl(line, trimmed)?;
        if block_filters
            .iter()
            .any(|existing: &FilterDeclAst| existing.name == filter.name)
        {
            return Err(line_error_owned(
                line,
                format!("duplicate filter `{}` in `filters` block", filter.name),
            ));
        }
        last_end = line.end;
        block_filters.push(filter);
        i += 1;
    }

    if block_filters.is_empty() {
        return Err(line_error(
            header,
            "filters block requires at least one filter declaration",
        ));
    }

    state.filters.extend(block_filters);
    Ok((i, last_end))
}

fn parse_filter_decl(line: &SourceLine<'_>, value: &str) -> Result<FilterDeclAst, ParseError> {
    let (name_raw, type_raw) = value.split_once(':').ok_or_else(|| {
        line_error(
            line,
            "filter declaration must be `<name>: [list of] <Type> [from query]`",
        )
    })?;
    let name = name_raw.trim().to_owned();
    if !is_lzx_bare_ident(&name) {
        return Err(line_error_owned(
            line,
            format!(
                "filter name `{}` must start with a letter and contain only letters, digits, or `_`",
                name
            ),
        ));
    }

    let mut rest = type_raw.trim();
    let mut url_sync = false;
    if let Some((head, source)) = rest.rsplit_once(" from ") {
        if source.trim() != "query" {
            return Err(line_error(line, "filter URL source must be `from query`"));
        }
        rest = head.trim();
        url_sync = true;
    }

    // GAP-UX-07 — `date_range` is a closed cardinality keyword. Bare
    // (`<name>: date_range`) defaults the backing type to `Date`; an
    // explicit type (`<name>: date_range DateTime`) overrides it. Either
    // way it lowers to a paired from/to picker (`<name>_from`/`<name>_to`).
    let (cardinality, type_ref) = if rest == "date_range" {
        (FilterCardinalityAst::DateRange, "Date")
    } else if let Some(type_ref) = rest.strip_prefix("date_range ") {
        (FilterCardinalityAst::DateRange, type_ref.trim())
    } else if let Some(type_ref) = rest.strip_prefix("list of ") {
        (FilterCardinalityAst::Multi, type_ref.trim())
    } else {
        (FilterCardinalityAst::Single, rest)
    };
    if type_ref.is_empty() {
        return Err(line_error(line, "filter declaration requires a type"));
    }
    if !is_lzx_bare_ident(type_ref) {
        return Err(line_error_owned(
            line,
            format!("filter type `{}` must be a bare identifier", type_ref),
        ));
    }

    Ok(FilterDeclAst {
        name,
        type_ref: type_ref.to_owned(),
        cardinality,
        url_sync,
        span: Span::new(line.start, line.end),
    })
}
