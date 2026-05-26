//! `design.color` group — named tokens with optional state sub-blocks
//! (`base`/`hover`/`active`/`foreground`), each supporting a `dark`
//! override.
//!
//! Extracted from the original monolithic `design.rs`.

use super::super::super::common::{
    SourceLine, find_top_level_token, is_trivia, line_error, strip_inline_comment,
};
use super::super::super::error::ParseError;
use super::helpers::{split_design_name, strip_design_quotes};
use crate::ast::{ColorStateAst, ColorTokenAst, Span};

/// Parse the body of `color` (group of named entries, each either flat
/// `<name> "<hex>"` or sub-block with state lines).
pub(super) fn parse_design_color_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ColorTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let state_indent = child_indent + 2;
    let mut colors: Vec<ColorTokenAst> = Vec::new();
    let mut i = header_index + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= header_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "color entries use one indentation level deeper than the `color` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;

        // Disambiguate flat vs sub-block: if `after` is empty (after stripping
        // trailing whitespace), this is a sub-block header; otherwise the
        // remainder is the flat-form hex value (with optional `dark <hex>`).
        let after = after.trim();
        if after.is_empty() {
            let entry_start = line.start;
            let (states, next, last_end) =
                parse_design_color_states(lines, i + 1, state_indent, child_indent)?;
            if states.is_empty() {
                return Err(line_error(
                    line,
                    "color sub-block requires at least one of `base`, `hover`, `active`, `foreground`",
                ));
            }
            colors.push(ColorTokenAst {
                name,
                states,
                span: Span::new(entry_start, last_end),
            });
            i = next;
        } else {
            // Flat form: `<name> "<hex>" [dark "<hex>"]`. Treat the value as
            // an implicit `base` state.
            let (value, dark) = parse_color_value_with_dark(line, after)?;
            colors.push(ColorTokenAst {
                name,
                states: vec![ColorStateAst {
                    kind: "base".to_owned(),
                    value,
                    dark,
                }],
                span: Span::new(line.start, line.end),
            });
            i += 1;
        }
    }

    Ok((colors, i))
}

/// Parse a sequence of `base | hover | active | foreground "<hex>" [dark
/// "<hex>"]` lines at `state_indent` until we leave the parent block.
fn parse_design_color_states(
    lines: &[SourceLine<'_>],
    start: usize,
    state_indent: usize,
    parent_indent: usize,
) -> Result<(Vec<ColorStateAst>, usize, usize), ParseError> {
    let mut states: Vec<ColorStateAst> = Vec::new();
    let mut i = start;
    let mut last_end = if start == 0 { 0 } else { lines[start - 1].end };
    while i < lines.len() {
        let line = &lines[i];
        let trimmed_raw = line.text.trim_start();
        if is_trivia(trimmed_raw) {
            i += 1;
            continue;
        }
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != state_indent {
            return Err(line_error(
                line,
                "color state entries use one indentation level deeper than the color sub-block name",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (kind, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        if after.is_empty() {
            return Err(line_error(
                line,
                "color state requires a hex value (e.g. `base \"#7c3aed\"`)",
            ));
        }
        let (value, dark) = parse_color_value_with_dark(line, after)?;
        states.push(ColorStateAst { kind, value, dark });
        last_end = line.end;
        i += 1;
    }
    Ok((states, i, last_end))
}

/// Parse the `<value> [dark <value>]` tail of a color line. The values
/// are typically quoted hex literals; we preserve quotes verbatim so the
/// analyzer can validate.
pub(super) fn parse_color_value_with_dark(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<(String, Option<String>), ParseError> {
    let rest = rest.trim();
    // `dark ` may appear after the primary value; we honor a top-level
    // (paren-depth-0) match so embedded `dark` inside an unlikely literal
    // stays put. In practice values are short hex strings.
    if let Some(idx) = find_top_level_token(rest, " dark ") {
        let primary = rest[..idx].trim();
        let dark_part = rest[idx + " dark ".len()..].trim();
        if primary.is_empty() {
            return Err(line_error(
                line,
                "color value missing before `dark` modifier",
            ));
        }
        if dark_part.is_empty() {
            return Err(line_error(
                line,
                "`dark` modifier requires a hex value (e.g. `dark \"#09090b\"`)",
            ));
        }
        Ok((
            strip_design_quotes(primary).to_owned(),
            Some(strip_design_quotes(dark_part).to_owned()),
        ))
    } else {
        Ok((strip_design_quotes(rest).to_owned(), None))
    }
}
