//! `auth mfa <method>` sub-block — enroll / verify / adapter.
//!
//! Extracted from the original monolithic `auth.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};
use crate::ast::{AuthMfa, Span};

pub(super) fn parse_auth_mfa(
    lines: &[SourceLine<'_>],
    start: usize,
    method: String,
) -> Result<(AuthMfa, usize), ParseError> {
    let header = &lines[start];
    let mut enroll: Option<String> = None;
    let mut verify: Option<String> = None;
    let mut adapter: Option<String> = None;
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
                "`auth mfa` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("enroll ") {
            enroll = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("verify ") {
            verify = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("adapter ") {
            adapter = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`auth mfa` children are `enroll`, `verify`, or `adapter`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let enroll = enroll.ok_or_else(|| {
        line_error(
            header,
            "`auth mfa` requires an `enroll @fn.<name>` declaration",
        )
    })?;
    let verify = verify.ok_or_else(|| {
        line_error(
            header,
            "`auth mfa` requires a `verify @validator.<name>` declaration",
        )
    })?;

    Ok((
        AuthMfa {
            method,
            enroll,
            verify,
            adapter,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
