//! `channel` + `notification` feature children — the user-facing async
//! delivery vocabulary (email, push, sms, in-app, banners, ...).
//!
//! ## Two feature kinds
//!
//! - `channel <name>` — declares a typed delivery channel. Required
//!   children: `tenant_from <axis>`, `policy @policy.<name>`, and
//!   `payload <RecordType>`. The audit / rate_limit / broadcast /
//!   presence variants are deferred per `docs/scope-discipline.md`.
//! - `notification <name>` — declares a notification rule. Required
//!   children: `recipient <path>`, `trigger event/schedule ...`,
//!   `template "./..."`. Optional: `channel <list>`, `tenant_from`,
//!   `idempotency by`, `retry`, `policy` (atom + expression),
//!   `emits <event>`, plus the `digest` and `throttle` sub-blocks
//!   (closed-catalog, at most one each).
//!
//! ## Cross-module borrows
//!
//! `parse_notification` reuses `parse_job_trigger` and `parse_job_retry`
//! from the sibling `job` parser (`pub(super)` re-exports in `mod.rs`) —
//! `trigger` and `retry` semantics match `job` exactly, so the surface
//! shares the same parser. It also calls `try_parse_policy_expr` from
//! `lzx::policy_expr` for the optional structured policy expression.
//!
//! ## See also
//!
//! - `docs/canonical-semantics.md` — `channel` + `notification` grammar.
//! - `lazuli_ir::nodes::notification` — typed lowering target.

use super::super::common::{
    SourceLine, is_trivia, line_error, split_lzx_list, unquote_lzx_value,
};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD};

use crate::ast::{
    Channel, JobRetry, JobTrigger, Notification, NotificationDigest, NotificationThrottle,
    PolicyExprAst, Span,
};

pub(super) fn parse_channel(
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

pub(super) fn parse_notification(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Notification, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("notification ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "notification header must be `notification <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "notification header requires a name"));
    }

    let mut channels: Vec<String> = Vec::new();
    let mut recipient: Option<String> = None;
    let mut trigger: Option<JobTrigger> = None;
    let mut tenant_from: Option<String> = None;
    let mut idempotency_by: Option<String> = None;
    let mut retry: Option<JobRetry> = None;
    let mut template: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut emits: Vec<String> = Vec::new();
    let mut digest: Option<NotificationDigest> = None;
    let mut throttle: Option<NotificationThrottle> = None;
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
                "notification body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("channel ") {
            channels = split_lzx_list(rest);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("recipient ") {
            recipient = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("trigger ") {
            trigger = Some(super::parse_job_trigger(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            tenant_from = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            retry = Some(super::parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("template ") {
            template = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            emits.push(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "digest" {
            if digest.is_some() {
                return Err(line_error(
                    line,
                    "`notification` may declare at most one `digest` sub-block",
                ));
            }
            let (parsed, next) = parse_notification_digest(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            digest = Some(parsed);
            i = next;
        } else if trimmed == "throttle" {
            if throttle.is_some() {
                return Err(line_error(
                    line,
                    "`notification` may declare at most one `throttle` sub-block",
                ));
            }
            let (parsed, next) = parse_notification_throttle(lines, i)?;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            throttle = Some(parsed);
            i = next;
        } else {
            return Err(line_error(
                line,
                "notification children are `channel`, `recipient`, `trigger`, `tenant_from`, `idempotency by`, `retry`, `template`, `policy`, `emits`, `digest`, or `throttle`",
            ));
        }
    }

    let recipient = recipient.ok_or_else(|| {
        line_error(
            header,
            "`notification` requires a `recipient <path>` declaration",
        )
    })?;
    let trigger = trigger.ok_or_else(|| {
        line_error(
            header,
            "`notification` requires a `trigger event ...` or `trigger schedule ...` declaration",
        )
    })?;
    let template = template.ok_or_else(|| {
        line_error(
            header,
            "`notification` requires a `template \"./...\"` declaration",
        )
    })?;

    Ok((
        Notification {
            name,
            channels,
            recipient,
            trigger,
            tenant_from,
            idempotency_by,
            retry,
            template,
            policy,
            policy_expr,
            emits,
            digest,
            throttle,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Notifications expanded bucket cycle — parse the `digest` sub-block
/// of a `notification`. Header line is bare `digest` at indent 4;
/// children at indent 6 are `every "<duration>"` (required),
/// `group_by <path>` (optional), `max_size <N>` (optional), and
/// `template_strategy <merge|append>` (optional). All other child
/// keys are rejected to keep the catalog closed.
fn parse_notification_digest(
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
fn parse_notification_throttle(
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
