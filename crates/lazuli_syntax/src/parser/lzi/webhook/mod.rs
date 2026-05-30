//! `webhook <name>` block parser and its tightly-coupled children
//! (`verify`, `replay`, `dlq`, `scope global`).
//!
//! The block sits at feature child indent (2 spaces); body children at
//! agent child indent (4 spaces); inner block children (`verify`,
//! `replay`, `dlq`) live at six-space grandchild indent.
//!
//! ## Closed catalog
//!
//! Webhook bodies recognise exactly these keys:
//!
//! - `path "<url>"` — route the webhook listens on (required).
//! - `verify hmac <alg>` — HMAC signature verification (required).
//! - `tenant_from <path>` — tenant key extractor on the payload.
//! - `scope global` + `reason "<text>"` — explicit waiver for webhooks
//!   that can't carry a tenant key.
//! - `idempotency by <path>` — replay-key extractor.
//! - `policy <expr>` — policy gate on the inbound call.
//! - `handler "<path>" [returns Type]` — Go-side body shim.
//! - `emits <event> [when <pred>]` — single-line or block form; the
//!   optional predicate is captured verbatim.
//! - `payload from webhook_events.<name>` — typed payload binding.
//! - `replay <mode>` (`allow`/`deny`) + optional `within "<dur>"` +
//!   `dedupe by <path>`. Short form on one line; long form as a block.
//! - `dlq emit <event>` / `dlq handler "<path>"` / `dlq drop` + nested
//!   `reason "..."` for the drop arm.
//! - `retry <count> backoff <strategy>` — shared with `parse_job_retry`.
//!
//! ## See also
//!
//! - `docs/proposals/webhooks-expanded.md`
//! - `lazuli_ir::nodes::webhook` — typed lowering target.

mod blocks;

use super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::error::ParseError;
use super::super::lzx::try_parse_policy_expr;
use super::job::{parse_handler_line, parse_job_retry};
use super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_FEATURE_CHILD, AGENT_INDENT_GRANDCHILD};
use crate::ast::{
    JobRetry, PolicyExprAst, Span, Webhook, WebhookDlq, WebhookHandler, WebhookReplay,
    WebhookVerify,
};

use blocks::{parse_webhook_dlq, parse_webhook_replay, parse_webhook_verify};

