//! session_cookie_host_prefix_violation_001 — a `__Host-`-prefixed cookie
//! name contradicts the prefix's derived invariants.
//!
//! The `__Host-` cookie-name prefix is a browser-enforced contract
//! (RFC 6265bis §4.1.3.2): a cookie whose name starts with `__Host-` MUST
//! be set with `Secure`, MUST NOT carry a `Domain` attribute (host-only),
//! and MUST have `Path=/`. A browser silently rejects a `__Host-` cookie
//! that breaks any of these, so the session cookie would not be stored —
//! the same failure mode as the `SameSite=None`-without-`Secure` reject,
//! but derived from the *name* rather than a cross-axis pair.
//!
//! The proposal keeps `__Host-` / `__Secure-` prefixes OUT of the closed
//! grammar (no `prefix` axis — that would be config-in-DSL) and instead
//! makes them a **doctor lint over the existing axes**. This rule is that
//! lint: it reads the authored `name` and the three axes the prefix
//! constrains, firing one finding per violated invariant.
//!
//! Triggers (any, when `name` starts with `__Host-`):
//!   - `domain` is set (must be host-only),
//!   - `path` is set to anything other than `/`,
//!   - `secure == Some(false)` (must be `Secure`; an absent `secure` defers
//!     to the runtime default `true`, so it is NOT a violation).
//!
//! Severity: **warning** (Security category, derived-invariant hygiene).
//! It catches a browser-reject shape but is a lint over a naming
//! convention the grammar does not enforce, so it warns rather than
//! blocks.
//!
//! Reference: docs/proposals/cookie-sessions-child.md §Doctor (row
//! `SESSION-COOKIE-HOST-PREFIX-VIOLATION-001`) + line 67 (the `__Host-`
//! derived-invariant lint, explicitly "lint sobre eixos fechados, não
//! vocab").

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ──────────────────────────────────────────────────────────────────

/// The `__Host-` prefix this rule lints. Public so tests and the
/// dispatcher share the constant.
pub const HOST_PREFIX: &str = "__Host-";

/// One `__Host-` invariant a `__Host-`-named session cookie violates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.lzi` path the offending `cookie` block lives in.
    pub path: PathBuf,
    /// Feature owning the `auth.sessions.cookie` block.
    pub feature: String,
    /// The authored cookie name (carries the `__Host-` prefix).
    pub name: String,
    /// The violated invariant, e.g. `"domain must be unset"`.
    pub violation: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "SESSION-COOKIE-HOST-PREFIX-VIOLATION-001";

    /// Render the remediation message naming the cookie + the specific
    /// invariant it breaks.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = finding.message();
    /// ```
    pub fn message(&self) -> String {
        format!(
            "Cookie `{name}` uses the `__Host-` prefix but {violation} (feature `{feature}`). Browsers reject a `__Host-` cookie that is not `Secure`, host-only (no `domain`), and `path \"/\"`. Fix the axis or drop the `__Host-` prefix.",
            name = self.name,
            violation = self.violation,
            feature = self.feature,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run session_cookie_host_prefix_violation_001 on a single feature.
///
/// Returns one finding per violated `__Host-` invariant when the session
/// cookie's `name` carries the `__Host-` prefix. Empty when there is no
/// session cookie, the name is absent or unprefixed, or all three
/// invariants hold.
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
    let Some(name) = cookie.name.as_deref() else {
        return Vec::new();
    };
    if !name.starts_with(HOST_PREFIX) {
        return Vec::new();
    }

    let mut violations: Vec<&'static str> = Vec::new();
    // `domain` must be unset (host-only).
    if cookie.domain.is_some() {
        violations.push("sets a `domain` (the prefix requires host-only)");
    }
    // `path` must be `/` when present.
    if cookie.path.as_deref().is_some_and(|p| p != "/") {
        violations.push("sets `path` to something other than `/`");
    }
    // `secure` must not be explicitly false. Absent defers to default true.
    if cookie.secure == Some(false) {
        violations.push("declares `secure false`");
    }

    let name_owned = name.to_owned();
    violations
        .into_iter()
        .map(|violation| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            name: name_owned.clone(),
            violation: violation.to_owned(),
        })
        .collect()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use lazuli_ir::SessionCookie;

    use super::*;
    use crate::doctor::auth::session_cookie_test_support::feature_with_cookie;

    fn host_cookie(domain: Option<&str>, path: Option<&str>, secure: Option<bool>) -> Feature {
        feature_with_cookie(SessionCookie {
            name: Some("__Host-lazuli_session".to_owned()),
            same_site: None,
            secure,
            http_only: None,
            domain: domain.map(str::to_owned),
            path: path.map(str::to_owned),
            span_ref: None,
        })
    }

    #[test]
    fn fires_on_domain_set() {
        let feature = host_cookie(Some(".example.com"), None, None);
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "SESSION-COOKIE-HOST-PREFIX-VIOLATION-001");
        assert!(findings[0].violation.contains("domain"));
        assert!(findings[0].message().contains("__Host-lazuli_session"));
    }

    #[test]
    fn fires_on_non_root_path() {
        let feature = host_cookie(None, Some("/app"), None);
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].violation.contains("path"));
    }

    #[test]
    fn fires_on_secure_false() {
        let feature = host_cookie(None, None, Some(false));
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].violation.contains("secure"));
    }

    #[test]
    fn fires_once_per_violation() {
        let feature = host_cookie(Some(".example.com"), Some("/x"), Some(false));
        let findings = check(&feature, Path::new("auth.lzi"));
        assert_eq!(findings.len(), 3);
    }

    #[test]
    fn silent_on_compliant_host_cookie() {
        // path "/", no domain, secure unset (defaults true) — compliant.
        let feature = host_cookie(None, Some("/"), None);
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn silent_on_compliant_host_cookie_secure_true() {
        let feature = host_cookie(None, Some("/"), Some(true));
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn silent_on_unprefixed_name() {
        let feature = feature_with_cookie(SessionCookie {
            name: Some("lazuli_session".to_owned()),
            same_site: None,
            secure: Some(false),
            http_only: None,
            domain: Some(".example.com".to_owned()),
            path: Some("/x".to_owned()),
            span_ref: None,
        });
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }

    #[test]
    fn silent_when_no_cookie_block() {
        let feature = crate::doctor::auth::session_cookie_test_support::feature_no_cookie();
        assert!(check(&feature, Path::new("auth.lzi")).is_empty());
    }
}
