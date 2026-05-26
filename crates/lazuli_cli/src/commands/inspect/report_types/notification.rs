//! `--expand=notifications` projection shapes.
//!
//! Notifications carry both a scalar `rate_limit` (forward-compat with
//! the language-wide per-call slot) and a structured `throttle`
//! sub-block. The `digest` sub-block is a separate IR shape. All three
//! live here together because the per-notification record references
//! both sub-types.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectNotification {
    pub(in crate::commands::inspect) name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) channels: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) recipient: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) idempotency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) retry: Option<String>,
    /// Scalar `rate_limit "N per <window>"` captured verbatim. Kept
    /// for forward-compat: the language reserves `rate_limit` as the
    /// per-call scalar slot across `agent`/`auth password`/`command`/
    /// `expose http` and may surface it on `notification` once pilot
    /// pressure requires it. Distinct from the structured `throttle`
    /// sub-block below.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) rate_limit: Option<String>,
    /// Notifications expanded bucket cycle — typed projection of the
    /// `digest` sub-block (`every`/`group_by`/`max_size`/
    /// `template_strategy`). `None` when the notification does not
    /// declare digesting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) digest: Option<InspectNotificationDigest>,
    /// Notifications expanded bucket cycle — typed projection of the
    /// `throttle` sub-block (`max_per`/`per_recipient`/`per_channel`/
    /// `burst`). `None` when the notification does not declare a
    /// throttle bucket. Distinct from scalar `rate_limit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) throttle: Option<InspectNotificationThrottle>,
    pub(in crate::commands::inspect) origin: &'static str,
}

/// Notifications expanded bucket cycle — `--expand=notifications`
/// projection of `ir::NotificationDigest`. Mirrors the IR shape one-
/// to-one so consumers can read the digest contract cold.
#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectNotificationDigest {
    pub(in crate::commands::inspect) every: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) group_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) max_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) template_strategy: Option<String>,
}

/// Notifications expanded bucket cycle — `--expand=notifications`
/// projection of `ir::NotificationThrottle`. Distinct shape from
/// scalar `rate_limit` so the structured per-recipient/per-channel
/// contract surfaces in JSON without being conflated with the scalar
/// slot above.
#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectNotificationThrottle {
    pub(in crate::commands::inspect) max_per: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(in crate::commands::inspect) per_recipient: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(in crate::commands::inspect) per_channel: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) burst: Option<u32>,
}
