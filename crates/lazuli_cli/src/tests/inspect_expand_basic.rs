    // Inspect-CLI canonical source expansion tests (basic surface) —
    // split from `crates/lazuli_cli/src/tests.rs`.

    use std::path::Path;

    use crate::{ExpandSet, expand_canonical_source, inspect_canonical_source};

    #[test]
    fn inspect_expand_rewrites_local_sugars() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required
      email: @semantic.Email @pii.contact required
      api_key: @cap.Encrypted(key:@key.tenant) optional

    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money

    query.lookup by_id by id: ID

    query.list list
      params
        name: Text optional

      filters
        name when params.name

      paginate 50

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id

      event created
        email: @semantic.Email

  command create
    input name, email
    policy @policy.create
    creates Customer from input

  command rename
    route id: ID
    input name
    policy @policy.update
    updates Customer
      name = input.name

  workflow lifecycle on Customer.status
    policy @policy.update

    activate: lead -> active requires @policy.delete emits customer_activated
"#;

        let expanded = expand_canonical_source(source);

        assert!(expanded.contains("    query.lookup by_id\n      params\n        id: ID"));
        assert!(expanded.contains("    event customer_created\n      customer_id: ID\n      org_id: ID\n      email: @semantic.Email"));
        assert!(
            expanded.contains(
                "    creates Customer\n      name = input.name\n      email = input.email"
            )
        );
        assert!(
            expanded.contains("    target query.by_id(id: route.id)\n    policy @policy.update")
        );
        assert!(expanded.contains(
            "    activate: lead -> active\n      requires @policy.delete\n      emits customer_activated"
        ));
        assert!(!expanded.contains("event_group customer_* on Customer"));
        assert!(!expanded.contains("from input"));
    }

    #[test]
    fn inspect_json_reports_selected_expansions_with_origin() {
        let source = r#"
feature customer
  purpose "Customers"

  requires integration gateway: PaymentGateway

  refs
    core: @role, @policy, @semantic, @cap, @pii, @key

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required
      email: @semantic.Email @pii.contact required
      api_key: @cap.Encrypted(key:@key.tenant) optional

    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money

    query.lookup by_id by id: ID

    query.list list
      params
        name: Text optional

      filters
        name when params.name

      paginate 50

    event_group customer_* on Customer
      payload
        customer_id = id

      event created
        email: @semantic.Email @pii.contact

  policies
    update: @role.admin

  command rename
    route id: ID
    input name
    policy @policy.update
    idempotency by route.id, input.name
    retry 2 backoff exponential
    calls gateway.rename_customer
      customer_id = route.id
      name = input.name
    timeout "5s"
    updates Customer
      name = input.name
    emits customer_created
"#;
        let mut expansions = ExpandSet::default();
        expansions.events = true;
        expansions.targets = true;
        expansions.policies = true;
        expansions.defaults = true;
        expansions.refs = true;
        expansions.summary = true;
        expansions.locators = true;
        expansions.dependencies = true;
        expansions.security = true;
        expansions.tests = true;

        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"schema\":\"lazuli.inspect.v0\""));
        assert!(json.contains("\"requirements\""));
        assert!(json.contains("\"kind\":\"integration\""));
        assert!(json.contains("\"name\":\"gateway\""));
        assert!(json.contains("\"contract\":\"PaymentGateway\""));
        assert!(json.contains("\"external_calls\""));
        assert!(json.contains("\"subject\":\"customer.command.rename\""));
        assert!(json.contains("\"slot\":\"gateway\""));
        assert!(json.contains("\"operation\":\"rename_customer\""));
        assert!(json.contains("\"timeout\":\"5s\""));
        assert!(json.contains("\"retry\":\"2 backoff exponential\""));
        assert!(json.contains("\"idempotency\":\"route.id, input.name\""));
        assert!(json.contains("\"origin\":\"event_group:customer_*\""));
        assert!(json.contains("\"refs\""));
        assert!(json.contains("\"summary\""));
        assert!(json.contains("\"resources\":[\"Customer\"]"));
        assert!(json.contains("\"records\":[\"CustomerLtv\"]"));
        assert!(json.contains("\"provides\""));
        assert!(json.contains("\"types\":[\"Customer\",\"CustomerLtv\"]"));
        assert!(!json.contains("\"missing\""));
        assert!(
            json.contains("\"origin\":\"inferred from local route id and query.lookup by_id\"")
        );
        assert!(json.contains("\"origin\":\"explicit\""));
        assert!(json.contains("\"origin\":\"defaults\""));
        assert!(json.contains("\"name\":\"query_order\""));
        assert!(json.contains("\"name\":\"query_filter_index\""));
        assert!(json.contains("\"value\":\"org, name\""));
        assert!(json.contains("\"origin\":\"language default\""));
        assert!(json.contains("\"locators\""));
        assert!(json.contains("\"name\":\"route.id\""));
        assert!(json.contains("\"name\":\"target\""));
        assert!(json.contains("\"dependencies\""));
        assert!(json.contains("\"kind\":\"emits_event\""));
        assert!(json.contains("\"security\""));
        assert!(json.contains("\"markers\":[\"@pii.contact\""));
        assert!(json.contains("@cap.Encrypted(key:@key.tenant)"));
        assert!(json.contains("\"tests\""));
        assert!(json.contains("\"assertion\":\"permits @role.admin\""));
        assert!(json.contains("\"origin\":\"generated from command policy @policy.update\""));
    }

    #[test]
    fn inspect_json_reports_app_manifest() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"

  uses
    customer

  packs
    customer_import from registry.packs.customer_import

  bindings
    customer.gateway = integrations.crm

  targets
    backend go
    web react

  environments
    local
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

  services
    service crm
      owns customer
      exposes
        query customer.query.list

  communication
    internal sync rpc
    propagate actor, tenant

  runtime
    unit api
      serves queries, commands
      healthcheck "/healthz"

  deploy
    migrations before_deploy
    rollback on_failed_healthcheck
