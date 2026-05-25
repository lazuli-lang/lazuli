//! Authentication IR — identity, password, sessions, MFA, OAuth.
//!
//! `auth` is a **family of related subcontracts** that the language groups
//! into one optional block. Modeling them together keeps the surface visible
//! in one place: a reader looking at `Customer` auth doesn't need to scan
//! three different feature files to know whether sessions rotate, what MFA
//! method is wired, and which OAuth providers are accepted.
//!
//! A feature should not declare more than one identity domain — that
//! invariant is enforced upstream by the analyzer, not encoded structurally
//! here.
//!
//! ## Refresh-token rotation
//!
//! [`AuthSessions`] carries two histories at once:
//!
//! - **Legacy single-token**: [`AuthSessions::ttl`] +
//!   [`AuthSessions::refresh`] (the original Phase 1e shape). Preserved
//!   verbatim for back-compat — existing IR fixtures deserialize unchanged.
//! - **Two-token rotation**: [`AuthSessions::access_ttl`] +
//!   [`AuthSessions::rotation`] (introduced by
//!   `docs/proposals/ir-auth-refresh-rotation.md`). When `rotation` is
//!   `Some(_)`, the legacy slots are silent and the runtime uses the
//!   two-token path.
//!
//! The [`AuthSessions::resolved_*`] helpers read whichever shape is
//! configured and surface the resolved values (defaults baked in) so codegen
//! and the runtime don't have to re-derive the dispatch each time.
//!
//! ## Theft response
//!
//! [`TheftAction`] is a **closed catalog** — adding a new action requires a
//! proposal. The default ([`TheftAction::RevokeSessionFamily`]) walks the
//! offending session's family chain and revokes every row, leaving other
//! devices logged in. [`TheftAction::RevokeUser`] is the heavier-blast-radius
//! alternative reserved for high-stakes apps.
//!
//! ## See also
//!
//! - `docs/ir-abi.md` — JSON ABI governance
//! - `docs/proposals/ir-auth-refresh-rotation.md` — refresh-token rotation
//!   design (§2.A authoring shape, §3.2 lowering shape)
//! - `docs/proposals/cross-feature-contracts.md` §3.5 + §5.3 — public-contract
//!   stamping on the identity slot

use serde::{Deserialize, Serialize};

use crate::{FieldRef, PublicContract, QualifiedName, RateLimitSpec, SpanRef};

/// Authentication is a family of related subcontracts. Modeling them in one
/// optional block keeps identity, password, sessions, MFA, and OAuth visible
/// together. A feature should not declare more than one identity domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Auth {
    pub identity: AuthIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<AuthPassword>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sessions: Option<AuthSessions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mfa: Option<AuthMfa>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth: Vec<AuthOAuthProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `auth identity Customer.email` — one identity field on one resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthIdentity {
    pub field: FieldRef,
    /// Cross-feature contract per `docs/proposals/cross-feature-contracts.md`
    /// §3.5 + §5.3. Populated when the source declares `public contract
    /// identity as v<N>` adjacent to the `auth identity` line. `None`
    /// when no contract is declared (the policy `actor.*` references
    /// cannot cross feature boundaries under `architecture mode
    /// microservices` without doctor erroring).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPassword {
    /// `algorithm argon2id` — required by Phase L. The runtime adapter
    /// pins the KDF; the language records the author's choice verbatim
    /// so doctor can spot legacy/banned algorithms (`md5`, `sha1`, etc.).
    /// Existing JSON payloads that omit this field default to the empty
    /// string for backward compatibility; doctor warns once the slot is
    /// known to be empty.
    #[serde(default)]
    pub algorithm: String,
    /// `hash @fn.hash_customer_password` — extension fn reference.
    pub hash: String,
    pub verify: String,
    /// `rate_limit "5 per 10 minutes"` — declarative throttle parsed by
    /// the auth adapter. Per `ir-rate-limit-env-aware` (cell 1) the slot
    /// accepts the env-qualified `RateLimitSpec` shape; single-line
    /// fixtures lower via `RateLimitSpec::from_default`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimitSpec>,
}

