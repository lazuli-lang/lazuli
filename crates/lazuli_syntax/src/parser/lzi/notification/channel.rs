//! `channel <name>` block — realtime bucket cycle MVP sibling of
//! `notification`. Closed three-child body
//! (`tenant_from`/`policy`/`payload`).
//!
//! Extracted from the original monolithic `notification.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD};
use crate::ast::{Channel, Span};

pub(in super::super) fn parse_channel(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Channel, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("channel ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "channel header must be `channel <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "channel header requires a name"));
    }

    let mut tenant_from: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut payload: Option<String> = None;
    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }

        if line.indent <= AGENT_INDENT_FEATURE_CHILD {
            break;
        }

        if line.indent != AGENT_INDENT_AGENT_CHILD {
            return Err(line_error(
                line,
                "channel body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            tenant_from = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("payload ") {
            payload = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "channel children are `tenant_from <axis>`, `policy @policy.<name>`, \
                 and `payload <RecordType>` (additional kinds — audit, rate_limit, \
                 broadcast, presence — deferred per docs/scope-discipline.md)",
            ));
        }
    }

    let tenant_from = tenant_from.ok_or_else(|| {
        line_error(
            header,
            "`channel` requires a `tenant_from <axis>` declaration",
        )
    })?;
    let policy = policy.ok_or_else(|| {
        line_error(
            header,
            "`channel` requires a `policy @policy.<name>` declaration",
        )
    })?;
    let payload = payload.ok_or_else(|| {
        line_error(
            header,
            "`channel` requires a `payload <RecordType>` declaration",
        )
    })?;

    Ok((
        Channel {
            name,
            tenant_from,
            policy,
            payload,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
