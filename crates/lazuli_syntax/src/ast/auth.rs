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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auth {
    pub identity: AuthIdentity,
    pub password: Option<AuthPassword>,
    pub sessions: Option<AuthSessions>,
    pub mfa: Option<AuthMfa>,
    pub oauth: Vec<AuthOAuthProvider>,
    pub span: Span,
}

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
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthDurationClause {
    pub value: String,
    pub span: Span,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthTheftDetectionActionClause {
    pub action: AuthTheftDetectionAction,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthTheftDetectionAction {
    RevokeSessionFamily,
    RevokeUser,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthOAuthProvider {
    /// Provider id, e.g. `google`, `github`, `microsoft`.
    pub provider: String,
    /// `adapter @adapter.<provider>_oauth` — required.
    pub adapter: String,
    pub span: Span,
}
