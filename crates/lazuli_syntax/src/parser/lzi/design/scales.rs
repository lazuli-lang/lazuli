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

#[cfg(test)]
mod scales_tests {
    use super::super::parse_design_document;

    #[test]
    fn design_digit_prefix_names_require_quotes() {
        let source = r##"
design example
  space
    "1" 0.25rem
    "2" 0.5rem
  breakpoint
    "2xl" 1536px
    "3xl" 1920px
"##;
        let ast = parse_design_document(source).unwrap();
        assert_eq!(ast.spaces[0].name, "1");
        assert_eq!(ast.spaces[0].value, "0.25rem");
        assert_eq!(ast.spaces[1].name, "2");
        assert_eq!(ast.breakpoints[0].name, "2xl");
        assert_eq!(ast.breakpoints[0].value, "1536px");
        assert_eq!(ast.breakpoints[1].name, "3xl");
    }

    #[test]
    fn design_shadow_quoted_strings_preserved_intact() {
        let source = r##"
design example
  shadow
    sm "0 1px 2px 0 rgb(0 0 0 / 0.05)"
    base "0 1px 3px 0 rgb(0 0 0 / 0.1)"
"##;
        let ast = parse_design_document(source).unwrap();
        assert_eq!(ast.shadows.len(), 2);
        assert_eq!(ast.shadows[0].name, "sm");
        assert_eq!(ast.shadows[0].value, "0 1px 2px 0 rgb(0 0 0 / 0.05)");
        assert_eq!(ast.shadows[1].value, "0 1px 3px 0 rgb(0 0 0 / 0.1)");
    }

    #[test]
    fn design_z_values_parsed_as_strings() {
        let source = r##"
design example
  z
    docked 10
    modal 1300
"##;
        let ast = parse_design_document(source).unwrap();
        assert_eq!(ast.z_indices.len(), 2);
        assert_eq!(ast.z_indices[0].name, "docked");
        assert_eq!(ast.z_indices[0].value, "10");
        assert_eq!(ast.z_indices[1].value, "1300");
    }

    // ── `custom` 9th meta-group ──────────────────────────────────────────────
    // Per `docs/proposals/design-tokens-custom.md` §2.

    #[test]
    fn design_custom_group_parses_flat_entries() {
        let source = r##"
design the canonical pilot
  custom
    chat-bubble-mine "#dcf8c6"
    chat-bubble-other "#ffffff"
    map-marker-active "#ff5722"
"##;
        let ast = parse_design_document(source).expect("parses");
        assert_eq!(ast.custom.len(), 3);
        assert_eq!(ast.custom[0].name, "chat-bubble-mine");
        assert_eq!(ast.custom[0].value, "#dcf8c6");
        assert!(ast.custom[0].dark.is_none());
        assert_eq!(ast.custom[1].name, "chat-bubble-other");
        assert_eq!(ast.custom[2].name, "map-marker-active");
    }

    #[test]
    fn design_custom_entry_captures_dark_suffix() {
        let source = r##"
design the canonical pilot
  custom
    chat-bubble-mine "#dcf8c6" dark "#005c4b"
    chat-bubble-other "#ffffff" dark "#202c33"
"##;
        let ast = parse_design_document(source).expect("parses");
        assert_eq!(ast.custom.len(), 2);
        assert_eq!(ast.custom[0].value, "#dcf8c6");
        assert_eq!(ast.custom[0].dark.as_deref(), Some("#005c4b"));
        assert_eq!(ast.custom[1].dark.as_deref(), Some("#202c33"));
    }

    #[test]
    fn design_custom_group_coexists_with_color_group() {
        let source = r##"
design the canonical pilot
  color
    primary "#28bbdd"
  custom
    chat-bubble "#dcf8c6"
"##;
        let ast = parse_design_document(source).expect("parses");
        assert_eq!(ast.colors.len(), 1);
        assert_eq!(ast.colors[0].name, "primary");
        assert_eq!(ast.custom.len(), 1);
        assert_eq!(ast.custom[0].name, "chat-bubble");
    }

    #[test]
    fn design_custom_entry_requires_value() {
        let source = r##"
design the canonical pilot
  custom
    chat-bubble
"##;
        let err = parse_design_document(source).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("custom entry requires"), "got: {msg}");
    }

    #[test]
    fn design_custom_empty_block_skips_cleanly() {
        // `custom` header with no children should not crash; the field
        // remains an empty Vec.
        let source = r##"
design the canonical pilot
  custom
  color
    primary "#28bbdd"
"##;
        let ast = parse_design_document(source).expect("parses");
        assert!(ast.custom.is_empty());
        assert_eq!(ast.colors.len(), 1);
    }

    #[test]
    fn design_without_custom_group_still_parses() {
        // Regression: pre-Z2 `design.lzi` blocks must keep parsing.
        let source = r##"
design legacy
  color
    primary "#28bbdd"
"##;
        let ast = parse_design_document(source).expect("parses");
        assert!(ast.custom.is_empty());
        assert_eq!(ast.colors.len(), 1);
    }
}