/// `theft_detection_action <verb>` — what the runtime does when a revoked
/// refresh token is used after the grace window has expired AND the
/// session family has a grandchild (i.e. the token has provably been
/// rotated past).
///
/// Closed catalog — adding a new action requires a proposal.
///
/// See `docs/proposals/ir-auth-refresh-rotation.md` §3.1, §2.H.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TheftAction {
    /// Walk the family chain (parents + descendants) of the offending
    /// session; revoke every row. The user stays logged in on other
    /// devices (other session families are untouched). Recommended default.
    RevokeSessionFamily,
    /// Revoke ALL sessions for the user. The user is logged out on every
    /// device. Stronger blast radius — reserve for high-stakes apps.
    RevokeUser,
}

impl Default for TheftAction {
    fn default() -> Self {
        TheftAction::RevokeSessionFamily
    }
}

/// Rotation policy for refresh tokens. Present iff the author declared a
/// `rotation` block under `auth.sessions`. Each inner slot is independently
/// optional — framework defaults kick in when absent (post-§2.B
/// auto-promotion).
///
/// See `docs/proposals/ir-auth-refresh-rotation.md` §2.A (authoring shape),
/// §3.2 (lowering shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationConfig {
    /// Long-lived refresh token TTL. When `None`, framework reads
    /// `"30 days"`. See proposal §2.A.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_ttl: Option<String>,
    /// Grace window for legitimate two-tab refresh races. When `None`,
    /// framework reads `"30 seconds"`. See proposal §2.G.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grace: Option<String>,
    /// What to do when theft is detected (revoked refresh used past
    /// grace with a successor). When `None`, framework reads
    /// `RevokeSessionFamily`. See proposal §2.H.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theft_detection_action: Option<TheftAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessions {
    /// `resource CustomerSession`
    pub resource: QualifiedName,
    /// `ttl "7 days"` — legacy single-token duration. PRESERVED for
    /// back-compat. When `access_ttl` is `None` AND `rotation` is `None`,
    /// the runtime uses this as the session lifetime (current single-token
    /// behavior).
    pub ttl: String,
    /// `refresh false` — legacy bool. Originally a placeholder for the
    /// rotation work landed in proposal `ir-auth-refresh-rotation`.
    /// PRESERVED for back-compat. When `rotation` is `Some(...)`, this
    /// field is silent (the new rotation slot takes over). Doctor warns
    /// on `refresh = true` && `rotation` absent
    /// (AUTH-REFRESH-LEGACY-REFRESH-BOOL).
    pub refresh: bool,
    /// Extra session-table columns beyond the v0 baseline
    /// (`id`, `user`, `token_hash`, `expires_at`, `created_at`).
    /// Empty for single-tenant resources — back-compat guaranteed.
    #[serde(default)]
    pub extra_columns: Vec<SessionExtraColumn>,
    /// IR Auth Refresh — short-lived access token TTL. When rotation is
    /// enabled and this is `None`, the framework reads `"15 minutes"`.
    /// See proposal §2.A, §2.L.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_ttl: Option<String>,
    /// IR Auth Refresh — rotation discipline. Presence = enabled; inner
    /// slots each optional with framework defaults. When `None`, the
    /// runtime uses the single-token path (preserves legacy behavior).
    /// See proposal §2.A.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RotationConfig>,
}

impl AuthSessions {
    /// Returns `true` when the resolved configuration uses two-token
    /// rotation. Used by codegen + runtime to branch.
    pub fn is_rotation_enabled(&self) -> bool {
        self.rotation.is_some()
    }

    /// Resolved access TTL. Reads `access_ttl` if present; else the
    /// framework default when rotation is on (`"15 minutes"`); else
    /// `ttl` (legacy single-token).
    pub fn resolved_access_ttl(&self) -> &str {
        if let Some(t) = self.access_ttl.as_deref() {
            return t;
        }
        if self.is_rotation_enabled() {
            return "15 minutes";
        }
        self.ttl.as_str()
    }

    /// Resolved refresh TTL. Reads `rotation.refresh_ttl` if present;
    /// else the framework default when rotation is on (`"30 days"`);
    /// else `None` (no refresh — single-token mode).
    pub fn resolved_refresh_ttl(&self) -> Option<&str> {
        self.rotation
            .as_ref()
            .map(|r| r.refresh_ttl.as_deref().unwrap_or("30 days"))
    }

    /// Resolved rotation grace. Reads `rotation.grace` if present; else
    /// the framework default when rotation is on (`"30 seconds"`); else
    /// `None`.
    pub fn resolved_rotation_grace(&self) -> Option<&str> {
        self.rotation
            .as_ref()
            .map(|r| r.grace.as_deref().unwrap_or("30 seconds"))
    }

