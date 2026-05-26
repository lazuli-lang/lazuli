//! Flat scale-style design groups — `space` / `radius` / `breakpoint`
//! (`ScaleTokenAst`), `shadow` (`ShadowTokenAst`), `z` (`ZTokenAst`),
//! and `custom` (`CustomTokenAst`).
//!
//! Extracted from the original monolithic `design.rs`. These groups
//! share the named-value block parser; `custom` adds optional `dark`
//! overrides via the color helper.

use super::super::super::common::{SourceLine, is_trivia, line_error, strip_inline_comment};
use super::super::super::error::ParseError;
use super::color::parse_color_value_with_dark;
use super::helpers::{parse_design_named_value_block, split_design_name};
use crate::ast::{CustomTokenAst, ScaleTokenAst, ShadowTokenAst, Span, ZTokenAst};

/// Parse the body of a flat `<group>` like `space` / `radius` /
/// `breakpoint`, where each child line is `<name> <value>`.
pub(super) fn parse_design_scale_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ScaleTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ScaleTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse `shadow` body: each child is `<name> "<value>"` where the value is
/// a CSS box-shadow string (lowering validates single-layer).
pub(super) fn parse_design_shadow_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ShadowTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ShadowTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse `z` body: each child line is `<name> <integer>`.
pub(super) fn parse_design_z_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<ZTokenAst>, usize), ParseError> {
    let entries = parse_design_named_value_block(lines, header_index, child_indent)?;
    Ok((
        entries
            .0
            .into_iter()
            .map(|(name, value)| ZTokenAst { name, value })
            .collect(),
        entries.1,
    ))
}

/// Parse `custom` body: each child line is `<kebab-name> "<hex>" [dark "<hex>"]`.
/// Flat sub-grammar — no state sub-blocks. Lowering enforces hex validity
/// + reserved-name + collision rules. See
/// `docs/proposals/design-tokens-custom.md` §2.
pub(super) fn parse_design_custom_group(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(Vec<CustomTokenAst>, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let mut entries: Vec<CustomTokenAst> = Vec::new();
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
                "custom entries use one indentation level deeper than the `custom` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let (name, after) = split_design_name(line, trimmed)?;
        let after = after.trim();
        if after.is_empty() {
            return Err(line_error(
                line,
                "custom entry requires a hex value (e.g. `chat-bubble \"#dcf8c6\"`)",
            ));
        }
        let (value, dark) = parse_color_value_with_dark(line, after)?;
        entries.push(CustomTokenAst {
            name,
            value,
            dark,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }
    Ok((entries, i))
}
