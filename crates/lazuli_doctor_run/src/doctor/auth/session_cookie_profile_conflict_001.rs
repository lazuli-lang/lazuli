//! session_cookie_profile_conflict_001 — the same cookie axis is pinned
//! to two *different* explicit values, once by the app-wide `app.cookie`
//! `default` profile and once by `auth.sessions.cookie`.
//!
//! Two anchor positions can dictate the session cookie's transport: the
//! app-wide hygiene profile (`app.cookie`, reserved `default` profile) and
//! the feature-level `auth.sessions.cookie`. Precedence is **nailed** in
//! the proposal: `auth.sessions.cookie` wins (the more specific override).
//! So this is never a hard error — the runtime always has a single
//! resolved value. It IS a hygiene smell worth surfacing: when both
//! positions set the *same* axis to *different* explicit values, the
//! author has written two contradictory sources of truth and the app-wide
//! intent is being silently overridden. The rule names the axis and states
//! which value wins so the divergence is intentional, not accidental.
//!
//! Comparison baseline is the reserved `default` profile under
//! `app.cookie` (the one profile that applies app-wide as a fallback; see
//! `CookieProfile` IR — "reserved `default` profile applies fallback").
//! Named non-`default` profiles are not compared: which named profile
//! binds to the session cookie is a runtime selection the language does
//! not fix, so comparing against them would guess. Only axes both sides
//! set *explicitly* (`Some` vs `Some`) and to *differing* values count —
//! an axis one side leaves `None` defers to the other with no conflict.
//!
//! Severity: **warning** (Security category, hygiene). Precedence is
//! resolvable, so the rule informs rather than blocks.
//!
//! Reference: docs/proposals/cookie-sessions-child.md §Doctor (row
//! `SESSION-COOKIE-PROFILE-CONFLICT-001`) + §Precedência (line 111: the
//! nailed `auth.sessions.cookie`-wins precedence).

use std::path::{Path, PathBuf};

use lazuli_ir::{AppCookie, CookieProfile, Feature, SessionCookie};

// ── output ──────────────────────────────────────────────────────────────────

