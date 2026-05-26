//! `--expand=webhooks` projection shapes (Phase L Tier 3).
//!
//! Per-feature lifted `ir::Webhook` records. The signature-verify
//! envelope, the optional payload-from typed reference, the replay
//! configuration, the DLQ disposition (an enum spanning
//! `emit`/`handler`/`drop`), and the jobs-IR-shaped retry policy all
//! live here.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectWebhook {
    pub(in crate::commands::inspect) name: String,
    pub(in crate::commands::inspect) route: String,
    pub(in crate::commands::inspect) verify: InspectWebhookVerify,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) idempotency_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) policy: Option<String>,
    pub(in crate::commands::inspect) handler: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) returns: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) emits: Vec<String>,
    // Webhooks expanded cycle — typed envelope reference. Atrito #2:
    // structured ref, not opaque string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) payload_from: Option<InspectWebhookPayloadFrom>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) replay: Option<InspectWebhookReplay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) dlq: Option<InspectWebhookDlq>,
    // Webhooks expanded cycle — Atrito #5: retry shares the jobs IR
    // `RetryPolicy` shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) retry: Option<InspectWebhookRetry>,
    pub(in crate::commands::inspect) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectWebhookVerify {
    pub(in crate::commands::inspect) scheme: &'static str,
    pub(in crate::commands::inspect) algorithm: String,
    pub(in crate::commands::inspect) secret_env: String,
    pub(in crate::commands::inspect) header: String,
}

/// Webhooks expanded cycle — typed payload-from projection. The
/// `path` field is the canonical surface form (`webhook_events.<name>`)
/// so JSON consumers do not have to reconstruct the catalog prefix.
#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectWebhookPayloadFrom {
    pub(in crate::commands::inspect) name: String,
    pub(in crate::commands::inspect) path: String,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectWebhookReplay {
    pub(in crate::commands::inspect) mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) within: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) dedupe_by: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(in crate::commands::inspect) enum InspectWebhookDlq {
    Emit { event: String },
    Handler { path: String },
    Drop { reason: String },
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectWebhookRetry {
    pub(in crate::commands::inspect) count: u32,
    pub(in crate::commands::inspect) backoff: &'static str,
}
