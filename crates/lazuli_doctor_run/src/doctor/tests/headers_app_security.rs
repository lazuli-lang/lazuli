    // Doctor cookie + proxy + limits + app-headers + referrer + secret-rotation + encryption-key tests
    // Split from crates/lazuli_cli/src/doctor/tests.rs.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    // ---------------------------------------------------------------

    #[test]
    fn doctor_rejects_cookie_same_site_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  cookie
    default
      same_site loose
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_cookie_contract_diagnostics"),
            "expected app_cookie_contract_diagnostics for unknown same_site; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_cookie_max_age_unparseable() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  cookie
    default
      max_age "forever"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_cookie_contract_diagnostics"),
            "expected app_cookie_contract_diagnostics for unparseable max_age; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_cookie_block_in_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  cookie
    default
      signed true
      secure true
      http_only true
      same_site strict
      max_age "7d"
    session
      same_site lax
      max_age "12h"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("app_cookie_contract_diagnostics"),
            "cookie block in closed catalog must not raise app_cookie_contract_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_proxy_trusted_unparseable_cidr() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  proxy
    trusted not_a_cidr
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_proxy_contract_diagnostics"),
            "expected app_proxy_contract_diagnostics for unparseable CIDR; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_proxy_real_ip_header_empty() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  proxy
    trusted 10.0.0.0/8
    real_ip_header ""
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_proxy_contract_diagnostics"),
            "expected app_proxy_contract_diagnostics for empty header name; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_proxy_block_with_well_formed_cidrs_and_headers() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  proxy
    trusted 10.0.0.0/8, 172.16.0.0/12, 2001:db8::/32
    real_ip_header X-Forwarded-For
    forwarded_proto_header X-Forwarded-Proto
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("app_proxy_contract_diagnostics"),
            "well-formed proxy block must not raise app_proxy_contract_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_limits_body_size_unparseable() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  limits
    body_size "huge"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_limits_contract_diagnostics"),
            "expected app_limits_contract_diagnostics for unparseable size; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_rejects_limits_timeout_unparseable() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  limits
    timeout "soon"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("app_limits_contract_diagnostics"),
            "expected app_limits_contract_diagnostics for unparseable duration; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn doctor_accepts_limits_block_with_well_formed_literals() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app MyApp
  limits
    body_size "10mb"
    header_size "16kb"
    upload_size "100mb"
    timeout "30s"
"#,
        )]);
        let diagnostics = package.diagnostics();
        assert!(
            !codes(&diagnostics).contains("app_limits_contract_diagnostics"),
            "well-formed limits block must not raise app_limits_contract_diagnostics; got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // =============================================================
    // Roadmap §1.10 — `headers-contract` /
    // `secret-rotation-overlap-contract` /
    // `secret-rotation-binding-unknown` tests.
    // =============================================================

    #[test]
    fn doctor_errors_under_production_when_headers_block_absent() {
        // Production profile errors when the app has no `headers`
        // block at all. Strict + Prototype defer until the author
        // opts in by declaring even a partial block.
        let mut package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  title "Acme CRM"
  environments
    production
"#,
        )]);
        package.security_profile = SecurityProfile::Production;
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("headers-contract"),
            "expected headers-contract under Production profile; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_warns_when_partial_headers_block_misses_required_slots() {
        // Author opted in by declaring a `headers` block but only
        // populated one slot. Strict profile (default) emits a
        // warning naming the missing slots.
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  headers
    csp "default-src 'self'"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("headers-contract"),
            "expected headers-contract when partial headers block omits required slots; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_accepts_full_app_headers_block() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  headers
    csp "default-src 'self'"
    hsts max_age 31536000 include_subdomains preload
    x_frame_options DENY
    x_content_type_options nosniff
    referrer_policy strict-origin-when-cross-origin
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            !codes.contains("headers-contract"),
            "well-formed headers block must not produce headers-contract; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_rejects_unknown_referrer_policy_token() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  headers
    csp "default-src 'self'"
    hsts max_age 31536000
    x_frame_options DENY
    x_content_type_options nosniff
    referrer_policy bogus-policy
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("headers-contract"),
            "expected headers-contract for unknown referrer_policy; got {:?}",
            codes
        );
        let message = diagnostics
            .iter()
            .find(|d| d.code == "headers-contract")
            .map(|d| d.message.as_str())
            .unwrap_or_default();
        assert!(
            message.contains("referrer_policy") || message.contains("bogus-policy"),
            "diagnostic should name referrer_policy or the bad value; got {message}"
        );
    }

    #[test]
    fn doctor_rejects_secret_rotation_overlap_longer_than_cadence() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  secret_rotation default
    cadence 24h
    overlap 48h
    auto_rollback true
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("secret-rotation-overlap-contract"),
            "expected secret-rotation-overlap-contract; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_accepts_secret_rotation_overlap_shorter_than_cadence() {
        let package = package_from_sources(vec![(
            "registry.lzi",
            r#"
registry
  secret_rotation default
    cadence 90d
    overlap 24h
    auto_rollback true
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            !codes.contains("secret-rotation-overlap-contract"),
            "well-formed overlap must not fire; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_rejects_encryption_key_pointing_at_unknown_rotation_profile() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
      rotation manual
      rotation_profile not_declared
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            codes.contains("secret-rotation-binding-unknown"),
            "expected secret-rotation-binding-unknown for missing profile; got {:?}",
            codes
        );
    }

    #[test]
    fn doctor_accepts_encryption_key_binding_to_declared_rotation_profile() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
      rotation manual
      rotation_profile default
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  secret_rotation default
    cadence 90d
    overlap 24h
    auto_rollback true
"#,
            ),
        ]);
        let diagnostics = package.diagnostics();
        let codes = codes(&diagnostics);
        assert!(
            !codes.contains("secret-rotation-binding-unknown"),
            "declared profile must satisfy the binding; got {:?}",
            codes
        );
    }

