//! Feature-level `poller <name>` block parser.
//!
//! A poller is the canonical contract for "drive a cursor row through
//! a closed-state lifecycle by ticking external work". Closed catalog,
//! no escape hatch — every child below is enforced here so the AST
//! the analyzer sees is fully typed.
//!
//! ```text
//! poller <name>
//!   source <Resource>
//!   cursor
//!     eligible_when <next_at>, <resolved_at>
//!     attempts <attempts_field>
//!   retry
//!     max_attempts <int>
//!     backoff <fixed|linear|exponential> [base <duration>] [cap <duration>]
//!   states
//!     <state>            # one of: initial | intermediate | terminal
//!     <state> terminal
//!   resolve via @fn.<name>
//!   terminal_status_field <field>
//!   terminal_result_field <field>
//!   tick every <duration> [batch <int>]
//!   tenant_from row.<axis>_id
//!   idempotency by row.<field>[, row.<field>]*
//!   audit <subject>
//!   emits <event>
//!   retry_quirk <kind>
//!     when <predicate>
//!     counter <field>
//!     mutate row.<field> = <transform>
//! ```
//!
//! Every child appears at most once except `states` (>= 2 entries),
//! `emits`, and `retry_quirk` (zero-or-many catalog entries).
//!
//! Visibility: only `parse_poller_block` leaves the file as
//! `pub(super)`; the feature-skeleton walker dispatches to it from
//! `mod.rs`. Every other helper stays private — there are no
//! cross-cluster consumers.
//!
//! Source-of-truth: `docs/proposals/poller-vocab.md` §3 + the canonical
//! `full-capsule` fixture.

mod cursor;
mod retry;
mod states;
mod tick;

use super::super::common::{SourceLine, is_trivia, line_error};
use super::super::error::ParseError;
use super::types::{
    PollerBlockAst, PollerCursorAst, PollerRetryAst, PollerRetryQuirkAst, PollerStateAst,
    PollerTickAst,
};

use crate::ast::Span;

use cursor::parse_poller_cursor;
use retry::{parse_poller_retry, parse_poller_retry_quirk};
use states::parse_poller_states;
use tick::parse_poller_tick;

