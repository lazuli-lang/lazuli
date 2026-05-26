//! Operational manifest parsers for `app.lzi`, `registry.lzi`, `workspace.lzi`,
//! `contracts/*.lzi`, and `profiles.lzi`.
//!
//! Rails parity: each entry point (`parse_app_manifest`, `parse_app_registry`,
//! `parse_app_workspace`, `parse_app_contracts`, `parse_app_profiles`) lives in
//! its own sub-file. Shared low-level line/identifier helpers live in
//! `parsers.rs`. Side-channel doctor-visible defect types live in `types.rs`.
//!
//! All parsers are deliberately line-oriented and lenient: they preserve
//! enough source signal to feed doctor without ever erroring out on a
//! malformed block. Validation is doctor's job; the parser only refuses to
//! emit IR for shapes that cannot be represented at all.
//!
//! See: `lazuli_ir::nodes::app_manifest`,
//!      `lazuli_syntax::ast::feature::PackageSkeleton`.

mod contracts;
mod manifest;
mod parsers;
mod profiles;
mod registry;
mod types;
mod workspace;

#[cfg(test)]
mod contracts_tests;
#[cfg(test)]
mod manifest_encryption_tests;
#[cfg(test)]
mod profiles_tests;
#[cfg(test)]
mod registry_tests;
#[cfg(test)]
mod workspace_tests;

pub use contracts::parse_app_contracts;
pub use manifest::parse_app_manifest;
pub use profiles::parse_app_profiles;
pub use registry::{parse_app_registry, parse_app_registry_with_defects};
pub use types::{RegistryParseOutput, RegistryToolDefectReason, RegistryToolEntryDefect};
pub use workspace::parse_app_workspace;

#[cfg(test)]
mod tests {
    use super::parse_app_manifest;

    #[test]
    fn parses_operational_manifest() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  error_page 404
    template "./views/404.tmpl"
    audience public
  uses
    customer
  packs
    customer_import from registry.packs.customer_import
  bindings
    customer.gateway = integrations.crm
  targets
    backend go
  environments
    production
  urls
    api production "https://api.acme.example"
  env
    server DATABASE_URL: Secret required
    group mailer
      server MAILER_API_KEY: Secret required in production
  integrations
    crm: CRMProvider
      adapter @adapter.crm
      environments production
      credentials platform
        webhook_secret env.CRM_WEBHOOK_SECRET
  capabilities
    database postgres
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
    timeout default "2s"
    retry default 2 backoff exponential
  runtime
    unit api
      serves queries, commands
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#;

        let manifest = parse_app_manifest(source).unwrap();

