//! `Notification` — outbound multi-channel dispatch counterpart of webhooks.
//!
//! Multi-channel dispatch keyed on a recipient path. Two structured batching
//! controls are sibling-bucketed to keep their semantics separate from the
//! scalar `rate_limit`:
//!
//! - [`NotificationDigest`] — aggregate N triggers within `every` into one
//!   dispatch per `group_by` key, capped at `max_size`. Template merge
//!   strategy via [`DigestStrategy`] (`merge` collapses payloads;
//!   `append` emits a list).
//! - [`NotificationThrottle`] — per-recipient / per-channel rate limit
//!   with optional immediate `burst`. Keyed on the notification's axes,
//!   not on the caller (which `rate_limit` already covers).

use serde::{Deserialize, Serialize};

use super::job::JobTrigger;
use super::shared::{IdempotencyKey, RetryPolicy, TenantFromSpec};
use crate::{PolicyExpr, PolicyRef, SpanRef, is_false};

/// Phase L Tier 3 — `notification <name>` declarative contract.
///
/// `channel`, `recipient`, `template`, and `trigger` are the
/// notification-specific axes; `tenant_from`, `idempotency`, `retry`,
/// `emits`, and `policy` reuse the same shapes as jobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    pub name: String,
    /// `trigger event <feature>.<event>` or `trigger schedule "<cron>"`.
    pub trigger: JobTrigger,
    /// `channel email, in_app`. Closed catalog enforced by
    /// `NOTIF-CHANNEL-001`: `email`, `in_app`, `sms`, `push`, `slack`,
    /// `discord`, `webhook`.
    pub channels: Vec<String>,
    /// `recipient target.email` — a path captured verbatim. Lowering
    /// keeps the literal so the adapter resolves against the live
    /// payload.
    pub recipient: String,
    /// `template "./outreach/welcome.mjml"`.
    pub template: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<PolicyRef>,
    /// RB.S6 — structured `policy <expr>` form (see `Command.policy_expr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_from: Option<TenantFromSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency: Option<IdempotencyKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    /// Notifications expanded bucket cycle — optional `digest` block.
    /// Aggregates triggers into a single dispatch per window per
    /// `group_by` key. Distinct from `rate_limit` (scalar, per-call) —
    /// digest is per-recipient/per-group structured batching. Doctor:
    /// `NOTIF-DIGEST-001/002/003`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<NotificationDigest>,
    /// Notifications expanded bucket cycle — optional `throttle` block.
    /// Per-recipient / per-channel structured rate-limit with burst.
    /// Distinct from scalar `rate_limit "N per <window>"` used on
    /// `agent` / `auth password` / `command` / `expose http`; throttle
    /// keys on the notification's recipient/channel axes, not on the
    /// caller. Doctor: `NOTIF-THROTTLE-001/002/003`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throttle: Option<NotificationThrottle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Notifications expanded bucket cycle — `digest` sub-block on
/// `notification`. Aggregates N triggers within `every` into one
/// dispatch per `group_by` value, capped at `max_size`. The
/// `template_strategy` closed catalog (`merge` | `append`) describes
/// how the adapter combines the per-trigger payloads when rendering
/// the digest template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationDigest {
    /// `every "15 minutes"` / `every "1 hour"` / `every "1 day"`.
    /// Captured verbatim; doctor `NOTIF-DIGEST-001` rejects shapes
    /// outside `<N> (seconds|minutes|hours|days)`.
    pub every: String,
    /// `group_by <payload-path>` — typically the recipient axis
    /// (`customer_id`, `target.email`). Optional: when absent, the
    /// digest groups globally per notification kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// `max_size <N>` — hard cap on items per digest. Doctor
    /// `NOTIF-DIGEST-002` rejects `<= 0` or `> 10000`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u32>,
    /// `template_strategy merge|append` — closed catalog. None defaults
    /// to `merge` at the adapter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_strategy: Option<DigestStrategy>,
    /// Raw authored `template_strategy` when it was outside the closed
    /// catalog. Kept only so doctor can report `NOTIF-DIGEST-003`
    /// after lowering without widening `DigestStrategy`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid_template_strategy: Option<String>,
}

/// Notifications expanded bucket cycle — closed catalog for
/// `digest template_strategy`. `merge` collapses per-trigger payloads
/// into a single object (last-write-wins per key); `append` emits a
/// list the template iterates over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DigestStrategy {
    Merge,
    Append,
}

/// Notifications expanded bucket cycle — `throttle` sub-block on
/// `notification`. Distinct from scalar `rate_limit "N per <window>"`:
/// throttle keys on recipient and/or channel and supports an
/// `immediate burst` before the bucket starts rejecting. The shape is
/// per-recipient / per-channel / per-burst, not per-caller — that is
/// why it does not reuse the `rate_limit` keyword.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationThrottle {
    /// `max_per "1 hour"` / `max_per "1 day"`. Window over which the
    /// bucket refills. Doctor `NOTIF-THROTTLE-003` rejects shapes
    /// outside `<N> (seconds|minutes|hours|days)`.
    pub max_per: String,
    /// `per_recipient` — when set, the throttle bucket is keyed on the
    /// notification's `recipient <path>` value. At least one of
    /// `per_recipient` or `per_channel` is required by doctor
    /// `NOTIF-THROTTLE-001`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub per_recipient: bool,
    /// `per_channel` — when set, each channel of a multi-channel
    /// notification gets its own bucket. Email and `in_app` are then
    /// throttled independently.
    #[serde(default, skip_serializing_if = "is_false")]
    pub per_channel: bool,
    /// `burst <N>` — number of immediate dispatches the bucket allows
    /// before throttling starts. Useful for OTP/login flows where the
    /// first 1-3 sends must go through without delay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burst: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_strategy_round_trips() {
        let s = serde_json::to_string(&DigestStrategy::Append).unwrap();
        assert_eq!(s, "\"append\"");
    }
}
