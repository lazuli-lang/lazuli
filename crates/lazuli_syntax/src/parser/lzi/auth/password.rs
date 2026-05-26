//! `auth password` sub-block — algorithm / hash / verify / rate_limit.
//!
//! Extracted from the original monolithic `auth.rs`. See `auth/mod.rs`
//! for the orchestrating `parse_auth` walker.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::numerics::{fold_rate_limit_line, parse_rate_limit_line_body};
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};
use crate::ast::{AuthPassword, RateLimitSpecAst, Span};

pub(super) fn parse_auth_password(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(AuthPassword, usize), ParseError> {
    let header = &lines[start];
    let mut algorithm: Option<String> = None;
    let mut hash: Option<String> = None;
    let mut verify: Option<String> = None;
    let mut rate_limit: Option<RateLimitSpecAst> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_AGENT_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_GRANDCHILD {
            return Err(line_error(
                line,
                "`auth password` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("algorithm ") {
            algorithm = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("hash ") {
            hash = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("verify ") {
            verify = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("rate_limit ") {
            let (literal, envs) = parse_rate_limit_line_body(line, rest)?;
            fold_rate_limit_line(line, &mut rate_limit, literal, envs)?;
        } else {
            return Err(line_error(
                line,
                "`auth password` children are `algorithm`, `hash`, `verify`, or `rate_limit`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let algorithm = algorithm.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires an `algorithm <name>` declaration",
        )
    })?;
    let hash = hash.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires a `hash @fn.<name>` declaration",
        )
    })?;
    let verify = verify.ok_or_else(|| {
        line_error(
            header,
            "`auth password` requires a `verify @fn.<name>` declaration",
        )
    })?;

    Ok((
        AuthPassword {
            algorithm,
            hash,
            verify,
            rate_limit,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
