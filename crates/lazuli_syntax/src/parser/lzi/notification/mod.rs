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

mod blocks;
mod channel;

use super::super::common::{SourceLine, is_trivia, line_error, split_lzx_list, unquote_lzx_value};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD};

use crate::ast::{
    JobRetry, JobTrigger, Notification, NotificationDigest, NotificationThrottle, PolicyExprAst,
    Span,
};

use blocks::{parse_notification_digest, parse_notification_throttle};
pub(super) use channel::parse_channel;

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
            trigger = Some(super::job::parse_job_trigger(line, rest)?);
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
            retry = Some(super::job::parse_job_retry(line, rest)?);
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

// =============================================================================
// Notifications expanded bucket cycle — digest/throttle parser tests.
// =============================================================================
#[cfg(test)]
mod notification_digest_throttle_parser_tests {
    use super::super::parse_feature_skeletons;

    fn source_with_notification(children: &str) -> String {
        format!(
            "feature customer_outreach\n  notification booking_confirmed\n    channel email, push\n    recipient target.user.email\n    trigger event payments.transaction_completed\n    template \"./templates/booking_confirmed.<locale>.tmpl\"\n    policy @policy.dispatch\n{children}"
        )
    }

    #[test]
    fn notification_digest_parses_full_surface() {
        let source = source_with_notification(
            "    digest\n      every 1h\n      group_by payload.user_id\n      max_size 50\n      template_strategy merge\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let digest = features[0].notifications[0]
            .digest
            .as_ref()
            .expect("digest");
        assert_eq!(digest.every, "1h");
        assert_eq!(digest.group_by.as_deref(), Some("payload.user_id"));
        assert_eq!(digest.max_size, Some(50));
        assert_eq!(digest.template_strategy.as_deref(), Some("merge"));
    }

    #[test]
    fn notification_digest_requires_every() {
        let source = source_with_notification(
            "    digest\n      group_by payload.user_id\n      max_size 50\n",
        );
        let err = parse_feature_skeletons(&source).unwrap_err();
        assert!(err.to_string().contains("every"), "{err}");
    }

    #[test]
    fn notification_digest_rejects_unknown_child() {
        let source = source_with_notification("    digest\n      every 1h\n      mode batch\n");
        let err = parse_feature_skeletons(&source).unwrap_err();
        assert!(err.to_string().contains("digest"), "{err}");
    }

    #[test]
    fn notification_throttle_parses_full_surface() {
        let source = source_with_notification(
            "    throttle\n      per_recipient\n      per_channel\n      burst 3\n      max_per 1h\n",
        );
        let features = parse_feature_skeletons(&source).expect("parses");
        let throttle = features[0].notifications[0]
            .throttle
            .as_ref()
            .expect("throttle");
        assert_eq!(throttle.max_per, "1h");
        assert!(throttle.per_recipient);
        assert!(throttle.per_channel);
        assert_eq!(throttle.burst, Some(3));
    }

    #[test]
    fn notification_throttle_requires_max_per() {
        let source = source_with_notification("    throttle\n      per_recipient\n");
        let err = parse_feature_skeletons(&source).unwrap_err();
        assert!(err.to_string().contains("max_per"), "{err}");
    }

    #[test]
    fn notification_throttle_rejects_unknown_child() {
        let source = source_with_notification("    throttle\n      max_per 1h\n      per_user\n");
        let err = parse_feature_skeletons(&source).unwrap_err();
        assert!(err.to_string().contains("throttle"), "{err}");
    }
}