        assert_eq!(manifest.name, "AcmeCRM");
        assert_eq!(manifest.error_pages.len(), 1);
        assert_eq!(manifest.error_pages[0].status, 404);
        assert_eq!(manifest.error_pages[0].template, "./views/404.tmpl");
        assert_eq!(manifest.error_pages[0].audience.as_deref(), Some("public"));
        assert_eq!(manifest.uses, ["customer"]);
        assert_eq!(manifest.packs[0].name, "customer_import");
        assert_eq!(manifest.packs[0].source, "registry.packs.customer_import");
        assert_eq!(manifest.bindings[0].target_feature, "customer");
        assert_eq!(manifest.bindings[0].target_slot, "gateway");
        assert_eq!(manifest.bindings[0].source, "integrations.crm");
        assert_eq!(manifest.targets, ["backend go"]);
        assert_eq!(manifest.environments, ["production"]);
        assert_eq!(manifest.urls[0].url, "https://api.acme.example");
        assert_eq!(manifest.env[0].name, "DATABASE_URL");
        assert_eq!(manifest.env[1].group.as_deref(), Some("mailer"));
        assert_eq!(manifest.env[1].name, "MAILER_API_KEY");
        assert_eq!(manifest.env[1].environments, ["production"]);
        assert_eq!(manifest.integrations[0].name, "crm");
        assert_eq!(manifest.integrations[0].kind, "CRMProvider");
        assert_eq!(
            manifest.integrations[0].adapter.as_deref(),
            Some("@adapter.crm")
        );
        assert_eq!(
            manifest.integrations[0].adapter_provenance.as_deref(),
            Some("local")
        );
        assert_eq!(
            manifest.integrations[0]
                .credentials
                .as_ref()
                .map(|credentials| credentials.scope.as_str()),
            Some("platform")
        );
        assert_eq!(manifest.capabilities[0].name, "database");
        assert_eq!(
            manifest
                .architecture
                .as_ref()
                .and_then(|architecture| architecture.mode.as_deref()),
            Some("modular_monolith")
        );
        assert_eq!(manifest.services[0].name, "crm");
        assert_eq!(manifest.services[0].owns, ["customer"]);
        assert_eq!(manifest.services[0].exposes[0].kind, "query");
        assert_eq!(
            manifest
                .communication
                .as_ref()
                .and_then(|communication| communication.internal.as_deref()),
            Some("sync rpc")
        );
        assert_eq!(manifest.runtime[0].name, "api");
        assert_eq!(manifest.runtime[0].serves, ["queries", "commands"]);
        assert_eq!(
            manifest
                .deploy
                .as_ref()
                .and_then(|deploy| deploy.rollback.as_deref()),
            Some("on_failed_healthcheck")
        );
    }

    #[test]
    fn parses_app_route_guard_and_actor_query() {
        let source = r#"
app AcmeCRM
  actor_query "account.query.me"
  route_guard
    default_policy @policy.authenticated
    on_unauthenticated redirect "/sign-in"
    on_unauthorized redirect "/403"
    skeleton @client.route_guard_skeleton
"#;

        let manifest = parse_app_manifest(source).unwrap();
        let route_guard = manifest.route_guard.as_ref().expect("route_guard");

        assert_eq!(manifest.actor_query.as_deref(), Some("account.query.me"));
        assert_eq!(
            route_guard.default_policy.as_deref(),
            Some("@policy.authenticated")
        );
        assert_eq!(route_guard.on_unauthenticated.as_deref(), Some("/sign-in"));
        assert_eq!(route_guard.on_unauthorized.as_deref(), Some("/403"));
        assert_eq!(
            route_guard.skeleton.as_deref(),
            Some("@client.route_guard_skeleton")
        );
        assert!(route_guard.span_ref.is_some());
    }

    #[test]
    fn auth_failed_redirect_lowers_to_route_guard_when_absent() {
        let source = r#"
app AcmeCRM
  auth_failed_redirect public_login
"#;

        let manifest = parse_app_manifest(source).unwrap();
        let route_guard = manifest.route_guard.as_ref().expect("route_guard");

        assert_eq!(
            route_guard.on_unauthenticated.as_deref(),
            Some("public_login")
        );
        assert!(route_guard.span_ref.is_some());
    }

    #[test]
    fn parse_app_observability_block() {
        let source = r#"
app crm
  observability
    error_source dev,staging
    panic_recover false
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let observability = manifest.observability.expect("observability block");
        assert_eq!(observability.error_source, ["dev", "staging"]);
        assert!(!observability.panic_recover);
    }

    // -------------------------------------------------------------
    // Roadmap §1.10 — `app.headers` parser tests. Three+ cases per
    // primitive: scalar children parse, `hsts` inline + body forms,
    // closed-catalog values preserved verbatim.
    // -------------------------------------------------------------

    #[test]
    fn parses_app_headers_scalar_children() {
        let source = r#"
app AcmeCRM
  headers
    csp "default-src 'self'; script-src 'self' 'unsafe-inline'"
    x_frame_options DENY
    x_content_type_options nosniff
    referrer_policy strict-origin-when-cross-origin
    permissions_policy "geolocation=(), camera=()"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let headers = manifest.headers.expect("headers block");
        assert_eq!(
            headers.csp.as_deref(),
            Some("default-src 'self'; script-src 'self' 'unsafe-inline'")
        );
        assert_eq!(headers.x_frame_options.as_deref(), Some("DENY"));
        assert_eq!(headers.x_content_type_options.as_deref(), Some("nosniff"));
        assert_eq!(
            headers.referrer_policy.as_deref(),
            Some("strict-origin-when-cross-origin")
        );
        assert_eq!(
            headers.permissions_policy.as_deref(),
            Some("geolocation=(), camera=()")
        );
    }

    #[test]
    fn parses_app_headers_hsts_inline() {
        let source = r#"
app AcmeCRM
  headers
    hsts max_age 31536000 include_subdomains preload
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let hsts = manifest
            .headers
            .expect("headers block")
            .hsts
            .expect("hsts sub-block");
        assert_eq!(hsts.max_age, 31_536_000);
        assert!(hsts.include_subdomains);
        assert!(hsts.preload);
    }

    #[test]
    fn parses_app_headers_hsts_body_form() {
        let source = r#"
app AcmeCRM
  headers
    hsts
      max_age 63072000
      include_subdomains
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let hsts = manifest
            .headers
            .expect("headers block")
            .hsts
            .expect("hsts sub-block");
        assert_eq!(hsts.max_age, 63_072_000);
        assert!(hsts.include_subdomains);
        assert!(!hsts.preload);
    }

    #[test]
    fn parses_app_headers_absent_yields_none() {
        let source = r#"
app AcmeCRM
  title "AcmeCRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.headers.is_none());
    }

    // -------------------------------------------------------------
    // Roadmap §1.2 — `cookie` block parser tests.
    // -------------------------------------------------------------

    #[test]
    fn parses_cookie_block_with_default_profile() {
        let source = r#"
app AcmeCRM
  cookie
    default
      signed true
      secure true
      http_only true
      same_site strict
      max_age "7d"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let cookie = manifest.cookie.expect("cookie block populated");
        assert_eq!(cookie.profiles.len(), 1);
        let default = &cookie.profiles[0];
        assert_eq!(default.name, "default");
        assert_eq!(default.signed, Some(true));
        assert_eq!(default.secure, Some(true));
        assert_eq!(default.http_only, Some(true));
        assert_eq!(default.same_site.as_deref(), Some("strict"));
        assert_eq!(default.max_age.as_deref(), Some("7d"));
    }

    #[test]
    fn parses_cookie_block_with_multiple_profiles() {
        let source = r#"
app AcmeCRM
  cookie
    default
      signed true
      same_site lax
      max_age "24h"
    session
      same_site strict
      max_age "12h"
    csrf
      http_only true
      same_site strict
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let cookie = manifest.cookie.expect("cookie block populated");
        let names: Vec<&str> = cookie.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["default", "session", "csrf"]);
        assert_eq!(cookie.profiles[1].same_site.as_deref(), Some("strict"));
        assert_eq!(cookie.profiles[1].max_age.as_deref(), Some("12h"));
        assert_eq!(cookie.profiles[2].http_only, Some(true));
        // `session` doesn't declare `signed`, so the slot stays None.
        assert_eq!(cookie.profiles[1].signed, None);
    }

    #[test]
    fn cookie_block_absent_yields_none() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.cookie.is_none());
    }

    // -------------------------------------------------------------
    // Roadmap §1.2 — `proxy` block parser tests.
    // -------------------------------------------------------------

    #[test]
    fn parses_proxy_block_with_trusted_cidrs() {
        let source = r#"
app AcmeCRM
  proxy
    trusted 10.0.0.0/8, 172.16.0.0/12
    real_ip_header X-Forwarded-For
    forwarded_proto_header X-Forwarded-Proto
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let proxy = manifest.proxy.expect("proxy block populated");
        assert_eq!(proxy.trusted, vec!["10.0.0.0/8", "172.16.0.0/12"]);
        assert_eq!(proxy.real_ip_header.as_deref(), Some("X-Forwarded-For"));
        assert_eq!(
            proxy.forwarded_proto_header.as_deref(),
            Some("X-Forwarded-Proto")
        );
        assert!(proxy.forwarded_host_header.is_none());
    }

    #[test]
    fn parses_proxy_block_with_all_four_headers() {
        let source = r#"
app AcmeCRM
  proxy
    trusted 192.168.0.0/16
    real_ip_header X-Real-IP
    forwarded_proto_header X-Forwarded-Proto
    forwarded_host_header X-Forwarded-Host
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let proxy = manifest.proxy.expect("proxy block populated");
        assert_eq!(proxy.trusted, vec!["192.168.0.0/16"]);
        assert_eq!(proxy.real_ip_header.as_deref(), Some("X-Real-IP"));
        assert_eq!(
            proxy.forwarded_host_header.as_deref(),
            Some("X-Forwarded-Host")
        );
    }

    #[test]
    fn proxy_block_absent_yields_none() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.proxy.is_none());
    }

    // -------------------------------------------------------------
    // Roadmap §1.2 — `limits` block parser tests.
    // -------------------------------------------------------------

    #[test]
    fn parses_limits_block_with_all_four_slots() {
        let source = r#"
app AcmeCRM
  limits
    body_size "10mb"
    header_size "16kb"
    upload_size "100mb"
    timeout "30s"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let limits = manifest.limits.expect("limits block populated");
        assert_eq!(limits.body_size.as_deref(), Some("10mb"));
        assert_eq!(limits.header_size.as_deref(), Some("16kb"));
        assert_eq!(limits.upload_size.as_deref(), Some("100mb"));
        assert_eq!(limits.timeout.as_deref(), Some("30s"));
    }

    #[test]
    fn parses_limits_block_with_partial_slots() {
        let source = r#"
app AcmeCRM
  limits
    body_size "5mb"
    timeout "10s"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        let limits = manifest.limits.expect("limits block populated");
        assert_eq!(limits.body_size.as_deref(), Some("5mb"));
        assert_eq!(limits.timeout.as_deref(), Some("10s"));
        // Unset slots stay None.
        assert!(limits.header_size.is_none());
        assert!(limits.upload_size.is_none());
    }

    #[test]
    fn limits_block_absent_yields_none() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
"#;
        let manifest = parse_app_manifest(source).unwrap();
        assert!(manifest.limits.is_none());
    }
}
