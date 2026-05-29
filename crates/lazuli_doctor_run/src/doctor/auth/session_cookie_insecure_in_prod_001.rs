//! session_cookie_insecure_in_prod_001 — `auth.sessions.cookie` declares
//! `secure false` while the deploy profile is `production`.
//!
//! The session cookie carries the authentication token. Stamping it
//! without the `Secure` flag means a browser will replay it over plain
//! HTTP, exposing the session to any network observer. In a `production`
//! deployment that is a session-hijack vector, so this rule blocks.
//!
//! Trigger is `secure == Some(false)` — an *explicit* downgrade. An
//! absent `secure` axis (`None`) is NOT a downgrade: the runtime defaults
//! it to `true` (SEC-H1), so the cookie ships `Secure` and the rule stays
//! silent. (The proposal's "default rebaixado" phrasing covers a future
//! lowered framework default; today the only authorable downgrade is the
//! explicit literal, which is what we gate.)
//!
//! Profile-scoped: only fires when the resolved deploy profile is
//! `production`. Under `prototype`/`strict` an explicit `secure false`
//! during local development is allowed (HTTP-only dev origins), so this
//! rule does not fire there. The dispatcher passes the production flag.
//!
//! Severity: **error** (Security, blocks under production). The companion
//! cross-axis rule `session_cookie_samesite_none_insecure_001` catches the
//! profile-independent `SameSite=None`-without-`Secure` browser-reject
//! shape.
//!
//! Reference: docs/proposals/cookie-sessions-child.md §Doctor (row
//! `SESSION-COOKIE-INSECURE-IN-PROD-001`).

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ──────────────────────────────────────────────────────────────────

/// One `auth.sessions.cookie` that declares `secure false` under a
/// `production` deploy profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` path the offending `cookie` block lives in.
    pub path: PathBuf,
    /// Feature owning the `auth.sessions.cookie` block.
    pub feature: String,
    /// Byte offset of the `cookie` block header (from
    /// `SessionCookie.span_ref`) for source anchoring. `None` when the IR
    /// carried no span.
    pub offset: Option<usize>,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "SESSION-COOKIE-INSECURE-IN-PROD-001";

    /// Render the remediation message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = finding.message();
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`auth.sessions.cookie` in feature `{feature}` declares `secure false` under a `production` deploy profile; the session cookie would replay over plain HTTP. Remove `secure false` (the runtime defaults to `Secure`) or set `secure true`.",
            feature = self.feature,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run session_cookie_insecure_in_prod_001 on a single feature.
///
/// Returns a single finding when the feature's `auth.sessions.cookie`
/// declares `secure == Some(false)` AND `is_production` is true. Empty
/// otherwise — no auth, no sessions, no cookie block, `secure` absent or
/// `true`, or a non-production profile.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_ir::Feature;
/// // let findings = check(&feature, Path::new("auth.lzi"), true);
/// ```
pub fn check(feature: &Feature, path: &Path, is_production: bool) -> Vec<Finding> {
    if !is_production {
        return Vec::new();
    }
    let Some(cookie) = feature
        .auth
        .as_ref()
        .and_then(|a| a.sessions.as_ref())
        .and_then(|s| s.cookie.as_ref())
    else {
        return Vec::new();
    };
    if cookie.secure != Some(false) {
        return Vec::new();
    }
    vec![Finding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
        offset: cookie.span_ref.as_ref().map(|s| s.start),
    }]
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use lazuli_ir::SessionCookie;

    use super::*;
    use crate::doctor::auth::session_cookie_test_support::feature_with_cookie;

    #[test]
    fn fires_on_explicit_secure_false_under_production() {
        let feature = feature_with_cookie(SessionCookie {
            name: None,
            same_site: None,
            secure: Some(false),
            http_only: None,
            domain: None,
            path: None,
            span_ref: None,
        });
        let findings = check(&feature, Path::new("auth.lzi"), true);
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "SESSION-COOKIE-INSECURE-IN-PROD-001");
        assert!(findings[0].message().contains("production"));
        assert!(findings[0].message().contains("secure false"));
    }

    #[test]
    fn silent_on_secure_false_outside_production() {
        // Local-dev HTTP origin: `secure false` is allowed under
        // strict/prototype (is_production = false).
        let feature = feature_with_cookie(SessionCookie {
            name: None,
            same_site: None,
            secure: Some(false),
            http_only: None,
            domain: None,
            path: None,
            span_ref: None,
        });
        assert!(check(&feature, Path::new("auth.lzi"), false).is_empty());
    }

    #[test]
    fn silent_when_secure_absent() {
        // `None` => runtime defaults to `Secure`; not a downgrade.
        let feature = feature_with_cookie(SessionCookie {
            name: None,
            same_site: None,
            secure: None,
            http_only: None,
            domain: None,
            path: None,
            span_ref: None,
        });
        assert!(check(&feature, Path::new("auth.lzi"), true).is_empty());
    }

    #[test]
    fn silent_when_secure_true() {
        let feature = feature_with_cookie(SessionCookie {
            name: None,
            same_site: None,
            secure: Some(true),
            http_only: None,
            domain: None,
            path: None,
            span_ref: None,
        });
        assert!(check(&feature, Path::new("auth.lzi"), true).is_empty());
    }

    #[test]
    fn silent_when_no_cookie_block() {
        let feature = crate::doctor::auth::session_cookie_test_support::feature_no_cookie();
        assert!(check(&feature, Path::new("auth.lzi"), true).is_empty());
    }
}