"#;

        let report = inspect_canonical_source(source, Path::new("app.lzi"), ExpandSet::default());
        let json = serde_json::to_string(&report).unwrap();

        assert!(json.contains("\"app\""));
        assert!(json.contains("\"name\":\"AcmeCRM\""));
        assert!(json.contains("\"packs\""));
        assert!(json.contains("\"registry.packs.customer_import\""));
        assert!(json.contains("\"bindings\""));
        assert!(json.contains("\"target_feature\":\"customer\""));
        assert!(json.contains("\"source\":\"integrations.crm\""));
        assert!(json.contains("\"environments\":[\"local\",\"production\"]"));
        assert!(json.contains("\"url\":\"https://api.acme.example\""));
        assert!(json.contains("\"DATABASE_URL\""));
        assert!(json.contains("\"group\":\"mailer\""));
        assert!(json.contains("\"MAILER_API_KEY\""));
        assert!(json.contains("\"environments\":[\"production\"]"));
        assert!(json.contains("\"integrations\""));
        assert!(json.contains("\"kind\":\"CRMProvider\""));
        assert!(json.contains("\"adapter_provenance\":\"local\""));
        assert!(json.contains("\"webhook_secret\""));
        assert!(json.contains("\"architecture\""));
        assert!(json.contains("\"mode\":\"modular_monolith\""));
        assert!(json.contains("\"services\""));
        assert!(json.contains("\"communication\""));
        assert!(json.contains("\"runtime\""));
        assert!(json.contains("\"migrations\":\"before_deploy\""));
    }

    #[test]
    fn inspect_expand_caches_projects_feature_level_profiles() {
        // CL.C.3 — `--expand=caches` surfaces every feature-level
        // `cache <name>` profile typed end-to-end (key + ttl literal +
        // optional namespace/tags/SWR/coalesce/sliding). The query's
        // inline `cache` slot keeps its own projection.
        let source = r#"
feature catalog
  cache product_view
    key "product:{product_id}"
    ttl 5m
    namespace catalog
    tags product, listing
    stale_while_revalidate 30s
    coalesce true
    sliding true

  domain
    resource Product
      id: ID required

    query.list list
      cache product_view
"#;
        let mut expansions = ExpandSet::default();
        expansions.caches = true;
        let report = inspect_canonical_source(source, Path::new("catalog.lzi"), expansions);
        let json = serde_json::to_string(&report).unwrap();

        // Expand label surfaces in the report header.
        assert!(
            json.contains("\"expand\":[\"caches\"]"),
            "expected expand label, got {json}"
        );
        // Profile shows up in the `caches` projection.
        assert!(
            json.contains("\"caches\":["),
            "expected caches array, got {json}"
        );
        assert!(
            json.contains("\"name\":\"product_view\""),
            "expected profile name, got {json}"
        );
        assert!(
            json.contains("\"namespace\":\"catalog\""),
            "expected namespace, got {json}"
        );
        assert!(json.contains("\"product\""), "expected tags, got {json}");
        assert!(json.contains("\"listing\""), "expected tags, got {json}");
        assert!(
            json.contains("\"coalesce\":true"),
            "expected coalesce, got {json}"
        );
        assert!(
            json.contains("\"sliding\":true"),
            "expected sliding, got {json}"
        );
    }
