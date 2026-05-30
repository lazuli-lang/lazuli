//! `job` + `poller` block lowerings — the two async-work envelopes.
//!
//! ## What lives here
//!
//! * `lower_job` — `job <name>` blocks. Threads through the shared
//!   workflow leaves (`lower_job_trigger`, `lower_retry`, `lower_fanout`,
//!   `lower_external_call`, `lower_job_body`) and builds the typed
//!   `ir::Job` envelope.
//! * `lower_poller` — `poller <name>` blocks (L0 #8). Surfaces the
//!   `POLLER-MISSING-FIELD` / `POLLER-UNKNOWN-ENUM` diagnostics at
//!   lowering time; doctor handles the cross-feature reachability
//!   rules (terminal-state existence, handler orphan, cursor-shape
//!   parity). The two default constants
//!   (`POLLER_DEFAULT_TICK_EVERY = "30s"`,
//!   `POLLER_DEFAULT_TICK_BATCH = 100`) live with the poller body so
//!   they're auditable in one place.
//!
//! Both lowerings stay `pub` because the orchestrator
//! (`feature::lower_feature_skeleton`) calls them, AND because they're
//! re-exported from `lazuli_analyzer` for downstream tools that lower a
//! single job / poller in isolation (codegen tests, fixture tools).
//!
//! ## What does NOT live here
//!
//! The `retry`, `fanout`, `external_call`, `job_trigger`, `job_body`
//! leaf lowerings live in `workflow.rs` — they're shared with webhook /
//! channel / notification / event-group, so they don't belong with the
//! job-and-poller envelopes specifically.

use crate::errors::AnalyzeError;
use crate::expr::{lower_path_string, lower_policy_atom, lower_policy_expr};
use crate::helpers::span_of;
use crate::workflow::{
    lower_external_call, lower_fanout, lower_job_body, lower_job_trigger, lower_retry,
};
use lazuli_ir as ir;
use lazuli_syntax as syntax;

/// Phase L Tier 3 — lower a canonical-indent `job` block into `ir::Job`.
/// Handler-backed bodies lower fully; declarative bodies preserve the
/// raw spine (`raw_target`, `raw_lets`, `raw_effect`) until Tier 4.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::lower_job;
/// use lazuli_syntax::Job;
///
/// let job: Job = unimplemented!("from canonical-indent parse");
/// let lowered = lower_job("Billing", &job)?;
/// assert!(!lowered.name.is_empty());
/// # Ok::<(), lazuli_analyzer::AnalyzeError>(())
/// ```
pub fn lower_job(feature: &str, job: &syntax::Job) -> Result<ir::Job, AnalyzeError> {
    let trigger = lower_job_trigger(feature, &job.trigger);
    let idempotency = job
        .idempotency_by
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::IdempotencyKey { by: path });
    let retry = job.retry.as_ref().map(lower_retry);
    let tenant_from = job
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });
    let fanout = job.fanout.as_ref().map(lower_fanout);
    let external_calls = job.external_calls.iter().map(lower_external_call).collect();
    let policy = job
        .policy
        .as_deref()
        .map(lower_policy_atom)
        .unwrap_or(ir::PolicyRef::None);
    let policy = match policy {
        ir::PolicyRef::None => None,
        other => Some(other),
    };
    let body = lower_job_body(&job.body);

    let policy_expr = job.policy_expr.as_ref().map(lower_policy_expr);
    Ok(ir::Job {
        name: job.name.clone(),
        trigger,
        queue: job.queue.clone(),
        idempotency,
        retry,
        policy,
        policy_expr,
        policy_when_denied: None,
        tenant_from,
        fanout,
        timeout: job.timeout.clone(),
        external_calls,
        body,
        emits: job.emits.clone(),
        previous_names: Vec::new(),
        span_ref: Some(span_of(job.span)),
    })
}

// =============================================================================
// L0 #8 — poller lowering (docs/proposals/poller-vocab.md §4).
//
// AST → IR is purely structural; doctor rules enforce the closed-catalog
// validity invariants (cursor field shapes, terminal-state existence,
// handler orphan, etc.). The lowering never fails on AST alone — it
// applies the defaults (`tick.every = 30s`, `tick.batch = 100`) and
// surfaces structurally well-formed IR for downstream consumers.
// =============================================================================

/// Default tick interval when `tick every <duration>` is omitted in source.
/// Per proposal §3.8.
const POLLER_DEFAULT_TICK_EVERY: &str = "30s";
const POLLER_DEFAULT_TICK_BATCH: u32 = 100;

