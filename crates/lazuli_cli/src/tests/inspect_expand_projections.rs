    // Inspect-CLI per-axis projection tests (aggregates, commands, apis,
    // resources, queries, records, tools, tenant_migrations) — split from
    // `crates/lazuli_cli/src/tests.rs`.

    use std::path::Path;

    use crate::{inspect_canonical_source, parse_expand_set};

    // CL.C.4 — `--expand=aggregates` projection test (spec wave-c-cl4).
    #[test]
    fn inspect_expand_aggregates_projects_root_contains_invariants() {
        let expansions = parse_expand_set("aggregates").unwrap();
        assert!(expansions.aggregates);

        let source = "
feature billing
  resource Order
    total: Integer required

  resource OrderLine
    amount: Integer required

  aggregate OrderBoundary
    root Order
    contains OrderLine
    invariants
      invariant total_non_negative
        when total >= 0
        message \"order total cannot be negative\"
";
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"aggregates\":["),
            "expected aggregates projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"OrderBoundary\""),
            "aggregate name should surface: {json}"
        );
        assert!(
            json.contains("\"root\":\"Order\""),
            "root should surface verbatim: {json}"
        );
        assert!(
            json.contains("\"contains\":[\"OrderLine\"]"),
            "contains list should surface: {json}"
        );
        assert!(
            json.contains("\"name\":\"total_non_negative\""),
            "invariant name should surface: {json}"
        );
        assert!(
            json.contains("\"when\":\"total >= 0\""),
            "predicate text should round-trip: {json}"
        );
        assert!(
            json.contains("\"when_kind\":\"closed\""),
            "closed predicate kind should surface: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Phase L Tier 4b/4cd — per-axis inspect projections (synth Wave 1 cell 01).
    // Mirrors the aggregates/caches template. Each test exercises one
    // `--expand=<axis>` flag in isolation and asserts the lifted IR slice
    // surfaces verbatim in the JSON projection.
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_commands_projects_lifted_commands_with_rate_limit_and_audit() {
        let expansions = parse_expand_set("commands").unwrap();
        assert!(expansions.commands);

        let source = r#"
feature billing
  domain
    event_group audit_stream on Order

  resource Order
    total: Integer required

  command pay
    route id: ID
    input
      amount: Integer required
    policy @policy.create
    rate_limit "30 per hour per ip"
    audit actor, target.id, input.amount
      emit_to audit_stream
    creates Order
      total = input.amount
    emits order_paid
    invalidates
      query.list
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"commands\":["),
            "expected commands projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"pay\""),
            "command name should surface: {json}"
        );
        assert!(
            json.contains("\"rate_limit\":{\"default\":\"30 per hour per ip\""),
            "rate_limit verbatim: {json}"
        );
        assert!(
            json.contains("\"audit\""),
            "audit spec should surface: {json}"
        );
        assert!(
            json.contains("\"emit_to\":\"audit_stream\""),
            "audit emit_to should surface: {json}"
        );
        assert!(
            json.contains("\"invalidates\""),
            "invalidates list should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_apis_alias_accepts_api_and_apis_tokens() {
        // Both tokens must populate the same boolean.
        let expansions_plural = parse_expand_set("apis").unwrap();
        assert!(expansions_plural.apis);
        let expansions_singular = parse_expand_set("api").unwrap();
        assert!(expansions_singular.apis);

        let source = r#"
feature billing
  api export
    method GET
    path "/api/billing/export"
    output Text
    policy @scope.public
    handler "./api/export.go"
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions_plural,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"apis\":["),
            "expected apis projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"export\""),
            "api name should surface: {json}"
        );
        assert!(
            json.contains("\"path\":\"/api/billing/export\""),
            "api path should surface: {json}"
        );
        assert!(
            json.contains("\"path\":\"./api/export.go\""),
            "api handler path should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_resources_projects_lifted_resources() {
        let expansions = parse_expand_set("resources").unwrap();
        assert!(expansions.resources);

        let source = r#"
feature billing
  resource Order
    customer_id: ID required
    total: Integer required
    is_high_value: Boolean derived from total > 1000
    retention 7y then anonymize
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"resources\":["),
            "expected resources projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"Order\""),
            "resource name should surface: {json}"
        );
        assert!(
            json.contains("\"retention\""),
            "retention slot should surface: {json}"
        );
        assert!(
            json.contains("\"derived_from\""),
            "derived_from slot should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_queries_projects_lifted_queries() {
        let expansions = parse_expand_set("queries").unwrap();
        assert!(expansions.queries);

        let source = r#"
feature billing
  domain
    query.lookup by_id by id: ID

    query.list list
      params
        status: Text optional
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"queries\":["),
            "expected queries projection in JSON: {json}"
        );
        assert!(
            json.contains("\"by_id\""),
            "lookup query name should surface: {json}"
        );
    }

    #[test]
    fn inspect_expand_records_projects_lifted_records() {
        let expansions = parse_expand_set("records").unwrap();
        assert!(expansions.records);

        let source = r#"
feature billing
  enum InvoiceStatus
    draft
    issued
    paid

  record InvoiceSummary
    status: InvoiceStatus required discriminator
    amount: Integer required
"#;
        let report = inspect_canonical_source(
            source,
            Path::new("features/billing/billing.lzi"),
            expansions,
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(
            json.contains("\"records\":["),
            "expected records projection in JSON: {json}"
        );
        assert!(
            json.contains("\"name\":\"InvoiceSummary\""),
            "record name should surface: {json}"
        );
        assert!(
            json.contains("\"discriminator_field\":\"status\""),
            "discriminator_field should surface: {json}"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A — inspect projections (§7.3 snapshot tests)
    // -------------------------------------------------------------------------

    #[test]
    fn inspect_expand_tools_flag_parses() {
        let expansions = parse_expand_set("tools").unwrap();
        assert!(expansions.tools);
        assert!(!expansions.summary);
    }

    #[test]
    fn inspect_expand_tenant_migrations_alias_projects_ir() {
        let source = r#"
feature customer
  defaults
    tenancy org

  domain
    query.lookup by_id by id: ID

  tenant_migration backfill_lifecycle_stage
    target query.by_id
    axis org
    idempotency envelope.tenant_id
    handler "./migrations/backfill_lifecycle_stage.go"
"#;
        let expansions = parse_expand_set("tenant_migrations").unwrap();
        let report = inspect_canonical_source(source, Path::new("customer.lzi"), expansions);
        let projected = report.features[0]
            .tenant_migrations
            .as_ref()
            .expect("tenant migrations projection");
        assert_eq!(projected[0].name, "backfill_lifecycle_stage");
        assert_eq!(projected[0].target.axis, "org");
        assert!(projected[0].target.operation.is_some());
    }
