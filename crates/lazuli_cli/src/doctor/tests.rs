//! Doctor command test suite.
//!
//! Extracted verbatim from `doctor/mod.rs` to keep mod.rs lean.
//! Do not add new tests outside of this `mod tests { ... }` block - the
//! wrapper preserves string-literal indentation for the existing tests.

mod tests {
    use crate::doctor::aggregators::auth::auth_diagnostics;
    use crate::doctor::aggregators::cross_feature::{
        APP_URLS_MISSING_MESSAGE, app_urls_missing_diagnostics, scope_owner_column_diagnostics,
    };
    use crate::doctor::aggregators::env_manifest::{
        cap_file_policy_implicit_diagnostics, dedupe_env_contract_diagnostics,
        manifest_required_diagnostics, suppress_env_schema_when_declared,
    };
    use crate::doctor::aggregators::field_health::{
        collect_unresolved_field_refs, field_derived_from_unresolved_diagnostics,
        resource_unique_qualifier_unknown_diagnostics, resource_validates_path_unknown_diagnostics,
    };
    use crate::doctor::aggregators::runtime_version::{
        lazuli_version_001_diagnostics, lazuli_version_002_diagnostics, schema_rich_gap_diagnostics,
    };
    use crate::doctor::aggregators::ts_consumers::{
        import_deprecated_alias_diagnostics, manual_param_coercion_diagnostics,
    };
    use crate::doctor::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    mod test_support_core {
        include!("tests/test_support_core.rs");
    }
    use test_support_core::*;

    mod codegen_pattern {
        include!("tests/codegen_pattern.rs");
    }


    mod test_support_packages {
        include!("tests/test_support_packages.rs");
    }
    use test_support_packages::*;

    mod manifest_plugin {
        include!("tests/manifest_plugin.rs");
    }

    mod surface_command {
        include!("tests/surface_command.rs");
    }

    mod app_manifest_registry {
        include!("tests/app_manifest_registry.rs");
    }

    mod integration_a {
        include!("tests/integration_a.rs");
    }

    mod integration_b {
        include!("tests/integration_b.rs");
    }

    mod integration_c {
        include!("tests/integration_c.rs");
    }

    mod design_query {
        include!("tests/design_query.rs");
    }

    mod version {
        include!("tests/version.rs");
    }

    mod hints_types {
        include!("tests/hints_types.rs");
    }

    mod agents_eval {
        include!("tests/agents_eval.rs");
    }
    mod app_logging {
        include!("tests/app_logging.rs");
    }

    mod scope_derived {
        include!("tests/scope_derived.rs");
    }

    mod event_health {
        include!("tests/event_health.rs");
    }

    mod cors_approval {
        include!("tests/cors_approval.rs");
    }

    mod event_trace_authored {
        include!("tests/event_trace_authored.rs");
    }

    mod expose_cap {
        include!("tests/expose_cap.rs");
    }

    mod auth_a {
        include!("tests/auth_a.rs");
    }

    mod auth_b {
        include!("tests/auth_b.rs");
    }

    mod lifecycle_misc {
        include!("tests/lifecycle_misc.rs");
    }

    mod observability_deprecated {
        include!("tests/observability_deprecated.rs");
    }

    mod locale_messages {
        include!("tests/locale_messages.rs");
    }

    mod policy_cache {
        include!("tests/policy_cache.rs");
    }

    mod openapi_webhook {
        include!("tests/openapi_webhook.rs");
    }

    // =========================================================================

    fn notification_package(extra_children: &str) -> DoctorPackage {
        let source = format!(
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
{extra_children}
"#
        );
        package_from_sources(vec![("package.lzi", source.as_str())])
    }