pub(super) fn parse_webhook(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(Webhook, usize), ParseError> {
    let header = &lines[start];
    let header_trimmed = header.text.trim_start();
    let name = header_trimmed
        .strip_prefix("webhook ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "webhook header must be `webhook <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "webhook header requires a name"));
    }

    let mut route: Option<String> = None;
    let mut verify: Option<WebhookVerify> = None;
    let mut tenant_from: Option<String> = None;
    let mut scope_global: Option<crate::ast::WebhookScopeGlobal> = None;
    let mut idempotency_by: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut policy_expr: Option<PolicyExprAst> = None;
    let mut handler: Option<WebhookHandler> = None;
    let mut emits: Vec<String> = Vec::new();
    let mut emits_predicates: Vec<Option<String>> = Vec::new();
    let mut payload_from: Option<String> = None;
    let mut replay: Option<WebhookReplay> = None;
    let mut dlq: Option<WebhookDlq> = None;
    let mut retry: Option<JobRetry> = None;
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
                "webhook body children use four-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("path ") {
            route = Some(unquote_lzx_value(rest.trim()).to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("verify ") {
            let (parsed, next) = parse_webhook_verify(lines, i, rest)?;
            verify = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            tenant_from = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if trimmed == "scope global" || trimmed.starts_with("scope global ") {
            // Closes WAR-VOCAB-WEBHOOK-01. `scope global` followed by
            // a required `reason "..."` child line — for cases where
            // the provider doesn't send a tenant key in the payload
            // and the handler reconciles tenant elsewhere (e.g.
            // external_reference lookup). Reason is captured for
            // audit + doctor surfaces so the escape is explicit.
            let scope_line = line;
            let mut reason_text: Option<String> = None;
            let mut next = i + 1;
            while next < lines.len() {
                let next_line = &lines[next];
                let next_trim = next_line.text.trim_start();
                if is_trivia(next_trim) {
                    next += 1;
                    continue;
                }
                if next_line.indent <= AGENT_INDENT_AGENT_CHILD {
                    break;
                }
                if let Some(reason_rest) = next_trim.strip_prefix("reason ") {
                    let raw = reason_rest.trim();
                    let unquoted = unquote_lzx_value(raw);
                    if unquoted.is_empty() {
                        return Err(line_error(
                            next_line,
                            "`scope global` requires non-empty `reason \"...\"`",
                        ));
                    }
                    reason_text = Some(unquoted.to_owned());
                    last_end = next_line.end;
                    next += 1;
                    continue;
                }
                return Err(line_error(
                    next_line,
                    "`scope global` child must be `reason \"...\"`",
                ));
            }
            let reason = reason_text.ok_or_else(|| {
                line_error(
                    scope_line,
                    "`scope global` requires a `reason \"...\"` child explaining why the webhook escapes tenant_from",
                )
            })?;
            scope_global = Some(crate::ast::WebhookScopeGlobal {
                reason,
                span: Span {
                    start: scope_line.start,
                    end: last_end,
                },
            });
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            idempotency_by = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("policy ") {
            policy = Some(rest.trim().to_owned());
            policy_expr = try_parse_policy_expr(line, rest)?;
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("handler ") {
            let handler_job = parse_handler_line(rest);
            handler = Some(WebhookHandler {
                path: handler_job.path,
                returns: handler_job.returns,
            });
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("emits ") {
            // B5 framework gap 2 — single-line `emits <event> [when
            // <predicate>]` shape. Recognises the optional `when`
            // suffix and splits it from the event name. The flat
            // shape (no `when`) keeps the existing behaviour.
            let (event_name, predicate) = split_emits_when(rest.trim());
            if event_name.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            emits.push(event_name.to_owned());
            emits_predicates.push(predicate.map(str::to_owned));
            last_end = line.end;
            i += 1;
        } else if trimmed == "emits" {
            // B5 framework gap 2 — block-form `emits` with one
            // `<event> [when <predicate>]` per line. Each child line
            // sits one indentation level deeper than the header.
            let block_indent = line.indent + 2;
            i += 1;
            while i < lines.len() {
                let child = &lines[i];
                let child_trim = child.text.trim_start();
                if is_trivia(child_trim) {
                    i += 1;
                    continue;
                }
                if child.indent < block_indent {
                    break;
                }
                if child.indent != block_indent {
                    return Err(line_error(
                        child,
                        "`emits` block children use one indentation level deeper than the header",
                    ));
                }
                let (event_name, predicate) = split_emits_when(child_trim);
                if event_name.is_empty() {
                    return Err(line_error(
                        child,
                        "`emits` block entry requires an event name",
                    ));
                }
                emits.push(event_name.to_owned());
                emits_predicates.push(predicate.map(str::to_owned));
                last_end = child.end;
                i += 1;
            }
        } else if let Some(rest) = trimmed.strip_prefix("payload from ") {
            // `payload from webhook_events.<name>` — the `webhook_events.`
            // prefix is mandatory at the surface so the catalog is
            // obvious to a cold-reading author/LLM; only the suffix is
            // kept in the AST.
            let raw = rest.trim();
            let suffix = raw.strip_prefix("webhook_events.").ok_or_else(|| {
                line_error(
                    line,
                    "`payload from` requires `webhook_events.<name>` (catalog prefix is mandatory)",
                )
            })?;
            if suffix.is_empty() {
                return Err(line_error(
                    line,
                    "`payload from webhook_events.` requires an entry name",
                ));
            }
            payload_from = Some(suffix.to_owned());
            last_end = line.end;
            i += 1;
        } else if let Some(rest) = trimmed.strip_prefix("replay") {
            // Two surface forms:
            //   `replay allow within "24h"`  (single-line short form)
            //   `replay\n  allow\n  within "24h"\n  dedupe by ...`
            // dispatched by inspecting the remainder of the header.
            let (parsed, next) = parse_webhook_replay(lines, i, rest)?;
            replay = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("dlq") {
            let (parsed, next) = parse_webhook_dlq(lines, i, rest)?;
            dlq = Some(parsed);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
        } else if let Some(rest) = trimmed.strip_prefix("retry ") {
            retry = Some(parse_job_retry(line, rest)?);
            last_end = line.end;
            i += 1;
        } else if trimmed.starts_with("gate ") {
            // PG.A — gates lifted via side-channel pass; tolerate here.
            last_end = line.end;
            i += 1;
        } else {
            return Err(line_error(
                line,
                "webhook children are `path`, `verify`, `tenant_from`, `scope global` + `reason`, `idempotency by`, `policy`, `handler`, `emits`, `payload from`, `replay`, `retry`, `dlq`, or `gate behind/quota plan.*`",
            ));
        }
    }

    let route = route
        .ok_or_else(|| line_error(header, "`webhook` requires a `path \"/...\"` declaration"))?;
    let verify = verify.ok_or_else(|| {
        line_error(
            header,
            "`webhook` requires a `verify hmac <alg>` declaration",
        )
    })?;

    // B5 framework gap 2 — only keep `emits_predicates` when at least
    // one entry carries a `when` clause; an all-None vec is the legacy
    // shape and serialises as empty.
    let emits_predicates = if emits_predicates.iter().any(|p| p.is_some()) {
        emits_predicates
    } else {
        Vec::new()
    };

    Ok((
        Webhook {
            name,
            route,
            verify,
            tenant_from,
            scope_global,
            idempotency_by,
            policy,
            policy_expr,
            handler,
            emits,
            emits_predicates,
            payload_from,
            replay,
            dlq,
            retry,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// B5 framework gap 2 — split an `emits <event> [when <predicate>]`
/// payload into its event name + optional predicate. The predicate
/// (everything after the ` when ` token) is trimmed but kept verbatim;
/// the analyzer is responsible for the typed lift.
fn split_emits_when(raw: &str) -> (&str, Option<&str>) {
    let text = raw.trim();
    // Match against ` when ` (with surrounding whitespace) so an
    // event name that contains `when` substring is not split. Use a
    // manual scan because `str::split_once` doesn't accept a closure
    // for whole-word boundaries.
    let mut search_from = 0;
    let bytes = text.as_bytes();
    while let Some(rel) = text[search_from..].find("when") {
        let abs = search_from + rel;
        let before_ok = abs > 0 && bytes[abs - 1].is_ascii_whitespace();
        let after_pos = abs + "when".len();
        let after_ok = after_pos < bytes.len() && bytes[after_pos].is_ascii_whitespace();
        if before_ok && after_ok {
            let head = text[..abs].trim_end();
            let predicate = text[after_pos..].trim();
            if predicate.is_empty() {
                // `emits foo when` (no predicate) — treat as a flat
                // emits line. The analyzer/doctor surfaces this as a
                // diagnostic if useful; the parser stays tolerant.
                return (head, None);
            }
            return (head, Some(predicate));
        }
        search_from = abs + "when".len();
    }
    (text, None)
}
