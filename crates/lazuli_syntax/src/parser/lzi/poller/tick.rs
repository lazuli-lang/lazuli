//! `poller.tick` leaf — `tick every <duration> [batch <int>]`.
//!
//! Extracted from the original monolithic `poller.rs`.

use super::super::super::common::{SourceLine, line_error};
use super::super::super::error::ParseError;
use super::super::types::PollerTickAst;
use crate::ast::Span;

pub(super) fn parse_poller_tick(
    line: &SourceLine<'_>,
    rest: &str,
) -> Result<PollerTickAst, ParseError> {
    let rest = rest.trim();
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "every" {
        return Err(line_error(
            line,
            "`tick` requires `tick every <duration> [batch <int>]`",
        ));
    }
    let every = parts[1].to_owned();
    let mut batch: Option<u32> = None;
    if parts.len() > 2 {
        if parts.len() != 4 || parts[2] != "batch" {
            return Err(line_error(
                line,
                "`tick` modifier is `batch <int>` after `every <duration>`",
            ));
        }
        let parsed = parts[3]
            .parse::<u32>()
            .map_err(|_| line_error(line, "`batch` requires a non-negative integer"))?;
        batch = Some(parsed);
    }
    Ok(PollerTickAst {
        every,
        batch,
        span: Span::new(line.start, line.end),
    })
}
