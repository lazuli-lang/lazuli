//! `design.typography` group — family / scale / weight / tracking
//! sub-groups.
//!
//! Extracted from the original monolithic `design.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, line_error_owned, strip_inline_comment};
use super::super::super::error::ParseError;
use super::helpers::{parse_design_named_value_block, parse_design_scale_block};
use crate::ast::{FamilyTokenAst, TrackingTokenAst, TypographyAst, WeightTokenAst};

/// Parse `typography` body: family / scale / weight / tracking sub-groups.
pub(super) fn parse_design_typography(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(TypographyAst, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let entry_indent = child_indent + 2;
    let mut typo = TypographyAst::default();
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
                "typography sub-groups use one indentation level deeper than the `typography` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let sub_header_index = i;
        match trimmed {
            "family" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.families = entries
                    .into_iter()
                    .map(|(name, value)| FamilyTokenAst { name, value })
                    .collect();
                i = next;
            }
            "scale" => {
                let (entries, next) =
                    parse_design_scale_block(lines, sub_header_index, entry_indent)?;
                typo.scale = entries;
                i = next;
            }
            "weight" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.weights = entries
                    .into_iter()
                    .map(|(name, value)| WeightTokenAst { name, value })
                    .collect();
                i = next;
            }
            "tracking" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                typo.tracking = entries
                    .into_iter()
                    .map(|(name, value)| TrackingTokenAst { name, value })
                    .collect();
                i = next;
            }
            other => {
                return Err(line_error_owned(
                    line,
                    format!(
                        "typography sub-groups are `family`, `scale`, `weight`, or `tracking` (got `{other}`)"
                    ),
                ));
            }
        }
    }
    Ok((typo, i))
}
