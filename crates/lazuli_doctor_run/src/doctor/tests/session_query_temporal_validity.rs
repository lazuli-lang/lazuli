// SESSION-QUERY-TEMPORAL-VALIDITY-001 — end-to-end dispatch tests.
//
// These run the full `DoctorPackage::diagnostics()` path (strict
// profile by default via `package_from_sources`), so they exercise
// the dispatch wiring in `package_methods.rs` + `dispatch.rs`, not
// just the pure-IR `check()` (which has its own unit tests in
// `doctor::auth::session_query_temporal_validity_001`).
//
// The rule promotes the warn-only LSP `active-session-temporal-scope`
// text-scan to a blocking, name-agnostic IR rule.

use super::test_support_core::*;
use super::test_support_packages::*;
use crate::doctor::*;

const CODE: &str = "session-query-temporal-validity";

#[test]
fn fires_on_session_query_without_temporal_bound_name_agnostic() {
    // Query named `live_tokens` (NOT `active_sessions`) over the
    // session resource, with a modifier but no temporal filter. The
    // modifier alone is not sufficient evidence.
    let package = package_from_sources(vec![(
        "x.lzi",
        r#"
feature x
  domain
    resource UserSession
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

    query.list live_tokens
      modifier @query_modifier.active_session_scope

      params
        user_id: ID

      filters
        user.id = params.user_id

  auth
    identity UserSession.email

    sessions
      resource UserSession
      ttl "7 days"
      refresh false
"#,
    )]);
    let diagnostics = package.diagnostics();
    assert!(
        codes(&diagnostics).contains(CODE),
        "expected {CODE} on a session query missing the temporal bound; got {:?}",
        diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
    let hit = diagnostics.iter().find(|d| d.code == CODE).unwrap();
    // Strict profile (package default) -> blocking error.
    assert_eq!(
        hit.severity,
        DoctorSeverity::Error,
        "{CODE} must block under the strict profile"
    );
    assert!(hit.message.contains("live_tokens"));
    assert!(hit.message.contains("UserSession"));
}

#[test]
fn clean_with_explicit_temporal_filter() {
    // Mirrors examples/production-grade/features/auth/auth.lzi:74 —
    // explicit `expires_at > ctx.now`, no modifier.
    let package = package_from_sources(vec![(
        "x.lzi",
        r#"
feature x
  domain
    resource UserSession
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

    query.list active_sessions
      params
        user_id: ID

      filters
        user.id = params.user_id
        expires_at > ctx.now

  auth
    identity UserSession.email

    sessions
      resource UserSession
      ttl "7 days"
      refresh false
"#,
    )]);
    let diagnostics = package.diagnostics();
    assert!(
        !codes(&diagnostics).contains(CODE),
        "an explicit `expires_at > ctx.now` filter must satisfy the rule; got {:?}",
        diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
}

#[test]
fn clean_when_query_targets_non_session_resource() {
    // A list query that scores onto a non-session resource is out of
    // scope. Multi-resource feature so the codegen scorer is active.
    let package = package_from_sources(vec![(
        "x.lzi",
        r#"
feature x
  domain
    resource UserSession
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

    resource AuditLog
      action: Text required

    query.list audit_logs
      params
        kind: Text

      filters
        action = params.kind

  auth
    identity UserSession.email

    sessions
      resource UserSession
      ttl "7 days"
      refresh false
"#,
    )]);
    let diagnostics = package.diagnostics();
    assert!(
        !codes(&diagnostics).contains(CODE),
        "audit_logs targets AuditLog, not the session resource; {CODE} must stay silent; got {:?}",
        diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
}