pub(super) fn parse_poller_block(
    lines: &[SourceLine<'_>],
    start: usize,
) -> Result<(PollerBlockAst, usize), ParseError> {
    let header = &lines[start];
    let block_indent = header.indent;
    let child_indent = block_indent + 2;
    let grandchild_indent = block_indent + 4;
    let name = header
        .text
        .trim_start()
        .strip_prefix("poller ")
        .map(|rest| rest.trim().to_owned())
        .ok_or_else(|| line_error(header, "poller header must be `poller <name>`"))?;
    if name.is_empty() {
        return Err(line_error(header, "poller header requires a name"));
    }

    let mut source: Option<String> = None;
    let mut cursor: Option<PollerCursorAst> = None;
    let mut retry: Option<PollerRetryAst> = None;
    let mut states: Vec<PollerStateAst> = Vec::new();
    let mut resolve_handler: Option<String> = None;
    let mut terminal_status_field: Option<String> = None;
    let mut terminal_result_field: Option<String> = None;
    let mut tick: Option<PollerTickAst> = None;
    let mut tenant_from: Option<String> = None;
    let mut idempotency: Vec<String> = Vec::new();
    let mut idempotency_seen = false;
    let mut audit: Option<String> = None;
    let mut emits: Vec<String> = Vec::new();
    let mut retry_quirks: Vec<PollerRetryQuirkAst> = Vec::new();

    let mut last_end = header.end;
    let mut i = start + 1;

    while i < lines.len() {
        let line = &lines[i];
        let trimmed = line.text.trim_start();

        if is_trivia(trimmed) {
            i += 1;
            continue;
        }
        if line.indent <= block_indent {
            break;
        }
        if line.indent != child_indent {
            return Err(line_error(
                line,
                "poller children use one indentation level deeper than the `poller` header",
            ));
        }

        if let Some(rest) = trimmed.strip_prefix("source ") {
            if source.is_some() {
                return Err(line_error(line, "poller declares `source` at most once"));
            }
            let val = rest.trim();
            if val.is_empty() {
                return Err(line_error(
                    line,
                    "`source` requires a resource name (`source <Resource>`)",
                ));
            }
            source = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if trimmed == "cursor" {
            if cursor.is_some() {
                return Err(line_error(line, "poller declares `cursor` at most once"));
            }
            let (block, next) = parse_poller_cursor(lines, i, child_indent)?;
            cursor = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "retry" {
            if retry.is_some() {
                return Err(line_error(line, "poller declares `retry` at most once"));
            }
            let (block, next) = parse_poller_retry(lines, i, child_indent)?;
            retry = Some(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if trimmed == "states" {
            if !states.is_empty() {
                return Err(line_error(line, "poller declares `states` at most once"));
            }
            let (block, next) = parse_poller_states(lines, i, child_indent)?;
            states = block;
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("resolve via ") {
            if resolve_handler.is_some() {
                return Err(line_error(line, "poller declares `resolve` at most once"));
            }
            let val = rest.trim();
            let handler = val.strip_prefix("@fn.").ok_or_else(|| {
                line_error(
                    line,
                    "`resolve via` requires `@fn.<name>` handler reference",
                )
            })?;
            if handler.is_empty() {
                return Err(line_error(
                    line,
                    "`resolve via @fn.<name>` requires a handler name",
                ));
            }
            resolve_handler = Some(handler.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("terminal_status_field ") {
            if terminal_status_field.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `terminal_status_field` at most once",
                ));
            }
            terminal_status_field = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("terminal_result_field ") {
            if terminal_result_field.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `terminal_result_field` at most once",
                ));
            }
            terminal_result_field = Some(rest.trim().to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("tick ") {
            if tick.is_some() {
                return Err(line_error(line, "poller declares `tick` at most once"));
            }
            tick = Some(parse_poller_tick(line, rest)?);
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("tenant_from ") {
            if tenant_from.is_some() {
                return Err(line_error(
                    line,
                    "poller declares `tenant_from` at most once",
                ));
            }
            let val = rest.trim();
            if !val.starts_with("row.") {
                return Err(line_error(
                    line,
                    "`tenant_from` requires `row.<axis>_id` (poller cursor row is the producer)",
                ));
            }
            tenant_from = Some(val.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("idempotency by ") {
            if idempotency_seen {
                return Err(line_error(
                    line,
                    "poller declares `idempotency` at most once",
                ));
            }
            idempotency_seen = true;
            for entry in rest.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                if !entry.starts_with("row.") {
                    return Err(line_error(
                        line,
                        "`idempotency by` entries are `row.<field>` paths",
                    ));
                }
                idempotency.push(entry.to_owned());
            }
            if idempotency.is_empty() {
                return Err(line_error(
                    line,
                    "`idempotency by` requires at least one `row.<field>`",
                ));
            }
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("audit ") {
            if audit.is_some() {
                return Err(line_error(line, "poller declares `audit` at most once"));
            }
            audit = Some(format!("audit {}", rest.trim()));
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("emits ") {
            let event = rest.trim();
            if event.is_empty() {
                return Err(line_error(line, "`emits` requires an event name"));
            }
            emits.push(event.to_owned());
            last_end = line.end;
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("retry_quirk ") {
            let (block, next) = parse_poller_retry_quirk(lines, i, rest, grandchild_indent)?;
            retry_quirks.push(block);
            last_end = lines[next.saturating_sub(1).max(i)].end;
            i = next;
            continue;
        }

        return Err(line_error(
            line,
            "poller children are `source`, `cursor`, `retry`, `states`, `resolve via @fn.<name>`, \
             `terminal_status_field`, `terminal_result_field`, `tick every <duration>`, \
             `tenant_from row.<axis>_id`, `idempotency by row.<field>, ...`, `audit`, `emits`, `retry_quirk`",
        ));
    }

    let source =
        source.ok_or_else(|| line_error(header, "poller requires a `source <Resource>` child"))?;

    Ok((
        PollerBlockAst {
            name,
            source,
            cursor,
            retry,
            states,
            resolve_handler,
            terminal_status_field,
            terminal_result_field,
            tick,
            tenant_from,
            idempotency,
            audit,
            emits,
            retry_quirks,
            span: Span::new(header.start, last_end),
        },
        i,
    ))
}




// =============================================================================
// L0 #8 — `poller` block parser tests (docs/proposals/poller-vocab.md §3)
// =============================================================================
#[cfg(test)]
mod poller_parser_tests {
    use super::super::parse_feature_skeletons;

    #[test]
    fn poller_block_parses_minimal() {
        let source = "feature multi_bank\n  poller v8_consult_resolver\n    source V8PendingConsult\n    cursor\n      eligible_when next_check_at, resolved_at\n      attempts attempts\n    retry\n      max_attempts 30\n      backoff exponential base 30s cap 10m\n    states\n      pending initial\n      resolved terminal\n      failed terminal\n    resolve via @fn.poll_v8\n    idempotency by row.id, row.attempts\n";
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        let pollers = &features[0].pollers;
        assert_eq!(pollers.len(), 1);
        let p = &pollers[0];
        assert_eq!(p.name, "v8_consult_resolver");
        assert_eq!(p.source, "V8PendingConsult");
        let cursor = p.cursor.as_ref().unwrap();
        assert_eq!(cursor.next_at_field, "next_check_at");
        assert_eq!(cursor.resolved_at_field, "resolved_at");
        assert_eq!(cursor.attempts_field, "attempts");
        let retry = p.retry.as_ref().unwrap();
        assert_eq!(retry.max_attempts, 30);
        assert_eq!(retry.backoff_strategy, "exponential");
        assert_eq!(retry.backoff_base.as_deref(), Some("30s"));
        assert_eq!(retry.backoff_cap.as_deref(), Some("10m"));
        assert_eq!(p.states.len(), 3);
        assert_eq!(p.states[0].name, "pending");
        assert_eq!(p.states[0].kind_keyword.as_deref(), Some("initial"));
        assert_eq!(p.resolve_handler.as_deref(), Some("poll_v8"));
        assert_eq!(p.idempotency, vec!["row.id", "row.attempts"]);
    }

    #[test]
    fn poller_block_parses_full_with_quirk() {
        let source = "feature multi_bank\n  poller v8_consult_resolver\n    source V8PendingConsult\n    cursor\n      eligible_when next_check_at, resolved_at\n      attempts attempts\n    retry\n      max_attempts 30\n      backoff exponential base 30s cap 10m\n    states\n      pending initial\n      gender_ambiguous intermediate\n      resolved terminal\n      failed terminal\n    resolve via @fn.poll_v8\n    terminal_status_field final_status\n    terminal_result_field final_resultado\n    tick every 15s batch 100\n    tenant_from row.org_id\n    idempotency by row.id, row.attempts\n    audit default\n    emits v8_consult_resolved\n    emits v8_consult_failed\n    retry_quirk gender_flip_once\n      when row.status_v8 == \"gender_ambiguous\"\n      counter gender_retry_count\n      mutate row.gender = flip(row.gender)\n";
        let features = parse_feature_skeletons(source).unwrap();
        let p = &features[0].pollers[0];
        assert_eq!(p.tick.as_ref().unwrap().every, "15s");
        assert_eq!(p.tick.as_ref().unwrap().batch, Some(100));
        assert_eq!(p.tenant_from.as_deref(), Some("row.org_id"));
        assert_eq!(p.terminal_status_field.as_deref(), Some("final_status"));
        assert_eq!(p.terminal_result_field.as_deref(), Some("final_resultado"));
        assert_eq!(p.audit.as_deref(), Some("audit default"));
        assert_eq!(p.emits.len(), 2);
        assert_eq!(p.retry_quirks.len(), 1);
        let q = &p.retry_quirks[0];
        assert_eq!(q.kind, "gender_flip_once");
        assert_eq!(q.counter_field, "gender_retry_count");
        assert_eq!(q.mutate_field, "gender");
        assert_eq!(q.mutate_transform, "flip(row.gender)");
    }

    #[test]
    fn poller_requires_source() {
        let source = "feature multi_bank\n  poller bad\n    cursor\n      eligible_when next_check_at, resolved_at\n      attempts attempts\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("source"));
    }

    #[test]
    fn poller_states_min_two() {
        let source =
            "feature multi_bank\n  poller bad\n    source X\n    states\n      only_one terminal\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("at least 2"));
    }

    #[test]
    fn poller_backoff_strategy_closed() {
        let source = "feature multi_bank\n  poller bad\n    source X\n    retry\n      max_attempts 3\n      backoff weibull\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("fixed"));
    }

    #[test]
    fn poller_tenant_from_requires_row_prefix() {
        let source =
            "feature multi_bank\n  poller bad\n    source X\n    tenant_from payload.org_id\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(err.to_string().contains("row."));
    }

    // -----------------------------------------------------------------
    // Realtime bucket cycle MVP — `channel` parser tests.
    //
    // Three cases per the cycle proposal: minimal happy path; rejects
    // when a required child is missing; rejects unknown child key. The
    // doctor diagnostic `CHANNEL-PAYLOAD-001` covers payload-resolution
    // separately (in `lazuli_cli`'s doctor suite).
    // -----------------------------------------------------------------

    #[test]
    fn channel_parses_minimal() {
        let source = "feature customer\n  channel customer_activity\n    tenant_from org\n    policy @policy.read\n    payload CustomerActivityEvent\n";
        let features = parse_feature_skeletons(source).unwrap();
        assert_eq!(features.len(), 1);
        let channels = &features[0].channels;
        assert_eq!(channels.len(), 1);
        let c = &channels[0];
        assert_eq!(c.name, "customer_activity");
        assert_eq!(c.tenant_from, "org");
        assert_eq!(c.policy, "@policy.read");
        assert_eq!(c.payload, "CustomerActivityEvent");
    }

    #[test]
    fn channel_rejects_missing_payload() {
        let source = "feature customer\n  channel customer_activity\n    tenant_from org\n    policy @policy.read\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("payload"),
            "error should mention missing payload, got: {err}"
        );
    }

    #[test]
    fn channel_rejects_unknown_child_key() {
        // `transport ws` is one of the explicitly rejected anti-proposals
        // — transport is adapter-resolved, never authored on the channel.
        let source = "feature customer\n  channel customer_activity\n    tenant_from org\n    policy @policy.read\n    payload CustomerActivityEvent\n    transport ws\n";
        let err = parse_feature_skeletons(source).unwrap_err();
        assert!(
            err.to_string().contains("channel children"),
            "error should mention closed catalog, got: {err}"
        );
    }
}
