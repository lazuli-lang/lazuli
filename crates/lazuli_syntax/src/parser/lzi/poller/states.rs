//! `poller.states` sub-block — at least 2 entries, each `<name>
//! [initial|intermediate|terminal]`.
//!
//! Extracted from the original monolithic `poller.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, line_error_owned};
use super::super::super::error::ParseError;
use super::super::types::PollerStateAst;
use crate::ast::Span;

pub(super) fn parse_poller_states(
    lines: &[SourceLine<'_>],
    start: usize,
    child_indent: usize,
) -> Result<(Vec<PollerStateAst>, usize), ParseError> {
    let header = &lines[start];
    let grandchild_indent = child_indent + 2;
    let mut states: Vec<PollerStateAst> = Vec::new();
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();
        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= child_indent {
            break;
        }
        if line.indent != grandchild_indent {
            return Err(line_error(
                line,
                "`states` body uses one indentation level deeper than the header",
            ));
        }

        let mut parts = trimmed.split_whitespace();
        let name = parts
            .next()
            .ok_or_else(|| line_error(line, "state entry requires a name"))?
            .to_owned();
        let kind_keyword = match parts.next() {
            None => None,
            Some(k @ ("initial" | "intermediate" | "terminal")) => Some(k.to_owned()),
            Some(other) => {
                return Err(line_error_owned(
                    line,
                    format!(
                        "state kind must be `initial`, `intermediate`, or `terminal` (got `{other}`)"
                    ),
                ));
            }
        };
        if parts.next().is_some() {
            return Err(line_error(
                line,
                "state entry accepts at most one kind modifier (initial | intermediate | terminal)",
            ));
        }
        states.push(PollerStateAst {
            name,
            kind_keyword,
            span: Span::new(line.start, line.end),
        });
        i += 1;
    }

    if states.len() < 2 {
        return Err(line_error(
            header,
            "poller `states` requires at least 2 entries",
        ));
    }

    Ok((states, i))
}
