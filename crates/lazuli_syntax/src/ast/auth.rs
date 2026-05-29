//! `auth` block — feature-scoped identity contract.
//!
//! Phase L. One `auth` block per feature; it pins which resource field
//! identifies a principal plus optional password / mfa / sessions /
//! oauth subcontracts.
//!
//! Authoring shape:
//!
//! ```text
//! public contract identity as v1
//! auth
//!   identity Customer.email
//!   password
//!     algorithm argon2id
//!     hash @fn.hash_password
//!     verify @fn.verify_password
//!     rate_limit "5 per 10 minutes"
//!   sessions
//!     resource CustomerSession
//!     ttl "7 days"
//!     access_ttl "15 minutes"
//!     rotation
//!       refresh_ttl "30 days"
//!       grace "30 seconds"
//!       theft_detection_action revoke_session_family
//!   mfa
//!     method totp
//!     enroll @fn.enroll_totp
//!     verify @validator.totp
//!   oauth google
//!     adapter @adapter.google_oauth
//! ```
//!
//! `identity` may be tagged as a `public contract identity as v<N>`
//! (singleton — one identity per feature). The contract is parsed
//! immediately above the `auth` block per cross-feature-contracts §3.5.

use serde::{Deserialize, Serialize};

use super::{PublicContractDeclAst, RateLimitSpecAst, Span};

/// `auth` block — feature-scoped identity contract.
///
/// At most one per feature. `identity` is required; `password`, `sessions`,
/// `mfa`, and zero-or-more `oauth` providers are optional subcontracts.
/// Lowering produces `ir::Auth` with structural 1:1 field correspondence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auth {
    pub identity: AuthIdentity,
    pub password: Option<AuthPassword>,
    pub sessions: Option<AuthSessions>,
    pub mfa: Option<AuthMfa>,
    pub oauth: Vec<AuthOAuthProvider>,
    pub span: Span,
}

/// `identity <Resource>.<field>` — the principal field reference.
///
/// Optionally tagged with a `public contract identity as v<N>` line
/// directly above for cross-feature consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthIdentity {
    /// Raw source text `Customer.email`. Lowering splits into
    /// `FieldRef { resource, field }`.
    pub field: String,
    /// Cross-feature contract per `docs/proposals/cross-feature-contracts.md`
    /// §3.5 + §5.3. Authored as `public contract identity as v<N>`
    /// IMMEDIATELY ABOVE the `auth identity <Resource>.<field>` line.
    /// Singleton (one identity per feature) so no per-name binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub span: Span,
}

/// `password` subcontract — algorithm + hash/verify hooks + optional rate limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPassword {
    /// `algorithm argon2id` — required.
    pub algorithm: String,
    /// `hash @fn.<name>` — extension fn reference.
    pub hash: String,
    /// `verify @fn.<name>` — extension fn reference.
    pub verify: String,
    /// `rate_limit "5 per 10 minutes"` — optional declarative throttle.
    /// Env-aware per `ir-rate-limit-env-aware` cell 1.
    pub rate_limit: Option<RateLimitSpecAst>,
    pub span: Span,
}

/// `sessions` subcontract — session resource binding + TTL + refresh wiring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessions {
    /// `resource CustomerSession` — name only; analyzer resolves the
    /// resource against the feature's domain.
    pub resource: String,
    /// `ttl "7 days"` — duration string parsed by the adapter.
    pub ttl: String,
    /// `refresh true|false` — legacy placeholder retained for back-compat.
    /// When omitted, lowering treats it as `false`.
    pub refresh: bool,
    /// `access_ttl "15 minutes"` — optional short-lived access-token TTL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_ttl: Option<AuthDurationClause>,
    /// `rotation` nested block. Presence enables refresh-token rotation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<AuthSessionRotation>,
    /// `cookie` nested block. Presence declares session-cookie transport
    /// attributes; absence keeps the runtime's hardcoded cookie literals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<AuthSessionCookie>,
    pub span: Span,
}

