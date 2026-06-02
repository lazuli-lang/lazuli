    // Doctor auth identity field + oauth adapter + well-formed/resolves tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_emits_auth_identity_field_unknown_for_missing_field() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

  auth
    identity Session.email
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_identity_field_unknown")
            .collect();
        assert!(
            !hits.is_empty(),
            "expected auth_identity_field_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(hits[0].message.contains("field not found"));
    }

    #[test]
    fn doctor_emits_auth_identity_field_unknown_for_non_identity_shape() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      note: Text required
      expires_at: DateTime required

  auth
    identity Session.note
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_identity_field_unknown")
            .collect();
        assert!(
            !hits.is_empty(),
            "expected auth_identity_field_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(hits[0].message.contains("identity-shaped"));
    }

    #[test]
    fn doctor_emits_auth_oauth_adapter_unbound() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    oauth google
      adapter @adapter.bogus_google_oauth

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("auth_oauth_adapter_unbound"),
            "expected auth_oauth_adapter_unbound; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_resolves_oauth_adapter_via_feature_extensions() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    oauth google
      adapter @adapter.google_oauth

    sessions
      resource Session
      ttl "1 day"
      refresh false

  extensions
    adapter google_oauth: IntegrationAdapter[GoogleOAuth] at "./oauth.go"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("auth_oauth_adapter_unbound"),
            "extension adapter must satisfy oauth adapter lookup; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_well_formed_auth_emits_no_auth_diagnostics() {
        // The canonical-shape positive case. None of the four auth_*
        // diagnostics should fire on a coherent block.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required
      email: @semantic.Email required

  auth
    identity Session.email
    password
      algorithm argon2id
      hash @fn.h
      verify @fn.v

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let surfaced = codes(&diagnostics);
        for code in [
            "AUTH-RATELIMIT-NOOP-001",
            "auth_password_algorithm_hash_mismatch",
            "auth_password_no_session",
            "auth_sessions_resource_unknown",
            "auth_identity_field_unknown",
            "auth_oauth_adapter_unbound",
            "auth_oauth_no_password_alt",
            "auth_session_ttl_too_short",
        ] {
            assert!(
                !surfaced.contains(code),
                "well-formed auth should not emit {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn doctor_resolves_identity_resource_via_feature_uses() {
        // `customer_auth uses customer` — Customer.email is declared
        // in the `customer` feature; auth identity in customer_auth
        // must resolve via the `uses` graph.
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature customer
  domain
    resource Customer
      email: @semantic.Email required

feature customer_auth
  uses customer

  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
      expires_at: DateTime required

  auth
    identity Customer.email
    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("auth_identity_field_unknown"),
            "uses-relative identity resolution failed: {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

