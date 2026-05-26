//! `--expand=auth` projection shapes (Phase L).
//!
//! Lowered `auth` block from the canonical-indent slice: identity,
//! password, sessions, mfa, oauth — each with its origin tag. The
//! `InspectOrigin` carrier lives here too because every auth sub-block
//! references it; non-auth carriers (`InspectAudit`, security shapes)
//! that also reference an origin use a plain `&'static str` instead.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAuth {
    pub(in crate::commands::inspect) origin: InspectOrigin,
    pub(in crate::commands::inspect) identity: InspectAuthIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) password: Option<InspectAuthPassword>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) sessions: Option<InspectAuthSessions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) mfa: Option<InspectAuthMfa>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::commands::inspect) oauth: Vec<InspectAuthOAuthProvider>,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAuthIdentity {
    /// `<Resource>.<field>` joined back together so downstream consumers
    /// don't need to reassemble it.
    pub(in crate::commands::inspect) field: String,
    pub(in crate::commands::inspect) resource: String,
    pub(in crate::commands::inspect) origin: InspectOrigin,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAuthPassword {
    pub(in crate::commands::inspect) algorithm: String,
    pub(in crate::commands::inspect) hash: String,
    pub(in crate::commands::inspect) verify: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) rate_limit: Option<String>,
    pub(in crate::commands::inspect) origin: InspectOrigin,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAuthSessions {
    pub(in crate::commands::inspect) resource: String,
    pub(in crate::commands::inspect) ttl: String,
    pub(in crate::commands::inspect) refresh: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) access_ttl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) rotation: Option<lazuli_ir::RotationConfig>,
    pub(in crate::commands::inspect) origin: InspectOrigin,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAuthMfa {
    pub(in crate::commands::inspect) method: String,
    pub(in crate::commands::inspect) enroll: String,
    pub(in crate::commands::inspect) verify: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) adapter: Option<String>,
    pub(in crate::commands::inspect) origin: InspectOrigin,
}

#[derive(Debug, Serialize)]
pub(in crate::commands::inspect) struct InspectAuthOAuthProvider {
    pub(in crate::commands::inspect) provider: String,
    pub(in crate::commands::inspect) adapter: String,
    pub(in crate::commands::inspect) origin: InspectOrigin,
}

#[derive(Debug, Serialize, Clone)]
pub(in crate::commands::inspect) struct InspectOrigin {
    pub(in crate::commands::inspect) feature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::commands::inspect) line: Option<usize>,
}
