    // Doctor app-manifest operational + registry + auth-failed-redirect + error-page tests
    // Split from `crates/lazuli_cli/src/doctor/tests.rs`.

    use super::test_support_packages::*;
    use super::test_support_core::*;
    use crate::doctor::*;

    #[test]
    fn doctor_reports_app_manifest_operational_gaps() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    customer
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves queries, commands
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  job import
    trigger schedule "0 2 * * *"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
      header "X-Inbound-Signature"
    tenant_from payload.org_id
    idempotency by payload.id
    handler "./webhooks/inbound.go"
"#,
            ),
            (
                "customer.web.lzx",
                r#"
route customer_list
  path "/customers"
  to customer.view.list
  surface customer web
  audience admin
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(codes.contains("APP-ENV-001"));
        assert!(codes.contains("APP-CAP-001"));
        assert!(codes.contains("APP-RUNTIME-001"));
        assert!(codes.contains("APP-RUNTIME-002"));
        assert!(codes.contains("APP-RUNTIME-003"));
        assert!(codes.contains("APP-TARGET-001"));
        assert!(codes.contains("APP-URL-001"));
        assert!(codes.contains("APP-URL-002"));
    }

    #[test]
    fn doctor_accepts_app_manifest_operational_contract() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    customer
  targets
    backend go
    web react
  environments
    production
  urls
    web production "https://app.acme.example"
    api production "https://api.acme.example"
  env
    group webhooks
      server INBOUND_SECRET: Secret required in production
  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.INBOUND_SECRET
  capabilities
    object_storage files
    integration crm
  architecture
    mode modular_monolith
    service_ready true
    enforce_service_boundaries true
  services
    service crm
      owns customer
      exposes
        query customer.query.list
      publishes customer.*
  communication
    internal sync rpc
    external http
    async event_bus
    propagate actor, tenant, trace_id, request_id
  runtime
    unit api
      serves queries, commands, webhooks, apis
      healthcheck "/healthz"
    unit worker
      runs jobs *
    unit scheduler
      runs schedules *
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  api export
    method GET
    path "/api/export"
    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1h)
    policy @scope.public
    handler "./api/export.go"

  job import
    trigger schedule "0 2 * * *"
    handler "./jobs/import.go"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
      header "X-Inbound-Signature"
    tenant_from payload.org_id
    idempotency by payload.id
    handler "./webhooks/inbound.go"
"#,
            ),
            (
                "customer.web.lzx",
                r#"
route customer_list
  path "/customers"
  to customer.view.list
  surface customer web
  audience admin
"#,
            ),
        ]);

        assert!(
            package
                .diagnostics()
                .into_iter()
                .filter(|d| !d.code.starts_with("VOCAB-CONTEXT-")
                    && d.code != "CAP-FILE-POLICY-IMPLICIT"
                    // API-HANDLER-UNWIRED-001 is filtered defensively: as
                    // of the wave-3 bridge a well-formed `api` handler is
                    // wired by codegen so the rule is quiet, but this test
                    // is about the manifest/operational contract, not api
                    // wiring — keep the filter so the assertion stays
                    // robust to api-wiring rule changes either way.
                    && d.code != "API-HANDLER-UNWIRED-001")
                .collect::<Vec<_>>()
                .is_empty()
        );
    }

    #[test]
    fn doctor_uses_registry_for_env_and_capabilities() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  uses
    customer
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  runtime
    unit api
      serves webhooks
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "registry.lzi",
                r#"
registry
  env
    group webhooks
      server INBOUND_SECRET: Secret required in production
  capabilities
    object_storage files
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
  domain
    resource Customer
      csv: @cap.File(max_size:10mb,accept:text/csv) optional

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_SECRET
      header "X-Inbound-Signature"
    tenant_from payload.org_id
    idempotency by payload.id
    handler "./webhooks/inbound.go"
"#,
            ),
        ]);

        let diagnostics: Vec<_> = package
            .diagnostics()
            .into_iter()
            .filter(|d| {
                !d.code.starts_with("VOCAB-CONTEXT-") && d.code != "CAP-FILE-POLICY-IMPLICIT"
            })
            .collect();

        assert!(
            diagnostics.is_empty(),
            "expected registry to satisfy app contract, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn doctor_rejects_unknown_auth_failed_redirect_route() {
        let package = package_from_sources(vec![
            (
                "app.lzi",
                r#"
app AcmeCRM
  lazuli_version "0.16"
  auth_failed_redirect public_login
  not_found public_not_found
  uses
    customer
  targets
    web react
  environments
    production
  urls
    web production "https://app.acme.example"
  runtime
    unit web
      serves surfaces web
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#,
            ),
            (
                "customer.lzi",
                r#"
feature customer
"#,
            ),
            (
                "app.lzx",
                r#"
route public_login
  path "/login"
  to customer.view.login
  surface customer web
  audience public
"#,
            ),
        ]);

        let diagnostics = package.diagnostics();
        let codes: BTreeSet<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();

        assert!(
            !codes.contains("APP-ROUTE-001"),
            "did not expect APP-ROUTE-001 for declared route, got: {diagnostics:#?}",
        );
        assert!(
            codes.contains("APP-ROUTE-002"),
            "expected APP-ROUTE-002 for missing not_found route, got: {diagnostics:#?}",
        );
    }

    #[test]
    fn doctor_rejects_error_page_status_outside_catalog() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  error_page 418
    template "./views/teapot.tmpl"
"#,
        )]);

        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("error-page-contract"),
            "expected error-page-contract, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn doctor_warns_when_error_page_template_is_missing() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  error_page 404
    template "./views/missing-404.tmpl"
"#,
        )]);

        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("error-page-template-missing"),
            "expected error-page-template-missing, got: {diagnostics:#?}"
        );
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code == "error-page-template-missing"
                    && diagnostic.severity == DoctorSeverity::Warning
            }),
            "template-missing should be a warning, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn doctor_rejects_duplicate_error_page_status() {
        let package = package_from_sources(vec![(
            "app.lzi",
            r#"
app AcmeCRM
  error_page 500
    template "./views/500.tmpl"
  error_page 500
    template "./views/other-500.tmpl"
"#,
        )]);

        let diagnostics = package.diagnostics();
        assert!(
            codes(&diagnostics).contains("error-page-duplicate"),
            "expected error-page-duplicate, got: {diagnostics:#?}"
        );
    }

