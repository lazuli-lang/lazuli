//! session_cookie_missing_001 — `auth.sessions` runs a refresh/rotation
//! flow but pins no cookie transport, and no app-wide `app.cookie` profile
//! covers the session cookie either.
//!
//! Once a session uses two-token rotation (or the legacy `refresh true`),
//! the refresh cookie crosses the HTTP edge on every silent re-auth. With
//! neither an `auth.sessions.cookie` block nor an `app.cookie` hygiene
//! profile, the cookie's `Secure` / `SameSite` / `Path` posture is left
//! entirely to the runtime literals — invisible at the `.lzi` surface and
//! easy to drift on. This rule nudges the author to pin the transport
//! envelope explicitly at one of the two anchor positions.
//!
//! Trigger: `rotation.is_some()` OR `refresh == true`, AND
//! `sessions.cookie.is_none()`, AND the app manifest declares no
//! session-cookie transport at *either* app anchor:
//!   - no `app.cookie` profile (an `app.cookie` block with at least one
//!     profile covers the session cookie — the reserved `default` profile
//!     applies app-wide), and
//!   - no refresh-cookie *capability* (`refresh_token_storage cookie` or a
//!     `cookie_domain` capability). The proposal pins these edge
//!     capabilities to the session cookie's transport
//!     (`auth-refresh/happy.lzi:18` + `rule_009_cookie_domain`), so an app
//!     that declares them HAS expressed where the refresh cookie lives —
//!     the `app.cookie`-block requirement is satisfied by the capability
//!     anchor too. The dispatcher passes this as `app_declares_cookie`.
//!
//! A plain single-token session (no rotation, `refresh false`) does NOT
//! fire: there is no refresh cookie to govern, and the runtime's
//! session-cookie defaults are sufficient.
//!
//! Severity: **hint** (Security category, advisory posture). It is the
//! one rule in the family that fires on cookie *absence*, and the runtime
//! already stamps safe session-cookie defaults, so it rides the softest,
//! never-blocking tier — a nudge to pin the transport explicitly, not a
//! wire-breaking defect. This keeps existing rotation apps that lean on
//! runtime defaults (no `cookie` child yet) free of any warning/error
//! regression. The cross-axis browser-reject and production-insecure rules
//! are the blocking members of the family; PROFILE-CONFLICT / HOST-PREFIX
//! warn (they only fire on a declared cookie).
//!
//! Reference: docs/proposals/cookie-sessions-child.md §Doctor (row
//! `SESSION-COOKIE-MISSING-001`).

use std::path::{Path, PathBuf};

use lazuli_ir::{AppCookie, Feature};

// ── output ──────────────────────────────────────────────────────────────────

/// One refresh/rotation `auth.sessions` with no session-cookie transport
/// pinned anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` path the offending `auth.sessions` block lives in.
    pub path: PathBuf,
    /// Feature owning the `auth.sessions` block.
    pub feature: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "SESSION-COOKIE-MISSING-001";

    /// Render the remediation message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = finding.message();
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`auth.sessions` in feature `{feature}` runs a refresh/rotation flow but pins no cookie transport, and no `app.cookie` profile covers it. Declare an `auth.sessions.cookie` block (or an `app.cookie` profile) so the refresh cookie's `secure`/`same_site`/`path` posture is explicit.",
            feature = self.feature,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Returns `true` when an `app.cookie` block declares at least one cookie
/// hygiene profile (the reserved `default` profile applies app-wide, so
/// any profile covers the session cookie).
fn app_cookie_covers(app_cookie: Option<&AppCookie>) -> bool {
    app_cookie.is_some_and(|c| !c.profiles.is_empty())
}

/// Run session_cookie_missing_001 on a single feature.
///
/// `app_cookie` is the app manifest's `app.cookie` block (or `None` when
/// no manifest / no block is present). `app_declares_cookie` is `true`
/// when the app manifest declares the refresh cookie via capability
/// (`refresh_token_storage cookie` or `cookie_domain`) — the second app
/// anchor the proposal recognises. Returns a single finding when the
/// feature's `auth.sessions` is refresh/rotation-enabled, declares no
/// `cookie` block, and neither app anchor covers it. Empty otherwise.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_ir::Feature;
/// // let findings = check(&feature, Path::new("auth.lzi"), None, false);
/// ```
pub fn check(
    feature: &Feature,
    path: &Path,
    app_cookie: Option<&AppCookie>,
    app_declares_cookie: bool,
) -> Vec<Finding> {
    let Some(sessions) = feature
        .auth
        .as_ref()
        .and_then(|a| a.sessions.as_ref())
    else {
        return Vec::new();
    };
    let refresh_flow = sessions.rotation.is_some() || sessions.refresh;
    if !refresh_flow {
        return Vec::new();
    }
    if sessions.cookie.is_some() {
        return Vec::new();
    }
    // Covered at either app anchor: the `app.cookie` hygiene block or the
    // refresh-cookie capability declaration.
    if app_cookie_covers(app_cookie) || app_declares_cookie {
        return Vec::new();
    }
    vec![Finding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
    }]
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use lazuli_ir::{CookieProfile, SessionCookie};

    use super::*;
    use crate::doctor::auth::session_cookie_test_support::{
        feature_no_cookie, feature_refresh_no_cookie, feature_with_cookie,
    };

    #[test]
    fn fires_on_refresh_flow_without_cookie_or_app_profile() {
        let feature = feature_refresh_no_cookie();
        let findings = check(&feature, Path::new("auth.lzi"), None, false);
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "SESSION-COOKIE-MISSING-001");
        assert!(findings[0].message().contains("refresh/rotation"));
    }

    #[test]
    fn silent_when_app_cookie_profile_covers() {
        let feature = feature_refresh_no_cookie();
        let app_cookie = AppCookie {
            profiles: vec![CookieProfile {
                name: "default".to_owned(),
                ..Default::default()
            }],
            span_ref: None,
        };
        assert!(check(&feature, Path::new("auth.lzi"), Some(&app_cookie), false).is_empty());
    }

    #[test]
    fn silent_when_app_declares_cookie_capability() {
        // The app declares `refresh_token_storage cookie` / `cookie_domain`
        // (the capability anchor) — coverage is satisfied even without an
        // `app.cookie` hygiene block.
        let feature = feature_refresh_no_cookie();
        assert!(check(&feature, Path::new("auth.lzi"), None, true).is_empty());
    }

    #[test]
    fn silent_when_session_cookie_declared() {
        let feature = feature_with_cookie(SessionCookie {
            name: None,
            same_site: None,
            secure: Some(true),
            http_only: None,
            domain: None,
            path: None,
            span_ref: None,
        });
        // feature_with_cookie has refresh=false, but a declared cookie also
        // short-circuits regardless. Assert it stays silent.
        assert!(check(&feature, Path::new("auth.lzi"), None, false).is_empty());
    }

    #[test]
    fn silent_on_single_token_session_no_refresh() {
        // No rotation, refresh=false, no cookie — there is no refresh
        // cookie to govern, so the rule does not fire.
        let feature = feature_no_cookie();
        assert!(check(&feature, Path::new("auth.lzi"), None, false).is_empty());
    }

    #[test]
    fn empty_app_cookie_block_does_not_cover() {
        // An `app.cookie` block with zero profiles does not cover the
        // session cookie.
        let feature = feature_refresh_no_cookie();
        let app_cookie = AppCookie {
            profiles: vec![],
            span_ref: None,
        };
        assert_eq!(
            check(&feature, Path::new("auth.lzi"), Some(&app_cookie), false).len(),
            1
        );
    }
}
