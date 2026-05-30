    // Doctor auth password / sessions / oauth-without-password tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use std::fs;
    use std::path::Path;

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_accepts_well_formed_agent() {
        // Sanity gate: an agent that pins determinism, supplies safety,
        // and uses local read tools whose targets exist emits none of
        // the Cut A error codes.
        let package = package_from_sources(vec![(
            "customer.lzi",
            r#"
feature customer
  domain
    query.lookup by_id by id: ID
      policy @policy.read

  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    safety @validator.pii_email_scrub
    tools
      query.lookup.by_id
    evals
      case mentions_status
        allows output contains "active"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let cut_a_errors = [
            "agent_tool_policy_diagnostics",
            "agent_tool_write_unguarded_diagnostics",
            "agent_discriminator_target_invalid_diagnostics",
            "agent_discriminator_field_invalid_diagnostics",
            "eval_ordered_op_invalid_diagnostics",
            "tool_registry_effect_required_diagnostics",
        ];
        let surfaced = codes(&diagnostics);
        for code in cut_a_errors {
            assert!(
                !surfaced.contains(code),
                "well-formed agent should not emit {code}; got {:?}",
                diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
            );
        }
    }

    // -------------------------------------------------------------------------
    // Phase L — `auth` block cross-feature diagnostics.
    //
    // Auth ids per docs/proposals/bucket-auth-cycle.md §Doctor/LSP:
    //   - auth_password_algorithm_hash_mismatch
    //   - auth_password_no_session
    //   - auth_sessions_resource_unknown
    //   - auth_identity_field_unknown
    //   - auth_oauth_adapter_unbound
    //   - auth_oauth_no_password_alt
    //   - auth_session_ttl_too_short
    // -------------------------------------------------------------------------

    #[test]
    fn doctor_emits_auth_password_algorithm_hash_mismatch() {
        // `auth.password.algorithm bcrypt` diverges from
        // `@cap.Hashed(algorithm:argon2id)` on the session resource's
        // hash field.
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
      algorithm bcrypt
      hash @fn.h
      verify @fn.v
      rate_limit "5 per 10 minutes"

    sessions
      resource Session
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let mismatch: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_password_algorithm_hash_mismatch")
            .collect();
        assert_eq!(
            mismatch.len(),
            1,
            "expected exactly one auth_password_algorithm_hash_mismatch; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert!(
            mismatch[0].message.contains("bcrypt"),
            "diagnostic should cite authored algorithm: {}",
            mismatch[0].message
        );
        assert!(
            mismatch[0].message.contains("argon2id"),
            "diagnostic should cite resource axis: {}",
            mismatch[0].message
        );
    }

    #[test]
    fn doctor_emits_auth_sessions_resource_unknown() {
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
    sessions
      resource BogusSession
      ttl "1 day"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("auth_sessions_resource_unknown"),
            "expected auth_sessions_resource_unknown; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_warns_auth_password_no_session() {
        let package = package_from_sources(vec![(
            "x.lzi",
            r#"
feature x
  domain
    resource Account
      email: @semantic.Email required

  auth
    identity Account.email
    password
      algorithm argon2id
      hash @fn.h
      verify @fn.v
      rate_limit "5 per 10 minutes"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_password_no_session")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one auth_password_no_session; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Warning);
        assert!(hits[0].message.contains("login will not issue sessions"));
    }

    #[test]
    fn doctor_infos_auth_oauth_no_password_alt() {
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
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_oauth_no_password_alt")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one auth_oauth_no_password_alt; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Info);
        assert!(hits[0].message.contains("OAuth-only"));
    }

    #[test]
    fn doctor_warns_auth_session_ttl_too_short() {
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
    sessions
      resource Session
      ttl "30 minutes"
      refresh false
"#,
        )]);
        let diagnostics = package.diagnostics();
        let hits: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "auth_session_ttl_too_short")
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one auth_session_ttl_too_short; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(hits[0].severity, DoctorSeverity::Warning);
        assert!(
            hits[0]
                .message
                .contains("session TTL <1h forces frequent re-login")
        );
    }

