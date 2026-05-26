//! `--expand=security` projection shapes.
//!
//! The security envelope aggregates four cross-cutting bands per
//! feature: PII / sensitive marker decoration on resource fields and
//! event payloads, the per-operation policy + tenant + rate-limit +
//! audit projection, and the webhook verification record. `InspectAudit`
//! lives here because every `InspectSecurityOperation` carries one.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectSecurity {
    pub(in crate::commands::inspect) fields: Vec<InspectSecurityField>,
    pub(in crate::commands::inspect) event_payloads: Vec<InspectSecurityEventPayload>,
    pub(in crate::commands::inspect) operations: Vec<InspectSecurityOperation>,
    pub(in crate::commands::inspect) webhooks: Vec<InspectSecurityWebhook>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectSecurityField {
    pub(in crate::commands::inspect) resource: String,
    pub(in crate::commands::inspect) field: String,
    pub(in crate::commands::inspect) markers: Vec<String>,
    pub(in crate::commands::inspect) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectSecurityEventPayload {
    pub(in crate::commands::inspect) event: String,
    pub(in crate::commands::inspect) field: String,
    pub(in crate::commands::inspect) markers: Vec<String>,
    pub(in crate::commands::inspect) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectSecurityOperation {
    pub(in crate::commands::inspect) subject: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) tenant_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) scope_reason: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) rate_limits: Vec<String>,
    pub(in crate::commands::inspect) scope_override: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) audit: Option<InspectAudit>,
    pub(in crate::commands::inspect) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAudit {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) fields: Vec<String>,
    /// Observability bucket cycle row 37 — `audit ... emit_to <X>`
    /// destination. `None` means "runtime falls back to the reserved
    /// `audit_log` stream".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) emit_to: Option<String>,
    pub(in crate::commands::inspect) origin: &'static str,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectSecurityWebhook {
    pub(in crate::commands::inspect) webhook: String,
    pub(in crate::commands::inspect) verify: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) secrets: Vec<String>,
    pub(in crate::commands::inspect) origin: &'static str,
}
