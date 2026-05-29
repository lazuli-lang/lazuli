//! `--expand=security` projection shapes.
//!
//! The security envelope aggregates four cross-cutting bands per
//! feature: PII / sensitive marker decoration on resource fields and
//! event payloads, the per-operation policy + tenant + rate-limit +
//! audit projection, and the webhook verification record. `InspectAudit`
//! lives here because every `InspectSecurityOperation` carries one.

use serde::Serialize;

use super::auth::InspectOrigin;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectSecurity {
    pub(in crate::commands::inspect) fields: Vec<InspectSecurityField>,
    pub(in crate::commands::inspect) event_payloads: Vec<InspectSecurityEventPayload>,
    pub(in crate::commands::inspect) operations: Vec<InspectSecurityOperation>,
    pub(in crate::commands::inspect) webhooks: Vec<InspectSecurityWebhook>,
    /// `cookie-sessions-child` — the session-cookie transport envelope
    /// lowered from `auth.sessions.cookie`. `None` when the feature has no
    /// `auth.sessions` block, or declared one without a `cookie` child
    /// (the runtime then stamps the hardcoded cookie literals). Additive
    /// axis: present only under `--expand=security`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) session_cookie: Option<InspectSessionCookie>,
}

/// `cookie-sessions-child` — projected session-cookie transport envelope.
/// Mirrors the IR [`lazuli_ir::SessionCookie`] 1:1; every axis is optional
/// so the projection shows exactly which attributes the author declared
/// (absent axes serialize nothing, signalling "runtime keeps its default").
#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectSessionCookie {
    /// The session resource the cookie carries (`UserSession`), echoed so
    /// the envelope is self-describing under the security axis.
    pub(in crate::commands::inspect) resource: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) same_site: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) secure: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) http_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) path: Option<String>,
    pub(in crate::commands::inspect) origin: InspectOrigin,
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
