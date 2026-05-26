//! Poller IR — `poller <name>` "pending rows" primitive.
//!
//! `poller` is the typed sibling of `job` for the "process pending rows
//! on a resource" pattern that appears in roughly every product (queued
//! work, settlement, refund cleanup, async LLM scoring, …). Where `job`
//! is a generic async-work primitive (one invocation = one unit), a
//! poller binds to a *resource*: it walks rows ready by cursor, calls a
//! `resolve_handler`, and advances state. Every component is typed and
//! closed-catalog — the runtime needs no per-pilot wiring.
//!
//! ## Why a primitive instead of a handler file
//!
//! Without `poller`, the "pending rows" pattern always degraded into a
//! freeform handler file: a custom Go loop with bespoke backoff,
//! bespoke state-machine, bespoke audit. Multiple authors invented
//! incompatible shapes. `poller` locks the shape — cursor field
//! conventions, closed-catalog backoff, closed-catalog state kinds,
//! mandatory idempotency. Lazuli's `inspect`/`doctor` can reason about
//! every poller in the workspace without grepping handler files.
//!
//! ## Closed-catalog escape hatch
//!
//! [`PollerRetryQuirk`] is the carefully bounded escape hatch. v0.1
//! ships one form (`gender_flip_once`) lifted from a pilot's payment
//! retry workflow. New forms require ≥2 products needing them, doctor
//! enforceability, and an explicit L0 review. The constraint exists so
//! the closed catalog doesn't drift into a free-form bag.
//!
//! ## Doctor lattice (per `poller-vocab.md` §5)
//!
//! - State catalog: ≥2 entries, ≥1 [`PollerStateKind::Terminal`].
//! - Cursor fields: each must resolve on `source` resource.
//! - Idempotency: canonical `row.id, row.attempts` shape.
//! - Backoff: closed-catalog enum.
//!
//! ## See also
//!
//! - `docs/proposals/poller-vocab.md` §4 — IR shape.
//! - [`crate::HandlerRef`] — resolve_handler reference shape (shared
//!   with lifecycle `invariant_handler`).
//! - [`crate::Job`] — generalist async-work sibling.

use serde::{Deserialize, Serialize};

use crate::{AuditSpec, HandlerRef, IdempotencyKey, SpanRef, TenantFromSpec};

/// Root IR node for a `poller <name> { … }` block — declarative
/// state-machine that walks a same-feature resource, advancing rows
/// through a typed state space until they reach a terminal kind.
/// Carries cursor + retry + tick wiring plus the resolve handler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poller {
    pub name: String,
    /// Same-feature resource holding the pending rows.
    pub source: String,
    /// Cursor field bindings.
    pub cursor: PollerCursor,
    /// Bounded retry policy.
    pub retry: PollerRetry,
    /// Declared state space; ≥2 entries; ≥1 terminal (doctor enforces).
    pub states: Vec<PollerState>,
    /// Resolution handler reference (`@fn.<name>`).
    pub resolve_handler: HandlerRef,
    /// Optional same-resource field receiving the terminal status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_status_field: Option<String>,
    /// Optional same-resource field receiving the terminal result (JSON).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_result_field: Option<String>,
    /// Tick cadence. Defaults applied at lowering when omitted.
    pub tick: PollerTick,
    /// Tenant axis derivation (`row.<axis>_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    /// Idempotency key — canonical `row.id, row.attempts`.
    pub idempotency: IdempotencyKey,
    /// Audit subjects; defaults to `AuditSpec::Default` semantics when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit: Option<AuditSpec>,
    /// Reactive events published after a row commits a state change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// Retry quirks — closed catalog.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retry_quirks: Vec<PollerRetryQuirk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Cursor field bindings on a [`Poller`]. Names the three resource
/// fields the poller updates: `next_at` (scheduled tick), `resolved_at`
/// (terminal stamp), and `attempts` (retry counter).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerCursor {
    pub next_at_field: String,
    pub resolved_at_field: String,
    pub attempts_field: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Bounded retry policy on a [`Poller`]. Pairs an attempt cap with a
/// typed [`PollerBackoff`] curve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerRetry {
    pub max_attempts: u32,
    pub backoff: PollerBackoff,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed-catalog backoff strategy. `serde(tag = "strategy")` keeps the
/// JSON projection self-describing for inspect consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum PollerBackoff {
    Fixed { base: Option<String> },
    Linear { base: String, cap: Option<String> },
    Exponential { base: String, cap: Option<String> },
}

/// One declared state on a [`Poller`]. At least 2 states are required
/// (with at least 1 terminal); doctor enforces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerState {
    pub name: String,
    pub kind: PollerStateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of poller state kinds. `Initial` is where rows enter;
/// `Terminal` ends the walk; everything else is `Intermediate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PollerStateKind {
    /// Entry state — rows start here.
    Initial,
    /// Walk-through state.
    Intermediate,
    /// Absorbing state — walk stops on entry.
    Terminal,
}

/// Tick cadence on a [`Poller`]. `every` is the verbatim duration
/// literal the runtime parses; `batch` is the per-tick row budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PollerTick {
    /// Verbatim duration literal (`15s`, `1m`); runtime parses.
    pub every: String,
    pub batch: u32,
}

/// Closed-catalog retry quirks (poller-vocab.md §3.13). v0.1 ships ONE
/// form (`gender_flip_once`). New forms require ≥2 products needing
/// them, doctor enforceability, and explicit L0 review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PollerRetryQuirk {
    /// Flip the row's `gender_field` once when `when` matches and
    /// `counter_field < 1`; re-call handler immediately.
    GenderFlipOnce {
        /// Raw predicate text from `when <predicate>` — closed predicate
        /// language enforced by doctor.
        when: String,
        counter_field: String,
        gender_field: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn poller_backoff_tags_strategy() {
        let b = PollerBackoff::Exponential {
            base: "2s".to_owned(),
            cap: Some("1m".to_owned()),
        };
        let value = serde_json::to_value(&b).unwrap();
        assert_eq!(value["strategy"], json!("exponential"));
        let back: PollerBackoff = serde_json::from_value(value).unwrap();
        assert_eq!(back, b);
    }

    #[test]
    fn poller_state_kind_snake_case() {
        let value = serde_json::to_value(PollerStateKind::Intermediate).unwrap();
        assert_eq!(value, json!("intermediate"));
    }

    #[test]
    fn poller_retry_quirk_gender_flip_round_trips() {
        let q = PollerRetryQuirk::GenderFlipOnce {
            when: "result.failed".to_owned(),
            counter_field: "attempts".to_owned(),
            gender_field: "title_gender".to_owned(),
        };
        let value = serde_json::to_value(&q).unwrap();
        assert_eq!(value["kind"], json!("gender_flip_once"));
        let back: PollerRetryQuirk = serde_json::from_value(value).unwrap();
        assert_eq!(back, q);
    }

    #[test]
    fn poller_tick_serializes_flat_fields() {
        let t = PollerTick {
            every: "15s".to_owned(),
            batch: 100,
        };
        let value = serde_json::to_value(&t).unwrap();
        assert_eq!(value["every"], json!("15s"));
        assert_eq!(value["batch"], json!(100));
    }

    #[test]
    fn poller_state_round_trips() {
        let s = PollerState {
            name: "pending".to_owned(),
            kind: PollerStateKind::Initial,
            span_ref: None,
        };
        let json = serde_json::to_value(&s).unwrap();
        let back: PollerState = serde_json::from_value(json).unwrap();
        assert_eq!(back, s);
    }
}
