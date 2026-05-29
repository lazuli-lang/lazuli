//! session_cookie_samesite_none_insecure_001 — `auth.sessions.cookie`
//! declares `same_site none` without `secure true`.
//!
//! Browsers reject a `Set-Cookie` carrying `SameSite=None` unless it also
//! carries `Secure` (the cookie is silently dropped). A session cookie
//! that the browser refuses to store breaks login entirely, so this is a
//! correctness *and* security defect, not mere hygiene.
//!
//! Cross-axis grammar rule (profile-independent): the trigger is the pair
//! `same_site == Some("none")` AND `secure != Some(true)`. An explicit
//! `secure false` and an *absent* `secure` axis both fire — the absent
//! case because the author wrote `same_site none` expecting it to take
//! effect, but the runtime's default `Secure` only holds when the author
//! has not contradicted it elsewhere; declaring `same_site none` without
//! pairing `secure true` is the exact shape browsers reject, and the rule
//! makes the dependency explicit. The fix is one line: add `secure true`.
//!
//! Severity: **error** under strict/production (Security), WARNING under
//! prototype — same posture as the session-family enforcement peers.
//!
//! Reference: docs/proposals/cookie-sessions-child.md §Doctor (row
//! `SESSION-COOKIE-SAMESITE-NONE-INSECURE-001`).

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ──────────────────────────────────────────────────────────────────

/// One `auth.sessions.cookie` pairing `same_site none` with a non-`true`
/// `secure` axis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` path the offending `cookie` block lives in.
    pub path: PathBuf,
    /// Feature owning the `auth.sessions.cookie` block.
    pub feature: String,
    /// Byte offset of the `cookie` block header for source anchoring.
    pub offset: Option<usize>,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "SESSION-COOKIE-SAMESITE-NONE-INSECURE-001";

    /// Render the remediation message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = finding.message();
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`auth.sessions.cookie` in feature `{feature}` declares `same_site none` without `secure true`; browsers drop a `SameSite=None` cookie that is not also `Secure`, breaking session storage. Add `secure true`.",
            feature = self.feature,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run session_cookie_samesite_none_insecure_001 on a single feature.
///
/// Returns a single finding when the feature's `auth.sessions.cookie`
/// declares `same_site == Some("none")` (ASCII-case-insensitive) AND
/// `secure != Some(true)`. Empty otherwise.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_ir::Feature;
/// // let findings = check(&feature, Path::new("auth.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let Some(cookie) = feature
        .auth
        .as_ref()
        .and_then(|a| a.sessions.as_ref())
        .and_then(|s| s.cookie.as_ref())
    else {
        return Vec::new();
    };
    let same_site_none = cookie
        .same_site
        .as_deref()
        .is_some_and(|v| v.eq_ignore_ascii_case("none"));
    if same_site_none && cookie.secure != Some(true) {
        return vec![Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            offset: cookie.span_ref.as_ref().map(|s| s.start),
        }];
    }
    Vec::new()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use lazuli_ir::SessionCookie;

    use super::*;
    use crate::doctor::auth::session_cookie_test_support::feature_with_cookie;

    fn cookie(same_site: Option<&str>, secure: Option<bool>) -> SessionCookie {
        SessionCookie {
            name: None,
            same_site: same_site.map(str::to_owned),
            secure,
            http_only: None,
            domain: None,
            path: None,
            span_ref: None,
        }
    }

    #[test]
    fn fires_on_samesite_none_without_secure_true() {
        let feature = feature_with_cookie(cookie(Some("none"), None));
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "SESSION-COOKIE-SAMESITE-NONE-INSECURE-001");
        assert!(findings[0].message().contains("same_site none"));
    }

    #[test]
    fn fires_on_samesite_none_with_secure_false() {
        let feature = feature_with_cookie(cookie(Some("none"), Some(false)));
        assert_eq!(check(&feature, Path::new("auth.lzi")).len(), 1);
    }

    #[test]
    fn silent_on_samesite_none_with_secure_true() {
        let feature = feature_with_cookie(cookie(Some("none"), Some(true)));
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn silent_on_samesite_lax() {
        let feature = feature_with_cookie(cookie(Some("lax"), None));
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn silent_when_no_cookie_block() {
        let feature = crate::doctor::auth::session_cookie_test_support::feature_no_cookie();
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }
}
