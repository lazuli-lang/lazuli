//! Notification sub-block parsers — `digest` and `throttle`. Both
//! sit at AGENT_INDENT_AGENT_CHILD under a `notification` header
//! with their own grandchild keyword catalogs.
//!
//! Extracted from the original monolithic `notification.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};
use crate::ast::{NotificationDigest, NotificationThrottle, Span};

pub(super) fn parse_notification_digest(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(NotificationDigest, usize), ParseError> {
    let header = &lines[start];
    let mut every: Option<String> = None;
    let mut group_by: Option<String> = None;
    let mut max_size: Option<u32> = None;
    let mut template_strategy: Option<String> = None;
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
                "`digest` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("every ") {
            every = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("group_by ") {
            group_by = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("max_size ") {
            let raw = rest.trim();
            match raw.parse::<u32>() {
                Ok(value) => max_size = Some(value),
                Err(_) => {
                    return Err(line_error(
                        line,
                        "`max_size` requires an unsigned 32-bit integer",
                    ));
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("template_strategy ") {
            template_strategy = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`digest` children are `every \"<duration>\"`, `group_by <path>`, `max_size <N>`, or `template_strategy merge|append`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let every = every.ok_or_else(|| {
        line_error(
            header,
            "`digest` requires an `every \"<duration>\"` declaration",
        )
    })?;

    Ok((
        NotificationDigest {
            every,
            group_by,
            max_size,
            template_strategy,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Notifications expanded bucket cycle — parse the `throttle`
/// sub-block of a `notification`. Header line is bare `throttle` at
/// indent 4; children at indent 6 are `max_per "<duration>"`
/// (required), `per_recipient` (bare flag), `per_channel` (bare
/// flag), and `burst <N>` (optional). Distinct keyword from scalar
/// `rate_limit` — the throttle keys on recipient/channel, not on the
/// caller.
pub(super) fn parse_notification_throttle(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(NotificationThrottle, usize), ParseError> {
    let header = &lines[start];
    let mut max_per: Option<String> = None;
    let mut per_recipient = false;
    let mut per_channel = false;
    let mut burst: Option<u32> = None;
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
                "`throttle` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("max_per ") {
            max_per = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if trimmed == "per_recipient" {
            per_recipient = true;
        } else if trimmed == "per_channel" {
            per_channel = true;
        } else if let Some(rest) = trimmed.strip_prefix("burst ") {
            let raw = rest.trim();
            match raw.parse::<u32>() {
                Ok(value) => burst = Some(value),
                Err(_) => {
                    return Err(line_error(
                        line,
                        "`burst` requires an unsigned 32-bit integer",
                    ));
                }
            }
        } else {
            return Err(line_error(
                line,
                "`throttle` children are `max_per \"<duration>\"`, `per_recipient`, `per_channel`, or `burst <N>`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let max_per = max_per.ok_or_else(|| {
        line_error(
            header,
            "`throttle` requires a `max_per \"<duration>\"` declaration",
        )
    })?;

    Ok((
        NotificationThrottle {
            max_per,
            per_recipient,
            per_channel,
            burst,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
