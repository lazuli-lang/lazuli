//! `design.motion` group — duration + easing sub-groups.
//!
//! Extracted from the original monolithic `design.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, line_error_owned, strip_inline_comment};
use super::super::super::error::ParseError;
use super::helpers::parse_design_named_value_block;
use crate::ast::{EasingTokenAst, MotionAst, ScaleTokenAst};

/// Parse the body of `motion` (duration + easing sub-groups).
pub(super) fn parse_design_motion(
    lines: &[SourceLine<'_>],
    header_index: usize,
    child_indent: usize,
) -> Result<(MotionAst, usize), ParseError> {
    let header_indent = lines[header_index].indent;
    let entry_indent = child_indent + 2;
    let mut motion = MotionAst::default();
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
                "motion sub-groups use one indentation level deeper than the `motion` header",
            ));
        }
        let trimmed = strip_inline_comment(trimmed_raw).trim_end();
        let sub_header_index = i;
        match trimmed {
            "duration" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                motion.durations = entries
                    .into_iter()
                    .map(|(name, value)| ScaleTokenAst { name, value })
                    .collect();
                i = next;
            }
            "easing" => {
                let (entries, next) =
                    parse_design_named_value_block(lines, sub_header_index, entry_indent)?;
                motion.easings = entries
                    .into_iter()
                    .map(|(name, value)| EasingTokenAst { name, value })
                    .collect();
                i = next;
            }
            other => {
                return Err(line_error_owned(
                    line,
                    format!("motion sub-groups are `duration` or `easing` (got `{other}`)"),
                ));
            }
        }
    }
    Ok((motion, i))
}