/// L0 #8 — lower a canonical-indent `poller <name>` block into
/// `ir::Poller`, surfacing `POLLER-MISSING-FIELD` / `POLLER-UNKNOWN-ENUM`
/// at lowering time and applying the documented defaults
/// (`tick.every = 30s`, `tick.batch = 100`) for omitted slots.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::lower_poller;
/// use lazuli_syntax::PollerBlockAst;
///
/// let poller: PollerBlockAst = unimplemented!("from canonical-indent parse");
/// let lowered = lower_poller(&poller)?;
/// assert!(!lowered.name.is_empty());
/// # Ok::<(), lazuli_analyzer::AnalyzeError>(())
/// ```
pub fn lower_poller(poller: &syntax::PollerBlockAst) -> Result<ir::Poller, AnalyzeError> {
    let cursor_ast = poller
        .cursor
        .as_ref()
        .ok_or_else(|| AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "cursor".to_owned(),
        })?;
    let retry_ast = poller
        .retry
        .as_ref()
        .ok_or_else(|| AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "retry".to_owned(),
        })?;
    let resolve_name =
        poller
            .resolve_handler
            .as_deref()
            .ok_or_else(|| AnalyzeError::MissingField {
                kind: "poller".to_owned(),
                name: poller.name.clone(),
                field: "resolve via @fn.<name>".to_owned(),
            })?;
    if poller.idempotency.is_empty() {
        return Err(AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "idempotency".to_owned(),
        });
    }
    if poller.states.is_empty() {
        return Err(AnalyzeError::MissingField {
            kind: "poller".to_owned(),
            name: poller.name.clone(),
            field: "states".to_owned(),
        });
    }

    let cursor = ir::PollerCursor {
        next_at_field: cursor_ast.next_at_field.clone(),
        resolved_at_field: cursor_ast.resolved_at_field.clone(),
        attempts_field: cursor_ast.attempts_field.clone(),
        span_ref: Some(span_of(cursor_ast.span)),
    };

    let backoff = match retry_ast.backoff_strategy.as_str() {
        "fixed" => ir::PollerBackoff::Fixed {
            base: retry_ast.backoff_base.clone(),
        },
        "linear" => ir::PollerBackoff::Linear {
            base: retry_ast
                .backoff_base
                .clone()
                .unwrap_or_else(|| "30s".to_owned()),
            cap: retry_ast.backoff_cap.clone(),
        },
        "exponential" => ir::PollerBackoff::Exponential {
            base: retry_ast
                .backoff_base
                .clone()
                .unwrap_or_else(|| "30s".to_owned()),
            cap: retry_ast.backoff_cap.clone(),
        },
        other => {
            return Err(AnalyzeError::UnknownEnum {
                kind: format!("poller `{}` backoff", poller.name),
                value: other.to_owned(),
            });
        }
    };
    let retry = ir::PollerRetry {
        max_attempts: retry_ast.max_attempts,
        backoff,
        span_ref: Some(span_of(retry_ast.span)),
    };

    let states = poller
        .states
        .iter()
        .map(|s| ir::PollerState {
            name: s.name.clone(),
            kind: match s.kind_keyword.as_deref() {
                Some("initial") => ir::PollerStateKind::Initial,
                Some("terminal") => ir::PollerStateKind::Terminal,
                Some("intermediate") | None => ir::PollerStateKind::Intermediate,
                Some(_) => ir::PollerStateKind::Intermediate,
            },
            span_ref: Some(span_of(s.span)),
        })
        .collect::<Vec<_>>();

    let tick = match poller.tick.as_ref() {
        Some(t) => ir::PollerTick {
            every: t.every.clone(),
            batch: t.batch.unwrap_or(POLLER_DEFAULT_TICK_BATCH),
        },
        None => ir::PollerTick {
            every: POLLER_DEFAULT_TICK_EVERY.to_owned(),
            batch: POLLER_DEFAULT_TICK_BATCH,
        },
    };

    let tenant_from = poller
        .tenant_from
        .as_deref()
        .map(lower_path_string)
        .map(|path| ir::TenantFromSpec { path });

    let idempotency = ir::IdempotencyKey {
        by: ir::Path {
            segments: poller.idempotency.to_vec(),
        },
    };

    let audit = poller.audit.as_deref().map(|raw: &str| {
        let rest = raw.strip_prefix("audit ").unwrap_or(raw).trim();
        if rest == "default" {
            ir::AuditSpec {
                subjects: vec!["actor".to_owned(), "target.id".to_owned()],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
                materialize: None,
            }
        } else if let Some(reason) = rest.strip_prefix("none ") {
            ir::AuditSpec {
                subjects: vec![format!("none {}", reason)],
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
                materialize: None,
            }
        } else {
            ir::AuditSpec {
                subjects: rest
                    .split(',')
                    .map(str::trim)
                    .filter(|s: &&str| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                emit_to: None,
                data_subject: None,
                record_before: false,
                record_after: false,
                retain_for: None,
                materialize: None,
            }
        }
    });

    let retry_quirks = poller
        .retry_quirks
        .iter()
        .filter_map(|q| match q.kind.as_str() {
            "gender_flip_once" => Some(ir::PollerRetryQuirk::GenderFlipOnce {
                when: q.when.clone(),
                counter_field: q.counter_field.clone(),
                gender_field: q.mutate_field.clone(),
            }),
            // Unknown catalog entries are dropped during lowering;
            // doctor `POLLER-QUIRK-CATALOG-MISMATCH-001` surfaces the
            // diagnostic at the AST layer.
            _ => None,
        })
        .collect();

    Ok(ir::Poller {
        name: poller.name.clone(),
        source: poller.source.clone(),
        cursor,
        retry,
        states,
        resolve_handler: ir::HandlerRef {
            namespace: "fn".to_owned(),
            name: resolve_name.to_owned(),
            span_ref: Some(span_of(poller.span)),
        },
        terminal_status_field: poller.terminal_status_field.clone(),
        terminal_result_field: poller.terminal_result_field.clone(),
        tick,
        tenant_from,
        idempotency,
        audit,
        emits: poller.emits.clone(),
        retry_quirks,
        span_ref: Some(span_of(poller.span)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poller_defaults_have_expected_values() {
        // The default-tick constants are part of the documented surface
        // (proposal §3.8); pin them so refactors don't silently change.
        assert_eq!(POLLER_DEFAULT_TICK_EVERY, "30s");
        assert_eq!(POLLER_DEFAULT_TICK_BATCH, 100);
    }
}