/// `cookie` nested block inside [`AuthSessions`] — closed-catalog session
/// cookie transport attributes. Sibling of [`AuthSessionRotation`]; each
/// axis is `Option<_>` so a partial block only overrides the axes it names
/// and the runtime keeps its hardcoded literal for the rest. The `cookie`
/// child reuses the same attribute vocabulary as the app-wide `cookie`
/// block (anchored here by the `sessions` parent) per
/// `docs/proposals/cookie-sessions-child.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessionCookie {
    /// `name "lazuli_session"` — optional cookie name override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `same_site lax|strict|none` — closed catalog, validated at parse
    /// time (unknown values are rejected with a clear error).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_site: Option<String>,
    /// `secure true|false` — `Secure` (HTTPS-only) flag override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure: Option<bool>,
    /// `http_only true|false` — `HttpOnly` flag override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_only: Option<bool>,
    /// `domain ".example.com"` — optional cookie `Domain`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// `path "/"` — optional cookie `Path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub span: Span,
}

/// One duration literal authored inside an [`AuthSessions`] / rotation
/// child (e.g. `"15 minutes"`). Span lets diagnostics point at the literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthDurationClause {
    /// Raw duration text verbatim (quotes stripped). Adapter parses.
    pub value: String,
    pub span: Span,
}

/// `rotation` nested block inside [`AuthSessions`] — refresh-token rotation knobs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessionRotation {
    /// `refresh_ttl "30 days"` — optional; IR defaults when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_ttl: Option<AuthDurationClause>,
    /// `grace "30 seconds"` — optional; IR defaults when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace: Option<AuthDurationClause>,
    /// `theft_detection_action <verb>` — optional closed catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theft_detection_action: Option<AuthTheftDetectionActionClause>,
    pub span: Span,
}

/// Span-carrying wrapper around [`AuthTheftDetectionAction`] so doctor
/// can point at the line that authored the `theft_detection_action <verb>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthTheftDetectionActionClause {
    pub action: AuthTheftDetectionAction,
    pub span: Span,
}

/// Closed catalog of theft-detection responses (`revoke_session_family` /
/// `revoke_user`) for [`AuthSessionRotation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthTheftDetectionAction {
    /// Revoke just the compromised refresh family — broader sessions live.
    RevokeSessionFamily,
    /// Hard-revoke every session for the affected user.
    RevokeUser,
}

/// `mfa` subcontract — second-factor enrol/verify hooks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMfa {
    /// MFA method id, e.g. `totp`, `sms`, `webauthn`. Adapter-specific
    /// beyond this.
    pub method: String,
    /// `enroll @fn.<name>` — required extension fn reference.
    pub enroll: String,
    /// `verify @validator.<name>` or `@fn.<name>` — required.
    pub verify: String,
    /// `adapter @adapter.<name>` — optional adapter reference.
    pub adapter: Option<String>,
    pub span: Span,
}

/// One `oauth <provider>` row inside [`Auth`] — adapter-driven third-party
/// identity provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthOAuthProvider {
    /// Provider id, e.g. `google`, `github`, `microsoft`.
    pub provider: String,
    /// `adapter @adapter.<provider>_oauth` — required.
    pub adapter: String,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theft_detection_action_serde_snake_case() {
        let v = serde_json::to_value(AuthTheftDetectionAction::RevokeSessionFamily).unwrap();
        assert_eq!(v, serde_json::json!("revoke_session_family"));
    }

    #[test]
    fn auth_duration_clause_value_preserved() {
        let d = AuthDurationClause {
            value: "15 minutes".into(),
            span: Span::new(0, 0),
        };
        assert_eq!(d.value, "15 minutes");
    }

    #[test]
    fn auth_oauth_provider_minimal_construct() {
        let p = AuthOAuthProvider {
            provider: "google".into(),
            adapter: "@adapter.google_oauth".into(),
            span: Span::new(0, 0),
        };
        assert_eq!(p.provider, "google");
    }
}
