// SESSION-COOKIE-* — end-to-end dispatch tests for the five session-cookie
// transport rules over `auth.sessions.cookie`.
//
// These run the full `DoctorPackage::diagnostics()` path (strict profile
// by default via `package_from_sources`), so they exercise the dispatch
// wiring in `package_methods.rs::session_cookie_diagnostics` +
// `dispatch.rs`, not just the pure-IR `check()` functions (which have
// their own unit tests under `doctor::auth::session_cookie_*`).
//
// The `INSECURE-IN-PROD` rule is profile-scoped, so its test promotes the
// package to the `production` profile by mutating `security_profile`
// directly (the field is `pub(super)` within the doctor crate).
//
// See `docs/proposals/cookie-sessions-child.md` §Doctor.

use super::test_support_core::*;
use super::test_support_packages::*;
use crate::doctor::*;

const INSECURE_IN_PROD: &str = "SESSION-COOKIE-INSECURE-IN-PROD-001";
const SAMESITE_NONE: &str = "SESSION-COOKIE-SAMESITE-NONE-INSECURE-001";
const MISSING: &str = "SESSION-COOKIE-MISSING-001";
const PROFILE_CONFLICT: &str = "SESSION-COOKIE-PROFILE-CONFLICT-001";
const HOST_PREFIX: &str = "SESSION-COOKIE-HOST-PREFIX-VIOLATION-001";

/// A complete auth feature with a `cookie` block whose body is the
/// supplied 8-space-indented lines. Single-resource so the session
/// binding resolves cleanly.
fn auth_feature_with_cookie_body(cookie_body: &str) -> String {
    format!(
        r#"
feature customer_auth
  domain
    resource UserSession
      token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity UserSession.email

    sessions
      resource UserSession
      ttl "7 days"
      cookie
{cookie_body}
"#
    )
}

#[test]
fn samesite_none_without_secure_fires_and_blocks_under_strict() {
    let src = auth_feature_with_cookie_body("        same_site none\n");
    let package = package_from_sources(vec![("auth.lzi", &src)]);
    let diagnostics = package.diagnostics();
    assert!(
        codes(&diagnostics).contains(SAMESITE_NONE),
        "expected {SAMESITE_NONE}; got {:?}",
        codes(&diagnostics)
    );
    let hit = diagnostics.iter().find(|d| d.code == SAMESITE_NONE).unwrap();
    // Blocking posture: strict profile (package default) -> error.
    assert_eq!(hit.severity, DoctorSeverity::Error);
    assert_eq!(hit.category, Some(RuleCategory::Security));
    assert!(hit.message.contains("same_site none"));
}

#[test]
fn samesite_none_with_secure_true_is_clean() {
    let src = auth_feature_with_cookie_body("        same_site none\n        secure true\n");
    let package = package_from_sources(vec![("auth.lzi", &src)]);
    let diagnostics = package.diagnostics();
    assert!(
        !codes(&diagnostics).contains(SAMESITE_NONE),
        "SameSite=None + Secure must be clean; got {:?}",
        codes(&diagnostics)
    );
}

#[test]
fn insecure_secure_false_fires_only_under_production() {
    let src = auth_feature_with_cookie_body("        secure false\n");

    // Strict profile (default): the production-scoped rule stays silent.
    let strict = package_from_sources(vec![("auth.lzi", &src)]);
    assert!(
        !codes(&strict.diagnostics()).contains(INSECURE_IN_PROD),
        "INSECURE-IN-PROD must NOT fire under strict; got {:?}",
        codes(&strict.diagnostics())
    );

    // Promote to the production profile -> the rule fires and blocks.
    let mut prod = package_from_sources(vec![("auth.lzi", &src)]);
    prod.security_profile = SecurityProfile::Production;
    // v2 — keep the severity config's profile in sync with the mutated
    // `security_profile` so config-driven severity resolution agrees.
    prod.config.profile = SecurityProfile::Production.into();
    let diagnostics = prod.diagnostics();
    assert!(
        codes(&diagnostics).contains(INSECURE_IN_PROD),
        "INSECURE-IN-PROD must fire under production; got {:?}",
        codes(&diagnostics)
    );
    let hit = diagnostics
        .iter()
        .find(|d| d.code == INSECURE_IN_PROD)
        .unwrap();
    assert_eq!(hit.severity, DoctorSeverity::Error);
    assert!(hit.message.contains("production"));
}

#[test]
fn host_prefix_violation_fires_as_warning() {
    // `__Host-` name with a `domain` set — violates the host-only
    // invariant.
    let src = auth_feature_with_cookie_body(
        "        name \"__Host-lazuli_session\"\n        domain \".example.com\"\n",
    );
    let package = package_from_sources(vec![("auth.lzi", &src)]);
    let diagnostics = package.diagnostics();
    assert!(
        codes(&diagnostics).contains(HOST_PREFIX),
        "expected {HOST_PREFIX}; got {:?}",
        codes(&diagnostics)
    );
    let hit = diagnostics.iter().find(|d| d.code == HOST_PREFIX).unwrap();
    // Hygiene posture: warning even under the strict default.
    assert_eq!(hit.severity, DoctorSeverity::Warning);
    assert_eq!(hit.category, Some(RuleCategory::Security));
    assert!(hit.message.contains("__Host-lazuli_session"));
}

