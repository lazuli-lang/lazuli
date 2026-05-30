//! Canonical-source formatter for `.lzi` feature blocks.
//!
//! `format_canonical_source` is the single entry point invoked from
//! `Backend::formatting` in `lib.rs`. It re-orders the inner segments
//! of every top-level `feature ...` block into the canonical IR order
//! (`CanonicalBlockKind::rank`) and lightly tidies workflow indentation
//! via [`format_workflow_lines`].
//!
//! Two helpers stay sibling-`pub(crate)` because the test module
//! (`lib_tests.rs`) and other formatters consume them directly:
//! [`format_feature_lines`] (segment-by-segment) and
//! [`format_workflow_lines`] (transition indentation tidy-up).
//!
//! Shared lib.rs helpers consumed here:
//!
//! - [`crate::is_canonical_source`] / [`crate::canonical_block_kind`]
//!   to gate which sources qualify and rank inner segments.
//! - [`crate::leading_spaces`] / [`crate::is_trivia_line`] /
//!   [`crate::is_transition_line`] for indentation discipline.

use crate::diagnostics::canonical_kinds::{CanonicalBlockKind, canonical_block_kind};
use crate::{is_canonical_source, is_transition_line, is_trivia_line, leading_spaces};

pub(crate) fn format_canonical_source(source: &str) -> Option<String> {
    if !is_canonical_source(source) {
        return None;
    }

    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut formatted_lines = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];

        if leading_spaces(line) == 0 && line.trim_start().starts_with("feature ") {
            let start = index;
            index += 1;

            while index < lines.len() {
                let next = lines[index];
                if leading_spaces(next) == 0 && next.trim_start().starts_with("feature ") {
                    break;
                }
                index += 1;
            }

            formatted_lines.extend(format_feature_lines(&lines[start..index]));
        } else {
            formatted_lines.push(line.to_owned());
            index += 1;
        }
    }

    let mut formatted = formatted_lines.join(newline);
    if source.ends_with('\n') {
        formatted.push_str(newline);
    }

    Some(formatted)
}

#[derive(Debug)]
pub(crate) struct FeatureBlockSegment {
    kind: Option<CanonicalBlockKind>,
    ordinal: usize,
    lines: Vec<String>,
}

pub(crate) fn format_feature_lines(lines: &[&str]) -> Vec<String> {
    let Some((first, rest)) = lines.split_first() else {
        return Vec::new();
    };

    let mut formatted = vec![(*first).to_owned()];
    let mut segments = Vec::new();
    let mut index = 0;

    while index < rest.len() && is_trivia_line(rest[index]) {
        formatted.push(rest[index].to_owned());
        index += 1;
    }

    while index < rest.len() {
        let line = rest[index];
        let kind = if leading_spaces(line) == 2 {
            canonical_block_kind(line.trim_start())
        } else {
            None
        };

        let start = index;
        index += 1;

        // Advance to the next indent-2 canonical block header — same scan
        // whether or not this segment had a recognized `kind`.
        while index < rest.len() {
            let next = rest[index];
            if leading_spaces(next) == 2 && canonical_block_kind(next.trim_start()).is_some() {
                break;
            }
            index += 1;
        }

        segments.push(FeatureBlockSegment {
            kind,
            ordinal: segments.len(),
            lines: rest[start..index]
                .iter()
                .map(|line| (*line).to_owned())
                .collect(),
        });
    }

    segments.sort_by_key(|segment| {
        (
            segment
                .kind
                .map(CanonicalBlockKind::rank)
                .unwrap_or(u8::MAX),
            segment.ordinal,
        )
    });

    for segment in segments {
        if segment.kind == Some(CanonicalBlockKind::Workflow) {
            formatted.extend(format_workflow_lines(segment.lines));
        } else {
            formatted.extend(segment.lines);
        }
    }

    formatted
}

pub(crate) fn format_workflow_lines(lines: Vec<String>) -> Vec<String> {
    let mut formatted = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = &lines[index];
        formatted.push(line.to_owned());

        if is_transition_line(line.trim_start()) {
            let transition_indent = leading_spaces(line);
            let mut next_non_blank = index + 1;

            while next_non_blank < lines.len() && lines[next_non_blank].trim().is_empty() {
                next_non_blank += 1;
            }

            if next_non_blank > index + 1
                && next_non_blank < lines.len()
                && leading_spaces(&lines[next_non_blank]) > transition_indent
            {
                index = next_non_blank;
                continue;
            }
        }

        index += 1;
    }

    formatted
}
