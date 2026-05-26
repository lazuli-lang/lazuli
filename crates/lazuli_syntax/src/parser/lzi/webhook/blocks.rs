//! Webhook child-block parsers — `replay`, `dlq`, `verify`. Each
//! takes the line slice plus the trailing head text after the keyword
//! and returns the parsed AST node plus the index past the block.
//!
//! Extracted from the original monolithic `webhook.rs`.

use super::super::super::common::{SourceLine, is_trivia, line_error, unquote_lzx_value};
use super::super::super::error::ParseError;
use super::super::{AGENT_INDENT_AGENT_CHILD, AGENT_INDENT_GRANDCHILD};
use crate::ast::{Span, WebhookDlq, WebhookReplay, WebhookVerify};

pub(super) fn parse_webhook_replay(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
) -> Result<(WebhookReplay, usize), ParseError> {
    let header = &lines[start];
    let head = head_rest.trim();

    if !head.is_empty() {
        // Short form: `replay allow within "24h"` (`deny` has no
        // `within`, so allow/deny dispatch happens first).
        let mut tokens = head.split_whitespace();
        let mode = tokens
            .next()
            .ok_or_else(|| line_error(header, "`replay` requires `allow` or `deny`"))?;
        if mode != "allow" && mode != "deny" {
            return Err(line_error(
                header,
                "`replay` mode must be `allow` or `deny`",
            ));
        }
        let mut within: Option<String> = None;
        let rest_tail: Vec<&str> = tokens.collect();
        if !rest_tail.is_empty() {
            // Expect `within "<duration>"`.
            if rest_tail[0] != "within" || rest_tail.len() < 2 {
                return Err(line_error(
                    header,
                    "`replay <mode>` short form takes only `within \"<duration>\"`",
                ));
            }
            within = Some(unquote_lzx_value(rest_tail[1..].join(" ").trim()).to_owned());
        }
        return Ok((
            WebhookReplay {
                mode: mode.to_owned(),
                within,
                dedupe_by: None,
                span: Span::new(header.start, header.end),
            },
            start + 1,
        ));
    }

    // Long form: nested children at AGENT_INDENT_GRANDCHILD.
    let mut mode: Option<String> = None;
    let mut within: Option<String> = None;
    let mut dedupe_by: Option<String> = None;
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
                "`replay` children use six-space indentation",
            ));
        }

        if trimmed == "allow" || trimmed.starts_with("allow ") {
            if mode.is_some() {
                return Err(line_error(line, "`replay` declares `allow` or `deny` once"));
            }
            mode = Some("allow".to_owned());
            // Allow `allow within "..."` on the same nested line as a
            // shorthand inside the long form.
            if let Some(rest) = trimmed.strip_prefix("allow ") {
                let rest = rest.trim();
                if let Some(within_rest) = rest.strip_prefix("within ") {
                    within = Some(unquote_lzx_value(within_rest.trim()).to_owned());
                } else {
                    return Err(line_error(
                        line,
                        "`allow` only accepts a trailing `within \"<duration>\"`",
                    ));
                }
            }
        } else if trimmed == "deny" {
            if mode.is_some() {
                return Err(line_error(line, "`replay` declares `allow` or `deny` once"));
            }
            mode = Some("deny".to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("within ") {
            within = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("dedupe by ") {
            dedupe_by = Some(rest.trim().to_owned());
        } else {
            return Err(line_error(
                line,
                "`replay` children are `allow`, `deny`, `within`, or `dedupe by`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    let mode = mode.ok_or_else(|| line_error(header, "`replay` requires `allow` or `deny`"))?;

    Ok((
        WebhookReplay {
            mode,
            within,
            dedupe_by,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}

/// Webhooks expanded cycle — parse `dlq` with three mutually-exclusive
/// surface forms:
///
///   `dlq emit <event>`
///   `dlq handler "./..."`
///   `dlq drop` + nested `reason "..."`
pub(super) fn parse_webhook_dlq(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
) -> Result<(WebhookDlq, usize), ParseError> {
    let header = &lines[start];
    let head = head_rest.trim();
    let span = Span::new(header.start, header.end);

    if let Some(rest) = head.strip_prefix("emit ") {
        let event = rest.trim();
        if event.is_empty() {
            return Err(line_error(header, "`dlq emit` requires an event name"));
        }
        return Ok((
            WebhookDlq::Emit {
                event: event.to_owned(),
                span,
            },
            start + 1,
        ));
    }

    if let Some(rest) = head.strip_prefix("handler ") {
        let path = unquote_lzx_value(rest.trim()).to_owned();
        if path.is_empty() {
            return Err(line_error(header, "`dlq handler` requires a quoted path"));
        }
        return Ok((WebhookDlq::Handler { path, span }, start + 1));
    }

    if head == "drop" || head.is_empty() && header.text.trim_start() == "dlq drop" {
        // Long form only: `dlq drop` + nested `reason "..."`.
        let mut reason: Option<String> = None;
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
                    "`dlq drop` children use six-space indentation",
                ));
            }
            if let Some(rest) = trimmed.strip_prefix("reason ") {
                reason = Some(unquote_lzx_value(rest.trim()).to_owned());
            } else {
                return Err(line_error(line, "`dlq drop` accepts only `reason \"...\"`"));
            }
            last_end = line.end;
            i += 1;
        }

        let reason = reason.ok_or_else(|| {
            line_error(
                header,
                "`dlq drop` requires `reason \"...\"` — silent drops on dead-letter must be explicit waivers",
            )
        })?;
        return Ok((
            WebhookDlq::Drop {
                reason,
                span: Span::new(header.start, last_end),
            },
            i,
        ));
    }

    Err(line_error(
        header,
        "`dlq` children are `emit <event>`, `handler \"...\"`, or `drop`",
    ))
}

pub(super) fn parse_webhook_verify(
    lines: &[SourceLine<'_>],
    start: usize,
    head_rest: &str,
) -> Result<(WebhookVerify, usize), ParseError> {
    let header = &lines[start];
    let head = head_rest.trim();
    let (scheme, algorithm) = head.split_once(' ').ok_or_else(|| {
        line_error(
            header,
            "`verify` requires `<scheme> <algorithm>` (e.g. `verify hmac sha256`)",
        )
    })?;
    let mut secret_env: Option<String> = None;
    let mut header_lit: Option<String> = None;
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
                "`verify` children use six-space indentation",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("secret ") {
            // `secret env.CRM_WEBHOOK_SECRET` — record the env binding
            // verbatim (analyzer extracts the env name).
            secret_env = Some(rest.trim().to_owned());
        } else if let Some(rest) = trimmed.strip_prefix("header ") {
            header_lit = Some(unquote_lzx_value(rest.trim()).to_owned());
        } else {
            return Err(line_error(
                line,
                "`verify` children are `secret` or `header`",
            ));
        }

        last_end = line.end;
        i += 1;
    }

    Ok((
        WebhookVerify {
            scheme: scheme.to_owned(),
            algorithm: algorithm.trim().to_owned(),
            secret_env,
            header: header_lit,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}