    /// Resolved theft action. Reads explicit value if present; else
    /// framework default when rotation is on (`RevokeSessionFamily`);
    /// else `None`.
    pub fn resolved_theft_action(&self) -> Option<TheftAction> {
        self.rotation
            .as_ref()
            .map(|r| r.theft_detection_action.unwrap_or_default())
    }
}

/// One non-baseline column on a session resource (e.g. `org: Org required`).
/// Populated by the lowering pass from the resource's `FieldSpec` list;
/// consumed by the `auth_session` codegen emitter for typed shim emission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExtraColumn {
    /// DSL field name as declared (`"org"`).
    pub field_name: String,
    /// SQL column name derived from the field (`"org_id"`).
    pub column_name: String,
    /// Go type string for the emitted parameter (`"lazuli.ID"`).
    pub go_type: String,
    /// Referenced resource name if the field is a resource ref (`"Org"`).
    pub references: Option<String>,
    /// Whether the column carries a `required` constraint.
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMfa {
    /// MFA method id: `totp`, `sms`, `webauthn`. Adapter-specific beyond this.
    pub method: String,
    /// `enroll @fn.<name>` — enrollment extension fn reference. Required
    /// by Phase L; legacy payloads default to the empty string.
    #[serde(default)]
    pub enroll: String,
    /// `verify @validator.<name>` or `@fn.<name>` — verification reference.
    /// Required by Phase L; legacy payloads default to the empty string.
    #[serde(default)]
    pub verify: String,
    /// Optional adapter reference, e.g. `@adapter.totp_provider`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthOAuthProvider {
    /// Provider id: `google`, `github`, `microsoft`, etc.
    pub provider: String,
    /// `@adapter.<provider>_oauth` reference.
    pub adapter: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theft_action_default_is_revoke_family() {
        assert_eq!(TheftAction::default(), TheftAction::RevokeSessionFamily);
    }

    #[test]
    fn rotation_disabled_falls_back_to_legacy_ttl() {
        let sessions = AuthSessions {
            resource: QualifiedName {
                feature: None,
                name: "CustomerSession".into(),
            },
            ttl: "7 days".into(),
            refresh: false,
            extra_columns: vec![],
            access_ttl: None,
            rotation: None,
        };
        assert!(!sessions.is_rotation_enabled());
        assert_eq!(sessions.resolved_access_ttl(), "7 days");
        assert_eq!(sessions.resolved_refresh_ttl(), None);
        assert_eq!(sessions.resolved_rotation_grace(), None);
        assert_eq!(sessions.resolved_theft_action(), None);
    }

    #[test]
    fn rotation_enabled_uses_framework_defaults_when_inner_slots_absent() {
        let sessions = AuthSessions {
            resource: QualifiedName {
                feature: None,
                name: "CustomerSession".into(),
            },
            ttl: "7 days".into(),
            refresh: false,
            extra_columns: vec![],
            access_ttl: None,
            rotation: Some(RotationConfig {
                refresh_ttl: None,
                grace: None,
                theft_detection_action: None,
                span_ref: None,
            }),
        };
        assert!(sessions.is_rotation_enabled());
        assert_eq!(sessions.resolved_access_ttl(), "15 minutes");
        assert_eq!(sessions.resolved_refresh_ttl(), Some("30 days"));
        assert_eq!(sessions.resolved_rotation_grace(), Some("30 seconds"));
        assert_eq!(
            sessions.resolved_theft_action(),
            Some(TheftAction::RevokeSessionFamily)
        );
    }

    #[test]
    fn explicit_rotation_values_override_defaults() {
        let sessions = AuthSessions {
            resource: QualifiedName {
                feature: None,
                name: "CustomerSession".into(),
            },
            ttl: "7 days".into(),
            refresh: false,
            extra_columns: vec![],
            access_ttl: Some("5 minutes".into()),
            rotation: Some(RotationConfig {
                refresh_ttl: Some("90 days".into()),
                grace: Some("10 seconds".into()),
                theft_detection_action: Some(TheftAction::RevokeUser),
                span_ref: None,
            }),
        };
        assert_eq!(sessions.resolved_access_ttl(), "5 minutes");
        assert_eq!(sessions.resolved_refresh_ttl(), Some("90 days"));
        assert_eq!(sessions.resolved_rotation_grace(), Some("10 seconds"));
        assert_eq!(
            sessions.resolved_theft_action(),
            Some(TheftAction::RevokeUser)
        );
    }
}