    fn assert_notification_diag(code: &str, extra_children: &str) {
        let package = notification_package(extra_children);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&code), "expected {code}, got {codes:?}");
    }

    /// `NOTIF-DIGEST-001` fires when `digest every "<duration>"` does
    /// not match the closed shape `<N> (seconds|minutes|hours|days)`.
    #[test]
    fn notif_digest_001_every_invalid_shape() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 month"
      group_by customer_id
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-001"),
            "expected NOTIF-DIGEST-001, got {codes:?}"
        );
    }

    /// `NOTIF-DIGEST-002` fires when `digest max_size` is 0 or above
    /// the 10_000 ceiling. Both extremes are authoring smells: 0 is
    /// dead; > 10k blows up the in-window buffer.
    #[test]
    fn notif_digest_002_max_size_out_of_range() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 hour"
      group_by customer_id
      max_size 99999
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-002"),
            "expected NOTIF-DIGEST-002, got {codes:?}"
        );
    }

    /// `NOTIF-DIGEST-003` fires when `digest template_strategy` is not
    /// in the closed catalog.
    #[test]
    fn notif_digest_003_template_strategy_unknown() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    digest
      every "1 hour"
      group_by customer_id
      template_strategy squash
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-DIGEST-003"),
            "expected NOTIF-DIGEST-003, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-001` fires when neither `per_recipient` nor
    /// `per_channel` is present.
    #[test]
    fn notif_throttle_001_axis_missing() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "1 hour"
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-001"),
            "expected NOTIF-THROTTLE-001, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-002` fires when `burst` is larger than the
    /// parsed `max_per` window.
    #[test]
    fn notif_throttle_002_burst_exceeds_max_per() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "1 second"
      per_recipient
      burst 2
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-002"),
            "expected NOTIF-THROTTLE-002, got {codes:?}"
        );
    }

    /// `NOTIF-THROTTLE-003` fires when `throttle max_per` does not
    /// match `<N> (seconds|minutes|hours|days)`.
    #[test]
    fn notif_throttle_003_max_per_invalid_shape() {
        let package = package_from_sources(vec![(
            "package.lzi",
            r#"
feature customer
  domain
    event_group customer_* on Customer
      payload
        customer_id = id
      event activated

feature customer_outreach
  uses customer
  domain
  notification welcome_email
    channel email
    recipient target.email
    trigger event customer.customer_activated
    template "./welcome.mjml"
    policy @policy.notify
    throttle
      max_per "forever"
      per_recipient
"#,
        )]);
        let diagnostics = package.diagnostics();
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains(&"NOTIF-THROTTLE-003"),
            "expected NOTIF-THROTTLE-003, got {codes:?}"
        );
    }

    /// Two extra cases per new diagnostic, paired with the focused
    /// tests above, give each code three covered variants without
    /// repeating a full package fixture 18 times.
    #[test]
    fn notif_digest_throttle_diagnostics_cover_three_cases_each() {
        for extra in [
            "    digest\n      every forever\n",
            "    digest\n      every \"\"\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-001", extra);
        }
        for extra in [
            "    digest\n      every 1h\n      max_size 0\n",
            "    digest\n      every 1h\n      max_size 10001\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-002", extra);
        }
        for extra in [
            "    digest\n      every 1h\n      template_strategy replace\n",
            "    digest\n      every 1h\n      template_strategy \"merge\"\n",
        ] {
            assert_notification_diag("NOTIF-DIGEST-003", extra);
        }
        for extra in [
            "    throttle\n      max_per 1h\n",
            "    throttle\n      max_per 1h\n      burst 1\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-001", extra);
        }
        for extra in [
            "    throttle\n      max_per 1s\n      per_channel\n      burst 2\n",
            "    throttle\n      max_per 0s\n      per_recipient\n      burst 1\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-002", extra);
        }
        for extra in [
            "    throttle\n      max_per later\n      per_channel\n",
            "    throttle\n      max_per \"1 month\"\n      per_recipient\n",
        ] {
            assert_notification_diag("NOTIF-THROTTLE-003", extra);
        }
    }

    // -------------------------------------------------------------------------
    // AUTH-SESSION-* doctor codes — tenant-pin shim validation
    // -------------------------------------------------------------------------

    fn auth_fact_with_extra_columns(
        feature: &str,
        sessions_resource: &str,
        extra_columns: Vec<ir::SessionExtraColumn>,
    ) -> AuthFacts {
        AuthFacts {
            feature: feature.to_owned(),
            auth: ir::Auth {
                identity: ir::AuthIdentity {
                    field: ir::FieldRef {
                        resource: ir::QualifiedName {
                            feature: None,
                            name: "User".to_owned(),
                        },
                        field: "email".to_owned(),
                    },
                    public_contract: None,
                },
                password: None,
                sessions: Some(ir::AuthSessions {
                    resource: ir::QualifiedName {
                        feature: None,
                        name: sessions_resource.to_owned(),
                    },
                    ttl: "7 days".to_owned(),
                    refresh: false,
                    extra_columns,
                    access_ttl: None,
                    rotation: None,
                }),
                mfa: None,
                oauth: vec![],
                span_ref: None,
            },
            path: PathBuf::from(format!("features/{feature}/{feature}.lzi")),
            line: 1,
            identity_line: 1,
            password_line: None,
            password_algorithm_line: None,
            sessions_line: Some(5),
            sessions_resource_line: Some(6),
            mfa_line: None,
            oauth_lines: BTreeMap::new(),
        }
    }

    fn extra_id_column(field_name: &str) -> ir::SessionExtraColumn {
        ir::SessionExtraColumn {
            field_name: field_name.to_owned(),
            column_name: format!("{field_name}_id"),
            go_type: "lazuli.ID".to_owned(),
            references: Some("Org".to_owned()),
            required: true,
        }
    }

    fn extra_non_id_column(field_name: &str) -> ir::SessionExtraColumn {
        ir::SessionExtraColumn {
            field_name: field_name.to_owned(),
            column_name: field_name.to_owned(),
            go_type: "string".to_owned(),
            references: None,
            required: true,
        }
    }

    fn call_auth_diagnostics(facts: &[AuthFacts]) -> Vec<DoctorDiagnostic> {
        let mut feature_resources: BTreeMap<String, BTreeMap<String, ResourceFact>> =
            BTreeMap::new();
        for fact in facts {
            if let Some(sessions) = fact.auth.sessions.as_ref() {
                let mut resources: BTreeMap<String, ResourceFact> = BTreeMap::new();
                resources.insert(
                    sessions.resource.name.clone(),
                    ResourceFact {
                        path: fact.path.clone(),
                        line: 1,
                        fields: BTreeMap::new(),
                    },
                );
                feature_resources.insert(fact.feature.clone(), resources);
            }
        }
        auth_diagnostics(
            facts,
            &feature_resources,
            &BTreeMap::new(),
            &BTreeMap::new(),
            None,
        )
    }

    #[test]
    fn auth_session_tenant_001_fires_on_non_id_go_type() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_non_id_column("region")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-TENANT-001"),
            "expected AUTH-SESSION-TENANT-001, got {codes:?}"
        );
    }

    #[test]
    fn auth_session_tenant_001_does_not_fire_on_id_type() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-TENANT-001"),
            "AUTH-SESSION-TENANT-001 must not fire for lazuli.ID columns; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_extra_001_fires_on_two_extra_columns() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org"), extra_id_column("workspace")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-EXTRA-001"),
            "expected AUTH-SESSION-EXTRA-001 for 2 extra columns; got {codes:?}"
        );
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "AUTH-SESSION-EXTRA-001")
            .collect();
        assert_eq!(
            errors[0].severity,
            DoctorSeverity::Error,
            "AUTH-SESSION-EXTRA-001 must be error severity"
        );
    }

    #[test]
    fn auth_session_extra_001_does_not_fire_on_one_extra_column() {
        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-EXTRA-001"),
            "AUTH-SESSION-EXTRA-001 must not fire for a single extra column; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_extra_001_does_not_fire_when_no_extra_columns() {
        let fact = auth_fact_with_extra_columns("auth_feature", "TenantSession", vec![]);
        let diagnostics = call_auth_diagnostics(&[fact]);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-EXTRA-001"),
            "AUTH-SESSION-EXTRA-001 must not fire when extra_columns is empty; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_fires_on_issue_session_call_in_handler() {
        let root = temp_project_root("callsite-001-fires");
        let handler_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.go");
        write_file(
            &handler_path,
            r#"package handlers

import "github.com/lazuli-lang/lazuli/runtime/go/lazuli/auth"

func Login(ctx *lazuli.Ctx, input LoginInput) (string, error) {
    token, _, err := auth.IssueSession(ctx, db, userID, auth.SessionAttrs{})
    return token, err
}
"#,
        );

        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            codes.contains("AUTH-SESSION-CALLSITE-001"),
            "expected AUTH-SESSION-CALLSITE-001 for auth.IssueSession in user handler; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_does_not_fire_when_no_extra_columns() {
        let root = temp_project_root("callsite-001-no-extra");
        let handler_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.go");
        write_file(
            &handler_path,
            r#"package handlers

func Login(ctx *lazuli.Ctx, input LoginInput) (string, error) {
    token, _, err := auth.IssueSession(ctx, db, userID, auth.SessionAttrs{})
    return token, err
}
"#,
        );

        let fact = auth_fact_with_extra_columns("auth_feature", "TenantSession", vec![]);
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-CALLSITE-001"),
            "AUTH-SESSION-CALLSITE-001 must not fire when session has no extra columns; got {codes:?}"
        );
    }

    #[test]
    fn auth_session_callsite_001_skips_gen_go_files() {
        let root = temp_project_root("callsite-001-skip-gen");
        let gen_path = root
            .join("features")
            .join("auth_feature")
            .join("handlers")
            .join("login.gen.go");
        write_file(
            &gen_path,
            "func Login() { auth.IssueSession(ctx, db, id, auth.SessionAttrs{}) }\n",
        );

        let fact = auth_fact_with_extra_columns(
            "auth_feature",
            "TenantSession",
            vec![extra_id_column("org")],
        );
        let diagnostics = check_auth_session_callsite_001(&[fact], &root);
        let codes: BTreeSet<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(
            !codes.contains("AUTH-SESSION-CALLSITE-001"),
            "AUTH-SESSION-CALLSITE-001 must not fire for .gen.go files; got {codes:?}"
        );
    }

    // ---------------------------------------------------------------
    // Roadmap §1.2 — HTTP hygiene contracts: cookie / proxy / limits.
    // Each block ships one diagnostic code that fires on any of its
    // closed-catalog violations.
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

    // =========================================================================
    // IR Error-Vocab (Cell ANALYZE-1) — fixture-driven coverage for the 7
    // `ERR-VOCAB-*` diagnostics. Each fixture trips exactly one rule (with
    // ERR-VOCAB-003 occasionally co-firing alongside ERR-VOCAB-001 on the
    // same source — both are legitimate, both warnings).
    //
    // The happy-path fixture asserts ZERO `ERR-VOCAB-*` codes fire.
    //
    // Cross-feature key resolution (`@translation.X` resolved through
    // `uses`) is exercised by `err_vocab_002_silent_through_uses_two_features`.
    //
    // See `docs/proposals/ir-error-messages-vocab.md` §6 §11 Cell ANALYZE-1.
    // =========================================================================

    const ERR_VOCAB_NO_WHEN_DENIED_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/no_when_denied.lzi");
    const ERR_VOCAB_KEY_UNKNOWN_FROM_POLICY_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/key_unknown_from_policy.lzi");
    const ERR_VOCAB_BUILTIN_FALLBACK_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/builtin_fallback.lzi");
    const ERR_VOCAB_CODE_UNKNOWN_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/code_unknown.lzi");
    const ERR_VOCAB_EXPOSE_UNKNOWN_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/expose_unknown.lzi");
    const ERR_VOCAB_WHEN_DENIED_NO_POLICY_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/when_denied_no_policy.lzi");
    const ERR_VOCAB_EXPOSE_5XX_MESSAGE_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/expose_5xx_message.lzi");
    const ERR_VOCAB_HAPPY_FIXTURE: &str =
        include_str!("../../tests/fixtures/error-vocab/happy.lzi");
    const ROUTE_GUARD_HAPPY_LZI: &str = include_str!("../../tests/fixtures/route-guard/happy.lzi");
    const ROUTE_GUARD_HAPPY_LZX: &str = include_str!("../../tests/fixtures/route-guard/happy.lzx");
    const ROUTE_GUARD_UNGUARDED_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/view_unguarded_with_gated_backend.lzx");
    const ROUTE_GUARD_LAXER_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/view_laxer_than_backend.lzx");
    const ROUTE_GUARD_REDIRECT_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/redirect_unreachable.lzx");
    const ROUTE_GUARD_MISSING_ACTOR_LZI: &str =
        include_str!("../../tests/fixtures/route-guard/missing_actor_query.lzi");
    const ROUTE_GUARD_MISSING_ACTOR_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/missing_actor_query.lzx");
    const ROUTE_GUARD_AUDIENCE_LZX: &str =
        include_str!("../../tests/fixtures/route-guard/audience_runtime_disagreement.lzx");
    const LIFECYCLE_GATE_HAPPY_LZI: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/happy.lzi");
    const LIFECYCLE_GATE_HAPPY_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/happy.lzx");
    const LIFECYCLE_GATE_UNKNOWN_RESOURCE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/unknown_resource.lzx");
    const LIFECYCLE_GATE_UNKNOWN_STATE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/unknown_state.lzx");
    const LIFECYCLE_GATE_MISSING_STATE_COVERAGE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/missing_state_coverage.lzx");
    const LIFECYCLE_GATE_EXTRA_STATE_ARM_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/extra_state_arm.lzx");
    const LIFECYCLE_GATE_WILDCARD_OVERUSE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/wildcard_overuse.lzx");
    const LIFECYCLE_GATE_REDIRECT_CYCLE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/redirect_cycle.lzx");
    const LIFECYCLE_GATE_RESUME_RESOURCE_MISMATCH_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/resume_resource_mismatch.lzx");
    const LIFECYCLE_GATE_WRONG_QUERY_KIND_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/wrong_query_kind.lzx");
    const LIFECYCLE_GATE_WITHOUT_ACTOR_GATE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/lifecycle_without_actor_gate.lzx");
    const LIFECYCLE_GATE_CROSS_FEATURE_LZX: &str =
        include_str!("../../tests/fixtures/lifecycle-gate/cross_feature_resume.lzx");

    fn err_vocab_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("ERR-VOCAB-"))
            .collect()
    }

    fn count_code(diagnostics: &[DoctorDiagnostic], code: &str) -> usize {
        diagnostics.iter().filter(|d| d.code == code).count()
    }

    fn route_guard_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("ROUTE-GUARD-"))
            .collect()
    }

    fn lifecycle_gate_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("LIFECYCLE-GATE-"))
            .collect()
    }

    fn route_guard_fixture(lzx: &str) -> DoctorPackage {
        package_from_sources(vec![
            ("happy.lzi", ROUTE_GUARD_HAPPY_LZI),
            ("case.lzx", lzx),
        ])
    }

    fn lifecycle_gate_fixture(lzx: &str) -> DoctorPackage {
        package_from_sources(vec![
            ("happy.lzi", LIFECYCLE_GATE_HAPPY_LZI),
            ("case.lzx", lzx),
        ])
    }

    #[test]
    fn err_vocab_001_fires_for_no_when_denied_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_NO_WHEN_DENIED_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-001"),
            1,
            "expected ERR-VOCAB-001 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_002_fires_for_key_unknown_from_policy_fixture() {
        let package =
            package_from_sources(vec![("app.lzi", ERR_VOCAB_KEY_UNKNOWN_FROM_POLICY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-002"),
            1,
            "expected ERR-VOCAB-002 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_003_fires_for_builtin_fallback_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_BUILTIN_FALLBACK_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-003"),
            1,
            "expected ERR-VOCAB-003 to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_code_unknown_fires_for_code_unknown_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_CODE_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-CODE-UNKNOWN"),
            1,
            "expected ERR-VOCAB-CODE-UNKNOWN to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_expose_unknown_fires_for_expose_unknown_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_EXPOSE_UNKNOWN_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-EXPOSE-UNKNOWN"),
            1,
            "expected ERR-VOCAB-EXPOSE-UNKNOWN to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_when_denied_no_policy_fires_for_when_denied_no_policy_fixture() {
        let package =
            package_from_sources(vec![("app.lzi", ERR_VOCAB_WHEN_DENIED_NO_POLICY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-WHEN-DENIED-NO-POLICY"),
            1,
            "expected ERR-VOCAB-WHEN-DENIED-NO-POLICY to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_expose_5xx_message_fires_for_expose_5xx_message_fixture() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_EXPOSE_5XX_MESSAGE_FIXTURE)]);
        let diagnostics = package.diagnostics();
        assert_eq!(
            count_code(&diagnostics, "ERR-VOCAB-EXPOSE-5XX-MESSAGE"),
            1,
            "expected ERR-VOCAB-EXPOSE-5XX-MESSAGE to fire exactly once; got: {:?}",
            err_vocab_diags(&diagnostics)
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn err_vocab_happy_fixture_fires_no_err_vocab_diagnostics() {
        let package = package_from_sources(vec![("app.lzi", ERR_VOCAB_HAPPY_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let err_vocab: Vec<_> = err_vocab_diags(&diagnostics);
        assert!(
            err_vocab.is_empty(),
            "happy.lzi must emit zero ERR-VOCAB-* diagnostics; got: {:?}",
            err_vocab.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    // Cross-feature key resolution: `feature sales` declares
    // `policies create.when_denied @translation.shared_key` and that key
    // lives in `feature crm`'s translation block. `feature sales`
    // imports it via `uses crm`. ERR-VOCAB-002 must stay silent.
    #[test]
    fn err_vocab_002_silent_through_uses_two_features() {
        const CRM_FIXTURE: &str = r#"
app AcmeApp
  title "Acme"
  version "0.1.0"
  targets
    backend go
  environments
    local
  locale
    default "pt-BR"
    supported "pt-BR"

feature crm
  domain
    resource Customer
      id: ID required

  translation
    catalog "./i18n/crm.<locale>.json"

    key shared_key
      pt-BR "Apenas administradores podem realizar esta ação."
"#;
        const SALES_FIXTURE: &str = r#"
feature sales
  uses crm
  domain
    resource Lead
      id: ID required

  policies
    create: @role.sales
      when_denied @translation.shared_key

  command create
    policy @policy.create
    creates Lead
"#;
        let package =
            package_from_sources(vec![("crm.lzi", CRM_FIXTURE), ("sales.lzi", SALES_FIXTURE)]);
        let diagnostics = package.diagnostics();
        let err_vocab_002 = count_code(&diagnostics, "ERR-VOCAB-002");
        assert_eq!(
            err_vocab_002,
            0,
            "cross-feature `@translation.shared_key` (declared in `crm`, used by `sales`) must \
             resolve through `uses crm`; got ERR-VOCAB-002 diagnostics: {:?}",
            diagnostics
                .iter()
                .filter(|d| d.code == "ERR-VOCAB-002")
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    const AUTH_REFRESH_HAPPY: &str = include_str!("../../tests/fixtures/auth-refresh/happy.lzi");
    const AUTH_REFRESH_001: &str =
        include_str!("../../tests/fixtures/auth-refresh/missing_secret_provider.lzi");
    const AUTH_REFRESH_002: &str =
        include_str!("../../tests/fixtures/auth-refresh/grace_exceeds_refresh_ttl.lzi");
    const AUTH_REFRESH_003: &str =
        include_str!("../../tests/fixtures/auth-refresh/schema_missing_columns.lzi");
    const AUTH_REFRESH_004: &str =
        include_str!("../../tests/fixtures/auth-refresh/revoke_user_missing_user_fk.lzi");
    const AUTH_REFRESH_005: &str =
        include_str!("../../tests/fixtures/auth-refresh/refresh_ttl_long.lzi");
    const AUTH_REFRESH_006: &str =
        include_str!("../../tests/fixtures/auth-refresh/missing_on_refresh_failure.lzi");
    const AUTH_REFRESH_007: &str =
        include_str!("../../tests/fixtures/auth-refresh/auto_promotion_applied.lzi");
    const AUTH_REFRESH_008: &str =
        include_str!("../../tests/fixtures/auth-refresh/auto_refresh_not_surfaced.lzi");
    const AUTH_REFRESH_009: &str =
        include_str!("../../tests/fixtures/auth-refresh/cookie_domain_missing.lzi");

    fn auth_refresh_diags<'a>(diagnostics: &'a [DoctorDiagnostic]) -> Vec<&'a DoctorDiagnostic> {
        diagnostics
            .iter()
            .filter(|d| d.code.starts_with("AUTH-REFRESH-"))
            .collect()
    }

    fn assert_auth_refresh_fixture(source: &str, expected_code: &str) -> Vec<DoctorDiagnostic> {
        let package = package_from_sources(vec![("auth_refresh.lzi", source)]);
        let diagnostics = package.diagnostics();
        let auth_refresh = auth_refresh_diags(&diagnostics);
        assert_eq!(
            auth_refresh.len(),
            1,
            "expected exactly one AUTH-REFRESH diagnostic ({expected_code}); got {:?}",
            auth_refresh
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
        assert_eq!(auth_refresh[0].code, expected_code);
        diagnostics
    }

    #[test]
    fn route_guard_happy_fixture_fires_no_route_guard_diagnostics() {
        let package = package_from_sources(vec![
            ("happy.lzi", ROUTE_GUARD_HAPPY_LZI),
            ("happy.lzx", ROUTE_GUARD_HAPPY_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert!(
            route_guard.is_empty(),
            "happy route guard fixtures must emit zero ROUTE-GUARD-* diagnostics; got: {:?}",
            route_guard
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn route_guard_001_fires_for_unguarded_gated_backend_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_UNGUARDED_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-001"]
        );
    }

    #[test]
    fn route_guard_002_fires_for_laxer_view_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_LAXER_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-002"]
        );
    }

    #[test]
    fn route_guard_003_fires_for_unreachable_redirect_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_REDIRECT_LZX);
        let diagnostics = package.diagnostics();
        assert_eq!(
            route_guard_diags(&diagnostics)
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>(),
            vec!["ROUTE-GUARD-003"]
        );
    }

    #[test]
    fn route_guard_004_fires_as_warning_for_missing_actor_query_fixture() {
        let package = package_from_sources(vec![
            ("missing_actor_query.lzi", ROUTE_GUARD_MISSING_ACTOR_LZI),
            ("missing_actor_query.lzx", ROUTE_GUARD_MISSING_ACTOR_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert_eq!(route_guard.len(), 1, "got {route_guard:?}");
        assert_eq!(route_guard[0].code, "ROUTE-GUARD-004");
        assert_eq!(route_guard[0].severity, DoctorSeverity::Warning);
    }

    #[test]
    fn route_guard_005_fires_as_info_for_runtime_audience_disagreement_fixture() {
        let package = route_guard_fixture(ROUTE_GUARD_AUDIENCE_LZX);
        let diagnostics = package.diagnostics();
        let route_guard = route_guard_diags(&diagnostics);
        assert_eq!(route_guard.len(), 1, "got {route_guard:?}");
        assert_eq!(route_guard[0].code, "ROUTE-GUARD-005");
        assert_eq!(route_guard[0].severity, DoctorSeverity::Info);
    }

    #[test]
    fn lifecycle_gate_happy_fixture_fires_no_lifecycle_gate_diagnostics() {
        let package = package_from_sources(vec![
            ("happy.lzi", LIFECYCLE_GATE_HAPPY_LZI),
            ("happy.lzx", LIFECYCLE_GATE_HAPPY_LZX),
        ]);
        let diagnostics = package.diagnostics();
        let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
        assert!(
            lifecycle_gate.is_empty(),
            "happy lifecycle gate fixtures must emit zero LIFECYCLE-GATE-* diagnostics; got: {:?}",
            lifecycle_gate
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn lifecycle_gate_fixtures_emit_exactly_the_documented_code() {
        for (source, expected) in [
            (LIFECYCLE_GATE_UNKNOWN_RESOURCE_LZX, "LIFECYCLE-GATE-001"),
            (LIFECYCLE_GATE_UNKNOWN_STATE_LZX, "LIFECYCLE-GATE-002"),
            (
                LIFECYCLE_GATE_MISSING_STATE_COVERAGE_LZX,
                "LIFECYCLE-GATE-003",
            ),
            (LIFECYCLE_GATE_EXTRA_STATE_ARM_LZX, "LIFECYCLE-GATE-004"),
            (LIFECYCLE_GATE_WILDCARD_OVERUSE_LZX, "LIFECYCLE-GATE-005"),
            (LIFECYCLE_GATE_REDIRECT_CYCLE_LZX, "LIFECYCLE-GATE-006"),
            (
                LIFECYCLE_GATE_RESUME_RESOURCE_MISMATCH_LZX,
                "LIFECYCLE-GATE-007",
            ),
            (LIFECYCLE_GATE_WRONG_QUERY_KIND_LZX, "LIFECYCLE-GATE-008"),
            (LIFECYCLE_GATE_WITHOUT_ACTOR_GATE_LZX, "LIFECYCLE-GATE-009"),
        ] {
            let package = lifecycle_gate_fixture(source);
            let diagnostics = package.diagnostics();
            let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
            assert_eq!(
                lifecycle_gate
                    .iter()
                    .map(|d| d.code.as_str())
                    .collect::<Vec<_>>(),
                vec![expected],
                "expected exactly {expected}; got {:?}",
                lifecycle_gate
                    .iter()
                    .map(|d| (&d.code, &d.message))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn lifecycle_gate_cross_feature_resume_resolves_through_uses() {
        let package = lifecycle_gate_fixture(LIFECYCLE_GATE_CROSS_FEATURE_LZX);
        let diagnostics = package.diagnostics();
        let lifecycle_gate = lifecycle_gate_diags(&diagnostics);
        assert!(
            lifecycle_gate.is_empty(),
            "qualified @resume account.account_onboarding must resolve through host.uses account; got {:?}",
            lifecycle_gate
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn auth_refresh_happy_fixture_has_zero_diagnostics() {
        let package = package_from_sources(vec![("auth_refresh.lzi", AUTH_REFRESH_HAPPY)]);
        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();
        assert!(
            diagnostics.is_empty(),
            "happy auth-refresh fixture must emit zero diagnostics; got {:?}",
            diagnostics
                .iter()
                .map(|d| (&d.code, &d.message))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn auth_refresh_fixtures_trigger_exact_codes() {
        for (source, code) in [
            (AUTH_REFRESH_001, "AUTH-REFRESH-001"),
            (AUTH_REFRESH_002, "AUTH-REFRESH-002"),
            (AUTH_REFRESH_003, "AUTH-REFRESH-003"),
            (AUTH_REFRESH_004, "AUTH-REFRESH-004"),
            (AUTH_REFRESH_005, "AUTH-REFRESH-005"),
            (AUTH_REFRESH_006, "AUTH-REFRESH-006"),
            (AUTH_REFRESH_007, "AUTH-REFRESH-007"),
            (AUTH_REFRESH_008, "AUTH-REFRESH-008"),
            (AUTH_REFRESH_009, "AUTH-REFRESH-009"),
        ] {
            assert_auth_refresh_fixture(source, code);
        }
    }

    #[test]
    fn auth_refresh_003_fires_for_incomplete_column_set() {
        let diagnostics = assert_auth_refresh_fixture(AUTH_REFRESH_003, "AUTH-REFRESH-003");
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "AUTH-REFRESH-003")
            .expect("AUTH-REFRESH-003 present");
        assert!(
            diag.message.contains("parent_session_id"),
            "missing-column message should name the incomplete column set: {}",
            diag.message
        );
    }

    #[test]
    fn auth_refresh_007_message_surfaces_resolved_defaults() {
        let diagnostics = assert_auth_refresh_fixture(AUTH_REFRESH_007, "AUTH-REFRESH-007");
        let diag = diagnostics
            .iter()
            .find(|d| d.code == "AUTH-REFRESH-007")
            .expect("AUTH-REFRESH-007 present");
        assert!(diag.message.contains("refresh_ttl 14d"), "{}", diag.message);
        assert!(
            diag.message.contains("rotation_grace 1m"),
            "{}",
            diag.message
        );
        assert!(
            diag.message
                .contains("theft_detection_action revoke_session_family"),
            "{}",
            diag.message
        );
    }

    #[test]
    fn auth_refresh_info_diagnostics_are_non_blocking() {
        for (source, code) in [
            (AUTH_REFRESH_006, "AUTH-REFRESH-006"),
            (AUTH_REFRESH_007, "AUTH-REFRESH-007"),
            (AUTH_REFRESH_008, "AUTH-REFRESH-008"),
            (AUTH_REFRESH_009, "AUTH-REFRESH-009"),
        ] {
            let diagnostics = assert_auth_refresh_fixture(source, code);
            let diag = diagnostics
                .iter()
                .find(|d| d.code == code)
                .expect("diagnostic present");
            assert_eq!(diag.severity, DoctorSeverity::Info, "{code}");
            assert!(
                diagnostics
                    .iter()
                    .all(|d| d.severity != DoctorSeverity::Error),
                "{code} fixture should not contain error-severity diagnostics"
            );
        }
    }
}