#[test]
fn host_prefix_compliant_cookie_is_clean() {
    let src = auth_feature_with_cookie_body(
        "        name \"__Host-lazuli_session\"\n        path \"/\"\n        secure true\n",
    );
    let package = package_from_sources(vec![("auth.lzi", &src)]);
    assert!(
        !codes(&package.diagnostics()).contains(HOST_PREFIX),
        "compliant __Host- cookie must be clean; got {:?}",
        codes(&package.diagnostics())
    );
}

#[test]
fn missing_fires_when_refresh_flow_has_no_cookie() {
    // refresh true, no cookie block, no app.cookie -> MISSING warns.
    let src = r#"
feature customer_auth
  domain
    resource UserSession
      token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity UserSession.email

    sessions
      resource UserSession
      ttl "7 days"
      refresh true
"#;
    let package = package_from_sources(vec![("auth.lzi", src)]);
    let diagnostics = package.diagnostics();
    assert!(
        codes(&diagnostics).contains(MISSING),
        "expected {MISSING}; got {:?}",
        codes(&diagnostics)
    );
    let hit = diagnostics.iter().find(|d| d.code == MISSING).unwrap();
    // Advisory tier: a never-blocking hint (the runtime has safe defaults).
    assert_eq!(hit.severity, DoctorSeverity::Hint);
    assert_eq!(hit.category, Some(RuleCategory::Security));
}

#[test]
fn missing_silent_when_app_declares_cookie_capability() {
    // The app declares the refresh-cookie capability (`refresh_token_storage
    // cookie` + `cookie_domain`) — the second app anchor — so MISSING is
    // silent even though the feature pins no `cookie` block. Mirrors the
    // canonical `auth-refresh/happy.lzi` fixture.
    let app = r#"
app demo
  capabilities
    refresh_token_storage cookie
    cookie_domain example.test
"#;
    let feature = r#"
feature customer_auth
  domain
    resource UserSession
      token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity UserSession.email

    sessions
      resource UserSession
      ttl "7 days"
      refresh true
"#;
    let package = package_from_sources(vec![("app.lzi", app), ("auth.lzi", feature)]);
    assert!(
        !codes(&package.diagnostics()).contains(MISSING),
        "app capability anchor must cover the session cookie; got {:?}",
        codes(&package.diagnostics())
    );
}

#[test]
fn missing_silent_for_single_token_session() {
    // No rotation, refresh false (default), no cookie — no refresh cookie
    // to govern, so MISSING does not fire.
    let src = r#"
feature customer_auth
  domain
    resource UserSession
      token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity UserSession.email

    sessions
      resource UserSession
      ttl "7 days"
"#;
    let package = package_from_sources(vec![("auth.lzi", src)]);
    assert!(
        !codes(&package.diagnostics()).contains(MISSING),
        "single-token session must not trip MISSING; got {:?}",
        codes(&package.diagnostics())
    );
}

#[test]
fn profile_conflict_fires_when_default_profile_diverges() {
    // app.cookie default profile sets secure false; session cookie sets
    // secure true -> divergence on the `secure` axis.
    let app = r#"
app demo
  cookie
    default
      secure false
"#;
    let feature = auth_feature_with_cookie_body("        secure true\n");
    let package = package_from_sources(vec![("app.lzi", app), ("auth.lzi", &feature)]);
    let diagnostics = package.diagnostics();
    assert!(
        codes(&diagnostics).contains(PROFILE_CONFLICT),
        "expected {PROFILE_CONFLICT}; got {:?}",
        codes(&diagnostics)
    );
    let hit = diagnostics
        .iter()
        .find(|d| d.code == PROFILE_CONFLICT)
        .unwrap();
    assert_eq!(hit.severity, DoctorSeverity::Warning);
    assert_eq!(hit.category, Some(RuleCategory::Security));
    // States which value wins (the session-level override).
    assert!(hit.message.contains("wins"));
}

#[test]
fn clean_cookie_block_emits_no_session_cookie_codes() {
    // A fully-compliant cookie block: secure true, lax, http_only true,
    // path "/". None of the five rules should fire.
    let src = auth_feature_with_cookie_body(
        "        name \"lazuli_session\"\n        same_site lax\n        secure true\n        http_only true\n        path \"/\"\n",
    );
    let package = package_from_sources(vec![("auth.lzi", &src)]);
    let diagnostics = package.diagnostics();
    let found = codes(&diagnostics);
    for code in [
        INSECURE_IN_PROD,
        SAMESITE_NONE,
        MISSING,
        PROFILE_CONFLICT,
        HOST_PREFIX,
    ] {
        assert!(
            !found.contains(code),
            "clean cookie tripped {code}; got {found:?}"
        );
    }
}