/// One axis the `app.cookie` `default` profile and `auth.sessions.cookie`
/// both set to differing explicit values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` path the offending `auth.sessions.cookie` block lives in.
    pub path: PathBuf,
    /// Feature owning the `auth.sessions.cookie` block.
    pub feature: String,
    /// The conflicting axis name (`secure`, `same_site`, `http_only`,
    /// `path`). Sorted/deterministic across findings.
    pub axis: String,
    /// The value the app-wide `default` profile sets for this axis.
    pub app_value: String,
    /// The value `auth.sessions.cookie` sets — the winner under the nailed
    /// precedence.
    pub session_value: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "SESSION-COOKIE-PROFILE-CONFLICT-001";

    /// Render the remediation message naming the axis + both values and
    /// the precedence winner.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = finding.message();
    /// ```
    pub fn message(&self) -> String {
        format!(
            "Cookie axis `{axis}` diverges: `app.cookie` default profile sets `{app}`, `auth.sessions.cookie` sets `{session}`. The feature-level value (`{session}`) wins in feature `{feature}`; align `app.cookie` or drop the override to remove the conflict.",
            axis = self.axis,
            app = self.app_value,
            session = self.session_value,
            feature = self.feature,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Locate the reserved `default` profile under `app.cookie`.
fn default_profile(app_cookie: Option<&AppCookie>) -> Option<&CookieProfile> {
    app_cookie?
        .profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("default"))
}

/// Push a finding when both `app` and `session` set an axis explicitly to
/// differing values. `None` on either side means "defers" — no conflict.
fn diff_bool(
    out: &mut Vec<(&'static str, String, String)>,
    axis: &'static str,
    app: Option<bool>,
    session: Option<bool>,
) {
    if let (Some(a), Some(s)) = (app, session)
        && a != s
    {
        out.push((axis, a.to_string(), s.to_string()));
    }
}

fn diff_str(
    out: &mut Vec<(&'static str, String, String)>,
    axis: &'static str,
    app: Option<&str>,
    session: Option<&str>,
) {
    if let (Some(a), Some(s)) = (app, session) {
        // `same_site` is a closed catalog compared case-insensitively
        // (parity with the parser's catalog validation).
        if !a.eq_ignore_ascii_case(s) {
            out.push((axis, a.to_owned(), s.to_owned()));
        }
    }
}

/// Run session_cookie_profile_conflict_001 on a single feature.
///
/// `app_cookie` is the app manifest's `app.cookie` block. Returns one
/// finding per axis the reserved `default` profile and
/// `auth.sessions.cookie` both set explicitly to differing values, in a
/// stable axis order. Empty when there is no session cookie, no `default`
/// profile, or every shared axis agrees.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_ir::Feature;
/// // let findings = check(&feature, Path::new("auth.lzi"), None);
/// ```
pub fn check(feature: &Feature, path: &Path, app_cookie: Option<&AppCookie>) -> Vec<Finding> {
    let Some(session): Option<&SessionCookie> = feature
        .auth
        .as_ref()
        .and_then(|a| a.sessions.as_ref())
        .and_then(|s| s.cookie.as_ref())
    else {
        return Vec::new();
    };
    let Some(profile) = default_profile(app_cookie) else {
        return Vec::new();
    };

    let mut diffs: Vec<(&'static str, String, String)> = Vec::new();
    // Stable axis order: secure, http_only, same_site, path.
    diff_bool(&mut diffs, "secure", profile.secure, session.secure);
    diff_bool(
        &mut diffs,
        "http_only",
        profile.http_only,
        session.http_only,
    );
    diff_str(
        &mut diffs,
        "same_site",
        profile.same_site.as_deref(),
        session.same_site.as_deref(),
    );
    // The app-wide profile has no `path` axis (`CookieProfile` carries
    // `max_age`, not `path`), so `path` cannot conflict app-wide. Left out
    // intentionally — there is no app-side value to compare against.

    diffs
        .into_iter()
        .map(|(axis, app_value, session_value)| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            axis: axis.to_owned(),
            app_value,
            session_value,
        })
        .collect()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use lazuli_ir::CookieProfile;

    use super::*;
    use crate::doctor::auth::session_cookie_test_support::feature_with_cookie;

    fn session(secure: Option<bool>, same_site: Option<&str>, http_only: Option<bool>) -> Feature {
        feature_with_cookie(SessionCookie {
            name: None,
            same_site: same_site.map(str::to_owned),
            secure,
            http_only,
            domain: None,
            path: None,
            span_ref: None,
        })
    }

    fn default_app_cookie(
        secure: Option<bool>,
        same_site: Option<&str>,
        http_only: Option<bool>,
    ) -> AppCookie {
        AppCookie {
            profiles: vec![CookieProfile {
                name: "default".to_owned(),
                signed: None,
                secure,
                http_only,
                same_site: same_site.map(str::to_owned),
                max_age: None,
                span_ref: None,
            }],
            span_ref: None,
        }
    }

    #[test]
    fn fires_on_divergent_secure_axis() {
        let feature = session(Some(true), None, None);
        let app = default_app_cookie(Some(false), None, None);
        let findings = check(&feature, Path::new("auth.lzi"), Some(&app));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "SESSION-COOKIE-PROFILE-CONFLICT-001");
        assert_eq!(findings[0].axis, "secure");
        assert_eq!(findings[0].app_value, "false");
        assert_eq!(findings[0].session_value, "true");
        // The message states the feature-level value wins.
        assert!(findings[0].message().contains("wins"));
    }

    #[test]
    fn fires_per_axis_in_stable_order() {
        let feature = session(Some(true), Some("strict"), Some(true));
        let app = default_app_cookie(Some(false), Some("lax"), Some(false));
        let findings = check(&feature, Path::new("auth.lzi"), Some(&app));
        let axes: Vec<&str> = findings.iter().map(|f| f.axis.as_str()).collect();
        assert_eq!(axes, vec!["secure", "http_only", "same_site"]);
    }

    #[test]
    fn silent_when_axes_agree() {
        let feature = session(Some(true), Some("lax"), Some(true));
        let app = default_app_cookie(Some(true), Some("lax"), Some(true));
        assert!(check(&feature, Path::new("auth.lzi"), Some(&app)).is_empty());
    }

    #[test]
    fn silent_when_app_leaves_axis_unset() {
        // App profile defers (None) on `secure`; session sets it. No
        // conflict — the session value applies with nothing to override.
        let feature = session(Some(false), None, None);
        let app = default_app_cookie(None, None, None);
        assert!(check(&feature, Path::new("auth.lzi"), Some(&app)).is_empty());
    }

    #[test]
    fn silent_when_no_default_profile() {
        // Only a named, non-`default` profile exists — not compared.
        let feature = session(Some(true), None, None);
        let app = AppCookie {
            profiles: vec![CookieProfile {
                name: "session".to_owned(),
                secure: Some(false),
                ..Default::default()
            }],
            span_ref: None,
        };
        assert!(check(&feature, Path::new("auth.lzi"), Some(&app)).is_empty());
    }

    #[test]
    fn silent_when_no_session_cookie() {
        let feature = crate::doctor::auth::session_cookie_test_support::feature_no_cookie();
        let app = default_app_cookie(Some(false), None, None);
        assert!(check(&feature, Path::new("auth.lzi"), Some(&app)).is_empty());
    }

    #[test]
    fn samesite_compared_case_insensitively() {
        // Same value, different case — not a conflict.
        let feature = session(None, Some("None"), None);
        let app = default_app_cookie(None, Some("none"), None);
        // session same_site=None would trip the samesite-insecure rule
        // elsewhere, but THIS rule only compares against the app value;
        // identical (case-insensitive) -> no conflict.
        assert!(check(&feature, Path::new("auth.lzi"), Some(&app)).is_empty());
    }
}
