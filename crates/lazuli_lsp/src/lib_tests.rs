//! Inline test suite for the LSP crate.
//!
//! Hosted out-of-line via  in
//! . Content is identical to the previous inline
//!  block: every  /
//!  still resolves to the crate root.

    use super::{
        SecurityProfile, diagnostics_for, diagnostics_for_uri, diagnostics_for_with_profile,
        format_canonical_source,
    };
    use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Url};

    /// Per-LSP-test helper: strip `lazuli-doctor` diagnostics so legacy
    /// tests that assert exact LSP-shape diagnostic counts keep passing
    /// after the R2.F doctor wire-up. Doctor wiring has its own tests
    /// further down — see `doctor_*` test cases.
    fn diagnostics_for_lsp_only(source: &str) -> Vec<Diagnostic> {
        diagnostics_for(source)
            .into_iter()
            .filter(|d| d.source.as_deref() != Some("lazuli-doctor"))
            .collect()
    }

    #[test]
    fn canonical_order_accepts_feature_blocks_in_order() {
        let source = r#"
registry
  env
    server INBOUND_WEBHOOK_SECRET: Secret required

feature customer
  purpose "Customers"

  defaults
    tenancy org

  uses org

  domain
    resource Customer

  policies
    create: @role.admin
    update: @role.admin
    read: @scope.same_org

  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Customer

  api export
    method GET
    path "/api/customers/export"
    output @cap.File(max_size:100mb,accept:text/csv)
    policy @policy.read
    handler "./api/export.go"

  workflow lifecycle on Customer.status
    policy @policy.update

  job sync
    trigger schedule "0 2 * * *"
    fanout tenants org

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    tenant_from payload.org_id
    idempotency by payload.id

  surface web admin
    view list Table

  extensions
    hook before_create: Hook[CreateCustomer]

  escape_route "/admin/customer-debug"
    at "./pages/customer_debug.tsx"
    policy @role.admin
    tenant org
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    #[test]
    fn feature_unknown_kind_flags_typo_with_suggestion() {
        let source = r#"
feature typo_test
  domain
    resource Item
      id: ID required

  comand move
    route id: ID
    policy @policy.member
"#;
        let diagnostics = diagnostics_for(source);
        let unknown_kind: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.code.as_ref().and_then(|c| match c {
                    tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.as_str()),
                    _ => None,
                }) == Some("feature-unknown-kind")
            })
            .collect();
        assert_eq!(
            unknown_kind.len(),
            1,
            "expected exactly one feature-unknown-kind diagnostic for `comand`; got {} (full set: {:#?})",
            unknown_kind.len(),
            diagnostics,
        );
        assert!(
            unknown_kind[0].message.contains("comand"),
            "diagnostic must name the offending typo `comand`; got `{}`",
            unknown_kind[0].message,
        );
        assert!(
            unknown_kind[0].message.contains("command"),
            "diagnostic must suggest the closest match `command`; got `{}`",
            unknown_kind[0].message,
        );
    }

    #[test]
    fn feature_unknown_kind_silent_for_decorators_and_field_decls() {
        let source = r#"
feature decorator_test
  domain
    resource Item
      id: ID required

  command create
    @anchor.something
    field_name: Text required
    other_field = ctx.now
    @cap.File(max_size: "10mb")
"#;
        let diagnostics = diagnostics_for(source);
        let unknown_kind: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.code.as_ref().and_then(|c| match c {
                    tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.as_str()),
                    _ => None,
                }) == Some("feature-unknown-kind")
            })
            .collect();
        assert!(
            unknown_kind.is_empty(),
            "decorators / field declarations / assignments / namespaced calls must NOT trip feature-unknown-kind; got {:#?}",
            unknown_kind,
        );
    }

    // ========================================================================
    // 2026-05-15 typo-detection sweep — 7 sibling contexts.
    //
    // Each context gets a positive (typo flagged + suggestion present)
    // and a negative (decorator/assignment/scalar lines stay silent)
    // test. Tests follow the `feature_unknown_kind_*` shape.
    // ========================================================================

    fn diagnostics_with_code<'a>(
        diagnostics: &'a [Diagnostic],
        target: &str,
    ) -> Vec<&'a Diagnostic> {
        diagnostics
            .iter()
            .filter(|d| {
                d.code.as_ref().and_then(|c| match c {
                    tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.as_str()),
                    _ => None,
                }) == Some(target)
            })
            .collect()
    }

    #[test]
    fn app_unknown_kind_flags_typo_with_suggestion() {
        let source = r#"
app demo
  title "Demo"
  urs
    public "https://example.com"
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "app-unknown-kind");
        assert_eq!(
            hits.len(),
            1,
            "expected one app-unknown-kind diagnostic for `urs`; got {} (full set: {:#?})",
            hits.len(),
            diagnostics,
        );
        assert!(hits[0].message.contains("urs"));
        assert!(
            hits[0].message.contains("urls"),
            "diagnostic must suggest `urls`; got `{}`",
            hits[0].message
        );
    }

    #[test]
    fn app_unknown_kind_silent_for_scalars_and_decorators() {
        let source = r#"
app demo
  title "Demo"
  version "1.0"
  lazuli_version "0.15"
  default_locale "en-US"
  default_timezone "UTC"
  urls
    public "https://example.com"
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "app-unknown-kind");
        assert!(
            hits.is_empty(),
            "valid app body lines must not fire app-unknown-kind; got {:#?}",
            hits
        );
    }

    #[test]
    fn registry_unknown_kind_flags_typo_with_suggestion() {
        let source = r#"
registry
  webhook_evnts
    inbound
      payload Webhook
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "registry-unknown-kind");
        assert_eq!(
            hits.len(),
            1,
            "expected one registry-unknown-kind diagnostic for `webhook_evnts`; got {} (full set: {:#?})",
            hits.len(),
            diagnostics,
        );
        assert!(hits[0].message.contains("webhook_evnts"));
        assert!(
            hits[0].message.contains("webhook_events"),
            "diagnostic must suggest `webhook_events`; got `{}`",
            hits[0].message
        );
    }

    #[test]
    fn registry_unknown_kind_silent_for_valid_children() {
        let source = r#"
registry
  env
    server INBOUND_WEBHOOK_SECRET: Secret required
  capabilities
  integrations
  bindings
  packs
  tools
  webhook_events
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "registry-unknown-kind");
        assert!(
            hits.is_empty(),
            "valid registry children must not fire registry-unknown-kind; got {:#?}",
            hits
        );
    }

    #[test]
    fn registry_bindings_sugar_does_not_fire_contract_warnings() {
        // B1 (W3-blockers) — the `bindings` registry sugar accepts the
        // simplified child grammar (endpoint + auth keys) at indent-6,
        // identical to the indent-6 children of `integrations`. No
        // `registry-unknown-kind`, no `registry-contract`, no
        // `app-integration-contract` warning should fire.
        let source = r#"
registry
  bindings
    object_store: ObjectStore
      adapter @lazuli/plugin-object-store
      endpoint env.S3_ENDPOINT
      auth keys env.S3_ACCESS_KEY_ID env.S3_SECRET_ACCESS_KEY
"#;
        let diagnostics = diagnostics_for(source);
        for code in [
            "registry-unknown-kind",
            "registry-contract",
            "app-integration-contract",
        ] {
            let hits = diagnostics_with_code(&diagnostics, code);
            assert!(
                hits.is_empty(),
                "valid `registry bindings` sugar must not fire `{code}`; got {hits:#?}",
            );
        }
    }

    #[test]
    fn view_unknown_kind_flags_typo_with_suggestion() {
        // L0 #6 view body: `selecton` is a typo of `selection`.
        let source = r#"
feature catalog
  surface web admin
    audience admin
      view list ItemList
        source query.list
        columns name, status
        selecton
          mode multi
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "view-unknown-kind");
        assert!(
            hits.iter().any(|h| h.message.contains("selecton")),
            "expected view-unknown-kind to flag `selecton`; got {:#?}",
            hits
        );
        assert!(
            hits.iter().any(|h| h.message.contains("selection")),
            "diagnostic must suggest `selection`; got {:#?}",
            hits
        );
    }

    #[test]
    fn view_unknown_kind_silent_for_valid_body() {
        let source = r#"
feature catalog
  surface web admin
    audience admin
      view list ItemList
        source query.list
        columns name, status
        search params.q over name
        sort
          by name asc
        selection
          mode multi
        bulk_actions delete
        actions create, update
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "view-unknown-kind");
        assert!(
            hits.is_empty(),
            "valid view body must not fire view-unknown-kind; got {:#?}",
            hits
        );
    }

    #[test]
    fn surface_unknown_kind_flags_typo_with_suggestion() {
        let source = r#"
feature catalog
  surface web admin
    audeince admin
      view list ItemList
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "surface-unknown-kind");
        assert_eq!(
            hits.len(),
            1,
            "expected one surface-unknown-kind diagnostic for `audeince`; got {} (full set: {:#?})",
            hits.len(),
            diagnostics,
        );
        assert!(hits[0].message.contains("audeince"));
        assert!(
            hits[0].message.contains("audience"),
            "diagnostic must suggest `audience`; got `{}`",
            hits[0].message
        );
    }

    #[test]
    fn surface_unknown_kind_silent_for_valid_children() {
        let source = r#"
feature catalog
  surface web admin
    uses experience CatalogExperience
    audience admin
      view list ItemList
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "surface-unknown-kind");
        assert!(
            hits.is_empty(),
            "valid surface body must not fire surface-unknown-kind; got {:#?}",
            hits
        );
    }

    #[test]
    fn command_statement_unknown_flags_typo_with_suggestion() {
        let source = r#"
feature billing
  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    audt actor, target.id
    creates Invoice
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "command-statement-unknown");
        assert_eq!(
            hits.len(),
            1,
            "expected one command-statement-unknown diagnostic for `audt`; got {} (full set: {:#?})",
            hits.len(),
            diagnostics,
        );
        assert!(hits[0].message.contains("audt"));
        assert!(
            hits[0].message.contains("audit"),
            "diagnostic must suggest `audit`; got `{}`",
            hits[0].message
        );
    }

    #[test]
    fn command_statement_unknown_silent_for_assignments_and_targets() {
        // Capitalized identifiers (effect targets) and assignments
        // (`let x = ...` / `field = expr`) and field-decl colon lines
        // must NOT fire the lint.
        let source = r#"
feature billing
  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    audit actor, target.id
    let computed = @fn.score(input)
    other_field = ctx.now
    Customer
    creates Invoice
    emits invoice_created from creates
    invalidates query.list
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "command-statement-unknown");
        assert!(
            hits.is_empty(),
            "assignments / capitalized targets / valid statements must not fire command-statement-unknown; got {:#?}",
            hits
        );
    }

    #[test]
    fn query_statement_unknown_flags_typo_with_suggestion() {
        let source = r#"
feature catalog
  query.list items
    policy @policy.read
    paginat 20
    order name asc
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "query-statement-unknown");
        assert_eq!(
            hits.len(),
            1,
            "expected one query-statement-unknown diagnostic for `paginat`; got {} (full set: {:#?})",
            hits.len(),
            diagnostics,
        );
        assert!(hits[0].message.contains("paginat"));
        assert!(
            hits[0].message.contains("paginate"),
            "diagnostic must suggest `paginate`; got `{}`",
            hits[0].message
        );
    }

    #[test]
    fn query_statement_unknown_silent_for_valid_body() {
        let source = r#"
feature catalog
  query.list items
    policy @policy.read
    params
      tenant_id: ID required
    filters
      status = "active"
    paginate 20
    order name asc
    cache items_cache
  query.lookup item by id: ID
    policy @policy.read
  query.sql item_count
    policy @policy.read
    returns Integer
    sql "./queries/count.sql"
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "query-statement-unknown");
        assert!(
            hits.is_empty(),
            "valid query body must not fire query-statement-unknown; got {:#?}",
            hits
        );
    }

    #[test]
    fn audience_unknown_kind_flags_typo_with_suggestion() {
        let source = r#"
feature catalog
  surface web admin
    audience admin
      vieww list ItemList
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "audience-unknown-kind");
        assert_eq!(
            hits.len(),
            1,
            "expected one audience-unknown-kind diagnostic for `vieww`; got {} (full set: {:#?})",
            hits.len(),
            diagnostics,
        );
        assert!(hits[0].message.contains("vieww"));
        assert!(
            hits[0].message.contains("view"),
            "diagnostic must suggest `view`; got `{}`",
            hits[0].message
        );
    }

    #[test]
    fn audience_unknown_kind_silent_for_valid_children() {
        let source = r#"
feature catalog
  surface web admin
    audience admin
      requires @scope.same_org
      view list ItemList
      view detail ItemDetail
"#;
        let diagnostics = diagnostics_for(source);
        let hits = diagnostics_with_code(&diagnostics, "audience-unknown-kind");
        assert!(
            hits.is_empty(),
            "valid audience children must not fire audience-unknown-kind; got {:#?}",
            hits
        );
    }

    #[test]
    fn canonical_order_accepts_full_capsule_fixture() {
        let diagnostics = diagnostics_for(include_str!(
            "../../../examples/full-capsule/full-capsule.lzi"
        ));

        // The full-capsule feature file references env vars declared in the
        // sibling `registry.lzi`. The per-file LSP can't see registry, so it
        // emits an informational `env-schema-reference` warning that doctor
        // resolves cross-package. Filter it out for ordering tests.
        //
        // Also filter `lazuli-doctor` source: the doctor catalog is wired
        // separately (R2.F) and has its own round-trip tests.
        let filtered: Vec<_> = diagnostics
            .iter()
            .filter(|d| {
                d.source.as_deref() != Some("lazuli-doctor")
                    && d.code.as_ref().and_then(|c| match c {
                        tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.as_str()),
                        _ => None,
                    }) != Some("env-schema-reference")
            })
            .cloned()
            .collect();

        assert!(
            filtered.is_empty(),
            "expected no canonical ordering diagnostics, got: {filtered:#?}"
        );
    }

    #[test]
    fn canonical_accepts_feature_integration_requirements() {
        let source = r#"
feature payments
  purpose "Payments"

  requires integration gateway: PaymentGateway

feature credit_check
  purpose "Credit checks"

  requires
    integration bureau: CreditBureau
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_feature_requirements() {
        let source = r#"
feature payments
  purpose "Payments"

  requires
    provider mercadopago: PaymentGateway
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
                "feature requirements currently use `integration <name>: <CapabilityType>`",
            )
        }));
    }

    #[test]
    fn canonical_accepts_external_calls_through_required_integration_slot() {
        let source = r#"
feature imports
  requires integration crm: CRMProvider

  job process_import
    trigger event import_uploaded
    idempotency by payload.batch_id
    retry 3 backoff exponential
    calls crm.normalize_import_batch
      batch_id = payload.batch_id
    timeout "30s"
    handler "./jobs/process_import.go"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected no external call diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_external_calls_without_contract_guards() {
        let source = r#"
feature imports
  job process_import
    trigger event import_uploaded
    calls crm.normalize_import_batch
      payload.batch_id
    handler "./jobs/process_import.go"
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("requires integration"))
        );
        assert!(messages.iter().any(|message| message.contains("timeout")));
        assert!(messages.iter().any(|message| message.contains("retry")));
        assert!(
            messages
                .iter()
                .any(|message| message.contains("idempotency"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("argument bindings"))
        );
    }

    #[test]
    fn canonical_warns_for_invalid_app_bindings() {
        let source = r#"
app AcmeCRM
  uses
    payments

  bindings
    payments.gateway -> mercadopago
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("app bindings use `<feature>.<slot> = integrations.<name>`")
        }));
    }

    #[test]
    fn canonical_accepts_app_profiles() {
        let source = r#"
profile local
  urls
    web "http://localhost:3000"
    api "http://localhost:8080"
  bindings
    customer_import.crm = integrations.crm
  integrations
    crm environment sandbox
    crm adapter @adapter.fake_crm
  deploy
    topology monolith
    migrations before_deploy
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected profile contract to pass LSP diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_app_profiles() {
        let source = r#"
profile 123
  urls
    web http://localhost:3000
  bindings
    customer_import.crm -> integrations.crm
  integrations
    crm sandbox
  deploy
    topology "split"
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("profile headers"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("URL overrides"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("profile bindings"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("integration overrides"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("profile deploy"))
        );
    }

    #[test]
    fn canonical_accepts_app_and_registry_pack_contracts() {
        let source = r#"
app AcmeCRM
  uses
    payments
  packs
    payments from registry.packs.payments
  targets
    backend go
  environments
    production
  runtime
    unit api
      serves commands
      healthcheck "/healthz"
  deploy
    migrations before_deploy
    rollback on_failed_healthcheck

registry
  integrations
    mercadopago: PaymentGateway
      adapter @runtime/mercadopago
    serasa: CreditBureau
      adapter @lazuli/plugin-acme/serasa
  packs
    payments from @runtime/payments
      version "0.1.0"
      provides feature payments
      requires integration gateway: PaymentGateway
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected app/registry pack contracts to pass LSP diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_pack_contracts() {
        let source = r#"
app AcmeCRM
  packs
    payments -> registry.packs.payments

registry
  packs
    payments @runtime/payments
      provides
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("app pack entries use `<alias> from registry.packs.<name>`")
        }));
        assert!(
            messages
                .iter()
                .any(|message| message.contains("registry packs use `<name> from"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("pack children use"))
        );
    }

    #[test]
    fn canonical_warns_for_unknown_adapter_provenance() {
        let source = r#"
registry
  integrations
    crm: CRMProvider
      adapter @unknown.crm

profile local
  integrations
    crm adapter @unknown.fake
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("adapter @runtime/") || message.contains("adapter <source>")
        }));
    }

    #[test]
    fn canonical_accepts_workspace_contract() {
        let source = r#"
workspace AcmeERP
  apps
    crm at "./apps/crm/app.lzi"
    ai external contract "acme.ai.v1"
  shared_registry "./registry.lzi"
  boundaries
    crm publishes customer.*
    ai consumes customer.*
  communication
    propagate actor, tenant, trace_id, request_id
    default sync internal rpc
    default async event_bus
  gateway public_api
    route "/api/customers/*" to app crm
      auth propagate
      tenant propagate
      timeout "5s"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected workspace contract to pass LSP diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_workspace_contract() {
        let source = r#"
workspace 123
  apps
    crm "./apps/crm/app.lzi"
  shared_registry ./registry.lzi
  boundaries
    crm listens customer.*
  communication
    default sync grpc
  gateway 123
    route "/api/customers/*" to app crm
      auth inherit
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace contracts use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace apps use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("shared registries use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace boundaries use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace communication uses"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("workspace gateways use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("gateway route children use"))
        );
    }

    #[test]
    fn canonical_accepts_external_contract() {
        let source = r#"
contract acme.ai.v1
  purpose "AI inference service."
  compatibility backward
  import openapi "./contracts/ai.openapi.json"

  record CustomerSummaryRequest
    customer_id: ID required
    email: @semantic.Email @pii.contact optional

  record CustomerSummaryResult
    summary: Text required
    generated_at: DateTime required

  operation summarize_customer
    transport http
    method POST
    path "/v1/customer-summary"
    input CustomerSummaryRequest
    output CustomerSummaryResult
    auth service
    timeout "10s"

  event summary_ready
    topic "ai.summary_ready"
    payload
      customer_id: ID required
      summary: Text required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics.is_empty(),
            "expected external contract to pass LSP diagnostics, got: {diagnostics:#?}"
        );
    }

    #[test]
    fn canonical_warns_for_invalid_external_contract() {
        let source = r#"
contract 123
  compatibility future
  import swagger ./ai.yaml
  record request
    customer_id ID required
  operation summarize
    transport grpc
    method FETCH
    path /v1/summary
  event summary_ready
    topic ai.summary_ready
    payload
      customer_id: ID
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("external contracts use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract compatibility"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract imports use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract records use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract fields use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("operation children use"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("contract event topics"))
        );
    }

    #[test]
    fn canonical_examples_satisfy_lsp_contracts() {
        let examples = [
            (
                "full-capsule-app.lzi",
                include_str!("../../../examples/full-capsule/app.lzi"),
            ),
            (
                "full-capsule-registry.lzi",
                include_str!("../../../examples/full-capsule/registry.lzi"),
            ),
            (
                "full-capsule-profiles.lzi",
                include_str!("../../../examples/full-capsule/profiles.lzi"),
            ),
            (
                "full-capsule-workspace.lzi",
                include_str!("../../../examples/full-capsule/workspace.lzi"),
            ),
            (
                "full-capsule-contract-ai.lzi",
                include_str!("../../../examples/full-capsule/contracts/ai.lzi"),
            ),
            (
                "audit-log.lzi",
                include_str!("../../../examples/audit-log.lzi"),
            ),
            ("billing.lzi", include_str!("../../../examples/billing.lzi")),
            ("comment.lzi", include_str!("../../../examples/comment.lzi")),
            (
                "customer-capsule.lzi",
                include_str!("../../../examples/customer-capsule.lzi"),
            ),
            (
                "extension-points.lzi",
                include_str!("../../../examples/extension-points.lzi"),
            ),
            (
                "field-permissions.lzi",
                include_str!("../../../examples/field-permissions.lzi"),
            ),
            (
                "full-capsule.lzi",
                include_str!("../../../examples/full-capsule/full-capsule.lzi"),
            ),
            (
                "import-csv.lzi",
                include_str!("../../../examples/import-csv.lzi"),
            ),
            (
                "linear-issue.lzi",
                include_str!("../../../examples/linear-issue.lzi"),
            ),
            (
                "notification.lzi",
                include_str!("../../../examples/notification.lzi"),
            ),
            (
                "org-team.lzi",
                include_str!("../../../examples/org-team.lzi"),
            ),
            (
                "user-auth.lzi",
                include_str!("../../../examples/user-auth.lzi"),
            ),
        ];

        for (name, source) in examples {
            let diagnostics = diagnostics_for(source);
            // `env-schema-reference` is a per-file warning that doctor
            // cross-checks across the package registry. The LSP can't see
            // sibling files, so feature sources that reference env vars
            // declared in `registry.lzi` legitimately surface this warning.
            // Filter it out for the per-file canonical contract.
            //
            // Also filter out `lazuli-doctor` sourced diagnostics: this
            // contract tests the per-file LSP shape lints, not the doctor
            // vocab/lifecycle/correctness catalog which has its own
            // round-trip tests further down (`doctor_*`).
            let filtered: Vec<_> = diagnostics
                .iter()
                .filter(|d| {
                    d.source.as_deref() != Some("lazuli-doctor")
                        && d.code.as_ref().and_then(|c| match c {
                            tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.as_str()),
                            _ => None,
                        }) != Some("env-schema-reference")
                })
                .cloned()
                .collect();
            assert!(
                filtered.is_empty(),
                "expected {name} to satisfy canonical LSP diagnostics, got: {filtered:#?}"
            );
        }
    }

    #[test]
    fn canonical_accepts_app_operational_manifest() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  version "0.1.0"

  uses
    customer

  bindings
    customer.gateway = integrations.crm

  targets
    backend go
    web react

  environments
    local
    production

  urls
    web local "http://localhost:3000"
    api production "https://api.acme.example"

  env
    server DATABASE_URL: Secret required
    group webhooks
      server CRM_WEBHOOK_SECRET: Secret required in production
    group public
      client PUBLIC_API_URL: Url required
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
    queue background_jobs
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
        command customer.command.create
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
      serves queries, commands, webhooks, apis
      healthcheck "/healthz"

    unit worker
      runs jobs *

  deploy
    migrations before_deploy
    migration_lock required
    destructive_migrations require_approval
    rollback on_failed_healthcheck
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_warns_for_incomplete_app_operational_manifest() {
        let source = r#"
app AcmeCRM
  targets
    backend go

  runtime
    unit api
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("app manifests should declare `uses`") })
        );
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("app manifests should declare `deploy`") })
        );
        assert!(messages.iter().any(|message| {
            message.contains("runtime units should declare what they `serves` or `runs`")
        }));
    }

    #[test]
    fn lzx_examples_satisfy_lsp_contracts() {
        let examples = [
            (
                "customer-capsule.lzx",
                include_str!("../../../examples/customer-capsule.lzx"),
            ),
            (
                "customer-capsule.web.lzx",
                include_str!("../../../examples/customer-capsule.web.lzx"),
            ),
            (
                "full-capsule.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.lzx"),
            ),
            (
                "full-capsule.admin.web.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.admin.web.lzx"),
            ),
            (
                "full-capsule.public.web.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.public.web.lzx"),
            ),
            (
                "full-capsule.account.web.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.account.web.lzx"),
            ),
            (
                "full-capsule.sales.mobile.lzx",
                include_str!("../../../examples/full-capsule/full-capsule.sales.mobile.lzx"),
            ),
        ];

        for (name, source) in examples {
            let diagnostics = diagnostics_for(source);
            assert!(
                diagnostics.is_empty(),
                "expected {name} to satisfy LZX diagnostics, got: {diagnostics:#?}"
            );
        }
    }

    #[test]
    fn canonical_events_payload_warns_for_unknown_resource_field() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id
        team_id = team.id

    event customer_created
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("resource `Customer` has no field named `team`")
        );
    }

    #[test]
    fn canonical_command_warns_for_missing_policy() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

  command create
    rate_limit "30 per hour per user"
    creates Customer
"#;

        let diagnostics = diagnostics_for_lsp_only(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("command `create` should declare `policy` explicitly")
        );
    }

    #[test]
    fn canonical_refs_warn_when_manifest_drifts() {
        let source = r#"
feature customer
  purpose "Customers"

  refs
    core: @role

  policies
    create: @role.admin

  command create
    policy @policy.create
    creates Customer
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("refs for feature `customer` is missing used namespaces: @policy")
        }));
    }

    #[test]
    fn canonical_warns_for_unknown_local_policy_reference() {
        let source = r#"
feature customer
  purpose "Customers"

  policies
    create: @role.admin

  command create
    policy @policy.update
    creates Customer
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
                "`@policy.*` references should resolve to a feature-local policy category",
            )
        }));
    }

    #[test]
    fn canonical_warns_for_direct_policy_atom_in_command() {
        let source = r#"
feature customer
  purpose "Customers"

  command create
    policy @role.admin
    creates Customer
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("commands and workflows should reference feature-local policy categories")
        }));
    }

    #[test]
    fn canonical_warns_for_scope_override_without_query_policy() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    query.list global_search
      scope override
        deleted_at = nil
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`scope override` replaces inherited tenant/soft-delete safety scope")
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`scope override` should include a `reason")
        }));
    }

    #[test]
    fn canonical_warns_for_event_job_without_tenant_from() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    event customer_activated
      customer_id: ID
      org_id: ID

feature outreach
  purpose "Outreach"

  uses customer

  job send_welcome
    trigger event customer.customer_activated
    idempotency by envelope.id
    handler "./jobs/send_welcome.go"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should declare `tenant_from payload.org_id`")
        }));
    }

    #[test]
    fn canonical_warns_for_public_command_without_rate_limit() {
        let source = r#"
feature user
  purpose "Users"

  policies
    login: @scope.public

  command login
    input
      email: @semantic.Email
      password: Text
    policy @policy.login
    returns AuthSession
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("commands that are public or mutate state must declare")
        }));
    }

    #[test]
    fn strict_profile_promotes_security_omissions_to_errors() {
        let source = r#"
feature customer
  purpose "Customers"

  command create
    creates Customer
"#;

        let prototype = diagnostics_for_with_profile(source, SecurityProfile::Prototype);
        let strict = diagnostics_for_with_profile(source, SecurityProfile::Strict);

        assert!(prototype.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::WARNING)
                && diagnostic
                    .message
                    .contains("should declare `policy` explicitly")
        }));
        assert!(strict.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("should declare `policy` explicitly")
        }));
    }

    #[test]
    fn canonical_requires_field_policies_for_sensitive_fields() {
        let source = r#"
feature auth
  purpose "Auth"

  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:argon2id) required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("must declare field-level `read` and `write` policies")
        }));
    }

    #[test]
    fn canonical_requires_webhook_verify_and_idempotency() {
        let source = r#"
feature billing
  purpose "Billing"

  webhook stripe_invoice_paid
    path "/webhooks/stripe/invoice-paid"
    handler "./integrations/stripe.go"
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("webhooks are inbound trust boundaries and must declare `verify")
        }));
        assert!(
            messages.iter().any(|message| {
                message.contains("webhooks must declare `idempotency by payload.")
            })
        );
        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
        );
    }

    #[test]
    fn canonical_warns_for_tenant_webhook_without_tenant_from() {
        let source = r#"
feature billing
  purpose "Billing"

  defaults
    tenancy org

  webhook stripe_invoice_paid
    path "/webhooks/stripe/invoice-paid"
    verify hmac sha256
      secret env.STRIPE_SECRET
      header "Stripe-Signature"
    idempotency by payload.org_id, payload.provider_event_id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should declare `tenant_from payload.org_id`")
        }));
    }

    #[test]
    fn strict_profile_rejects_security_opt_out_without_reason() {
        let source = r#"
feature billing
  purpose "Billing"

  webhook inbound
    path "/webhooks/inbound"
    verify none
    idempotency by payload.id
"#;

        let strict = diagnostics_for_with_profile(source, SecurityProfile::Strict);

        assert!(strict.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("`verify none` must include a `reason")
        }));
    }

    #[test]
    fn production_profile_rejects_reasoned_security_opt_out() {
        let source = r#"
feature billing
  purpose "Billing"

  webhook inbound
    path "/webhooks/inbound"
    verify none
      reason "Internal tunnel in development only."
    idempotency by payload.id
"#;

        let strict = diagnostics_for_with_profile(source, SecurityProfile::Strict);
        let production = diagnostics_for_with_profile(source, SecurityProfile::Production);

        assert!(strict.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::WARNING)
                && diagnostic
                    .message
                    .contains("`verify none` is an explicit security opt-out")
        }));
        assert!(production.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("`verify none` is an explicit security opt-out")
        }));
    }

    #[test]
    fn canonical_requires_escape_route_security_envelope() {
        let source = r#"
feature customer
  purpose "Customers"

  escape_route "/admin/raw"
    at "./pages/raw.tsx"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Some(DiagnosticSeverity::ERROR)
                && diagnostic
                    .message
                    .contains("`escape_route` is outside generated UI ownership")
        }));
    }

    #[test]
    fn canonical_requires_auth_password_and_session_contracts() {
        let source = r#"
feature auth
  purpose "Auth"

  auth
    password
      hash @fn.hash_password

    sessions
      resource Session
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| message.contains("`auth password` must declare `algorithm"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("credential guessing protection"))
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("`auth sessions` must declare `ttl`"))
        );
    }

    #[test]
    fn canonical_warns_for_incomplete_crypto_contracts() {
        let source = r#"
feature auth
  purpose "Auth"

  domain
    resource Session
      legacy_secret: @cap.Secret required
      refresh_token_hash: @cap.Hashed required
      api_key: @cap.Encrypted required
      reset_token: @cap.Token(ttl:1h,single_use:true) required
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("`@cap.Secret` is legacy") })
        );
        assert!(messages.iter().any(|message| {
            message.contains("`@cap.Hashed` should declare `algorithm:<name>`")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("`@cap.Encrypted` should declare `key:@key.<scope>`")
        }));
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("`@cap.Token` should declare `store:hashed`") })
        );
    }

    #[test]
    fn canonical_warns_for_invalid_crypto_capability_arguments() {
        let source = r#"
feature auth
  purpose "Auth"

  domain
    resource Session
      refresh_token_hash: @cap.Hashed(algorithm:md5,pepper:true) required
      api_key: @cap.Encrypted(key:tenant) required
      private_note: @cap.E2ee(key:tenant) optional
      reset_token: @cap.Token(ttl:"one hour",single_use:yes,store:plain) required
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("canonical v0 hash algorithms are `argon2id` or `bcrypt`")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("@cap.Hashed only accepts canonical arguments: algorithm")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("encryption capability keys should use `key:@key.<scope>`")
        }));
        assert!(
            messages.iter().any(|message| {
                message.contains("`@cap.Token` ttl should use `ttl:<duration>`")
            })
        );
        assert!(messages.iter().any(|message| {
            message.contains("`@cap.Token` single_use should be `true` or `false`")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("`@cap.Token` store should be `hashed` in canonical v0")
        }));
    }

    #[test]
    fn canonical_warns_for_authored_summary() {
        let source = r#"
feature customer
  purpose "Customers"

  summary
    resources: Customer
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`summary` is generated by `lazuli inspect --expand=summary`")
        }));
    }

    #[test]
    fn canonical_warns_for_env_reference_without_schema() {
        let source = r#"
feature integration
  purpose "Integration"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    idempotency by payload.id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("environment reference `env.INBOUND_WEBHOOK_SECRET`")
        }));
    }

    #[test]
    fn canonical_accepts_declared_env_reference() {
        let source = r#"
env
  group webhooks
    server INBOUND_WEBHOOK_SECRET: Secret required in production
  group public_clients
    client PUBLIC_APP_URL: Url required
    mobile EXPO_PUBLIC_API_URL: Url required

feature integration
  purpose "Integration"

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    idempotency by payload.id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(!diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("environment reference `env.INBOUND_WEBHOOK_SECRET`")
        }));
    }

    #[test]
    fn canonical_warns_for_incomplete_api_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  api stream_summary
    method POST
    path "/api/customers/:id/summary"
    output stream Text
    policy @policy.read
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| message.contains("handler")));
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("api path parameter `id` should be declared") })
        );
    }

    #[test]
    fn canonical_warns_for_incomplete_cache_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

    query.list list
      cache
        key customer.list(params)
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("query cache contracts should declare ttl")
        }));
    }

    #[test]
    fn canonical_warns_for_invalid_error_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  errors
    default leak
    expose client 4xx stack

  command archive
    policy @policy.update
    rate_limit "30 per minute per user"
    error CustomerGone status 900 expose stack
    deletes Customer
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("feature error defaults use `default hide`") })
        );
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("error exposure uses `expose client") })
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("HTTP status code from 100 to 599"))
        );
    }

    #[test]
    fn lzx_accepts_experience_and_platform_surface_layers() {
        let experience = r#"
experience customer
  imports customer

  view list
    source customer.query.list
    action create -> customer.command.create
"#;

        let surface = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
      columns name, email, tier
"#;

        assert!(diagnostics_for(experience).is_empty());
        assert!(diagnostics_for(surface).is_empty());
    }

    #[test]
    fn lzx_warns_for_untyped_top_level_route_params() {
        let source = r#"
route customer_detail
  path "/customers/:id"
  to customer.view.detail(id: route.id)
  surface customer web
  audience admin
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("route path parameter `id` should be declared")
        }));
    }

    #[test]
    fn lzx_accepts_typed_top_level_routes() {
        let source = r#"
app AcmeCRM
  title "Acme CRM"
  targets
    backend go
    web react
  uses customer

route customer_detail
  path "/customers/:id"
  route id: Customer.ID
  to customer.view.detail(id: route.id)
  surface customer web
  audience admin
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn derived_field_accepts_expression() {
        let source = r#"
feature customer
  domain
    resource Customer
      score: Integer = 0
      is_high_value: Boolean derived from score > 80
"#;
        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn derived_field_rejects_default_or_requiredness() {
        let bad_default = r#"
feature customer
  domain
    resource Customer
      tier: Text derived from compute_tier(score) = "bronze"
"#;
        let diagnostics = diagnostics_for(bad_default);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("must not declare `default`"))
        );

        let bad_required = r#"
feature customer
  domain
    resource Customer
      tier: Text required derived from compute_tier(score)
"#;
        let diagnostics = diagnostics_for(bad_required);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("must not declare `required`"))
        );
    }

    #[test]
    fn derived_field_requires_expression() {
        let source = r#"
feature customer
  domain
    resource Customer
      tier: Text derived from
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("requires an expression"))
        );
    }

    #[test]
    fn has_many_accepts_canonical_collections() {
        let source = r#"
feature customer
  domain
    resource Customer
      has_many notes: CustomerNote inverse customer
      has_many tags: CustomerTag
"#;
        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn has_many_rejects_unexpected_tail_or_missing_inverse_field() {
        let bad_tail = r#"
feature customer
  domain
    resource Customer
      has_many notes: CustomerNote required
"#;
        assert!(
            diagnostics_for(bad_tail)
                .iter()
                .any(|d| d.message.contains("Only `inverse <field>` is allowed"))
        );

        let bad_inverse = r#"
feature customer
  domain
    resource Customer
      has_many notes: CustomerNote inverse
"#;
        assert!(
            diagnostics_for(bad_inverse)
                .iter()
                .any(|d| d.message.contains("`inverse` requires a field name"))
        );
    }

    #[test]
    fn agent_accepts_canonical_declaration() {
        let source = r#"
feature customer
  policies
    read: @scope.same_org

  agent summarize_customer
    input
      customer_id: ID required
    context customer.query.by_id(id: input.customer_id)
    policy @policy.read
    rate_limit "20 per hour per user"
    output stream Text
    model @llm.default
    prompt "./prompts/summarize_customer.md"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics, got: {:#?}",
            diagnostics
                .iter()
                .map(|d| (d.message.clone(), d.code.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn agent_rejects_missing_required_children() {
        let source = r#"
feature customer
  agent summarize_customer
    input
      prompt: Text required
"#;
        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`policy @policy.<name>`"))
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`output [stream] <Type>`"))
        );
        assert!(messages.iter().any(|m| m.contains("`model @llm.<name>`")));
        assert!(messages.iter().any(|m| m.contains("prompt")));
    }

    #[test]
    fn agent_rejects_non_llm_model_reference() {
        let source = r#"
feature customer
  agent summarize_customer
    input
      prompt: Text required
    policy @policy.read
    output stream Text
    model gpt-4
    prompt "./prompts/summarize_customer.md"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("must be a `@llm.<name>` reference"))
        );
    }

    // -------------------------------------------------------------------------
    // Cut A — file-local diagnostics (§6.2 snapshot tests)
    // -------------------------------------------------------------------------

    fn diagnostic_codes(diagnostics: &[Diagnostic]) -> Vec<String> {
        diagnostics
            .iter()
            .filter_map(|d| match d.code.as_ref()? {
                tower_lsp::lsp_types::NumberOrString::String(s) => Some(s.clone()),
                tower_lsp::lsp_types::NumberOrString::Number(n) => Some(n.to_string()),
            })
            .collect()
    }

    #[test]
    fn agent_tools_accepts_canonical_block() {
        let source = r#"
feature customer
  agent triage
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer.query.lookup.by_id
      query.by_id
      command.archive
      @tool.web_search
      @tool.calendar.create_event
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            !codes.iter().any(|c| c == "agent_tools_diagnostics"),
            "canonical tool block should not produce agent_tools_diagnostics; got: {codes:?}"
        );
    }

    #[test]
    fn agent_tools_rejects_unknown_kind_segment() {
        let source = r#"
feature customer
  agent broken
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer.script.run_unsafe
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            codes.iter().any(|c| c == "agent_tools_diagnostics"),
            "expected agent_tools_diagnostics for unknown kind; got: {codes:?}"
        );
    }

    #[test]
    fn agent_tools_rejects_empty_segment() {
        // `customer..by_id` has an empty segment — must be rejected.
        let source = r#"
feature customer
  agent broken
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    tools
      customer..by_id
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_tools_diagnostics"),
            "expected agent_tools_diagnostics for empty segment"
        );
    }

    #[test]
    fn agent_evals_accepts_case_with_requires_forbids() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      case redacts_email
        requires customer.email = "ada@example.com"
        forbids output contains @semantic.Email
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            !codes.iter().any(|c| c == "agent_evals_diagnostics"),
            "canonical evals block should not produce agent_evals_diagnostics; got: {codes:?}"
        );
        assert!(
            !codes.iter().any(|c| c == "eval_nondeterministic_warning"),
            "agent pinned at temperature 0 + seed 1 must not warn nondeterministic; got: {codes:?}"
        );
    }

    #[test]
    fn agent_evals_rejects_given_expect_legacy_vocabulary() {
        let source = r#"
feature customer
  agent legacy
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
    evals
      given a_case
        expect output contains "ok"
"#;
        let diagnostics = diagnostics_for(source);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("`given` is legacy")),
            "expected `given` legacy diagnostic; got: {messages:?}"
        );
        assert!(
            messages.iter().any(|m| m.contains("`expect` is legacy")),
            "expected `expect` legacy diagnostic; got: {messages:?}"
        );
    }

    #[test]
    fn agent_discriminator_rejects_when_marker_outside_record() {
        // Field `tag: Status discriminator` declared inside `agent
        // input` instead of a record — must be rejected.
        let source = r#"
feature customer
  agent classify
    input
      message: Text required
      tag: Status discriminator
    policy @policy.read
    output discriminator Intent
    model @llm.default
    temperature 0
    seed 1
    prompt "./p.md"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_discriminator_diagnostics"),
            "expected agent_discriminator_diagnostics when marker appears outside record"
        );
    }

    #[test]
    fn agent_evals_warns_without_temperature_zero_seed() {
        // Agent has an evals block but `temperature 0.7` (non-zero) and
        // no `seed` — must emit `eval_nondeterministic_warning`.
        let source = r#"
feature customer
  agent flaky
    input
      message: Text required
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0.7
    prompt "./p.md"
    evals
      case smoke
        requires output contains "ok"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "eval_nondeterministic_warning"),
            "expected eval_nondeterministic_warning"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A.7 — `expose http` file-local LSP tests
    // -------------------------------------------------------------------------

    #[test]
    fn agent_expose_local_path_conflict_caught() {
        // Two agents in the same file declare the same (method, path).
        let source = r#"
feature customer
  agent first
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:id"
      route id: ID

  agent second
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./q.md"
    expose http
      method POST
      path "/api/x/:other"
      route other: ID
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_expose_path_conflict_local_diagnostics"),
            "expected local path conflict; got: {:?}",
            diagnostic_codes(&diagnostics)
        );
    }

    #[test]
    fn agent_expose_slot_unbound_caught() {
        let source = r#"
feature customer
  agent broken
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:customer_id"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_expose_slot_unbound_diagnostics"),
            "expected slot_unbound"
        );
    }

    #[test]
    fn agent_expose_slot_must_use_route_caught_with_input_slot_collision() {
        let source = r#"
feature customer
  agent broken
    input
      customer_id: ID required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/x/:customer_id"
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            codes
                .iter()
                .any(|c| c == "agent_expose_slot_must_use_route_diagnostics"),
            "expected slot_must_use_route; got: {codes:?}"
        );
    }

    #[test]
    fn agent_expose_method_get_streaming_warns() {
        let source = r#"
feature customer
  agent flaky
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method GET
      path "/api/customers/:id/summary"
      route id: ID
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "agent_expose_method_streaming_mismatch_warning"),
            "expected method/streaming warning"
        );
    }

    #[test]
    fn agent_expose_well_formed_emits_nothing() {
        let source = r#"
feature customer
  agent summarize
    input
      customer_id: Customer.ID required
    policy @policy.read
    output stream Text
    model @llm.default
    prompt "./p.md"
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
      route customer_id: Customer.ID
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        for code in [
            "agent_expose_path_conflict_local_diagnostics",
            "agent_expose_slot_unbound_diagnostics",
            "agent_expose_slot_must_use_route_diagnostics",
            "agent_expose_method_streaming_mismatch_warning",
        ] {
            assert!(
                !codes.iter().any(|c| c == code),
                "well-formed expose should not produce {code}; got: {codes:?}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Cut A.11 — `cors` block file-local LSP tests
    // -------------------------------------------------------------------------

    #[test]
    fn cors_rejects_unknown_child() {
        let source = r#"
app MyApp
  cors
    allow_methods GET, POST
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "cors_contract_diagnostics"),
            "expected cors_contract_diagnostics for unknown child `allow_methods`"
        );
    }

    #[test]
    fn cors_rejects_allow_origins_without_origins() {
        let source = r#"
app MyApp
  cors
    allow_origins production
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "cors_contract_diagnostics"),
            "expected cors_contract_diagnostics for missing origins"
        );
    }

    #[test]
    fn cors_rejects_invalid_allow_credentials() {
        let source = r#"
app MyApp
  cors
    allow_credentials yes
"#;
        let diagnostics = diagnostics_for(source);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages.iter().any(|m| m.contains("allow_credentials yes")),
            "expected diagnostic about invalid allow_credentials value; got {messages:?}"
        );
    }

    #[test]
    fn cors_well_formed_emits_nothing() {
        let source = r#"
app MyApp
  cors
    allow_origins production "https://app.example.com", "https://*.example.com"
    allow_origins local "*"
    allow_credentials true
    max_age "1h"
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            !diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "cors_contract_diagnostics"),
            "well-formed cors must not produce cors_contract_diagnostics"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A.9 — `approval` file-local LSP tests
    // -------------------------------------------------------------------------

    #[test]
    fn approval_rejects_missing_required_children() {
        let source = r#"
feature customer
  command archive
    approval
      by @role.admin
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "approval_contract_diagnostics"),
            "expected approval_contract_diagnostics for missing timeout/then"
        );
    }

    #[test]
    fn approval_rejects_unknown_then_action() {
        let source = r#"
feature customer
  command archive
    approval
      by @role.admin
      timeout "24h"
      then escalate
"#;
        let diagnostics = diagnostics_for(source);
        let messages: Vec<&str> = diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("`approval then escalate`")),
            "expected diagnostic about invalid then value; got: {messages:?}"
        );
    }

    #[test]
    fn approval_well_formed_emits_nothing() {
        let source = r#"
feature customer
  command archive
    approval
      required_when target.tier = enterprise
      by @role.admin
      timeout "24h"
      then deny
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            !diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "approval_contract_diagnostics"),
            "well-formed approval must not produce approval_contract_diagnostics"
        );
    }

    // -------------------------------------------------------------------------
    // Cut A.8 — reserved trace event name (LSP file-local)
    // -------------------------------------------------------------------------

    #[test]
    fn event_trace_agent_run_authored_is_rejected() {
        let source = r#"
feature customer
  domain
    event.trace agent_run
      payload
        agent_id: ID
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "event_trace_reserved_name_diagnostics"),
            "expected reserved-name diagnostic"
        );
    }

    #[test]
    fn event_trace_custom_name_is_allowed() {
        let source = r#"
feature customer
  domain
    event.trace custom_metric
      payload
        value: Integer
"#;
        let diagnostics = diagnostics_for(source);
        assert!(
            !diagnostic_codes(&diagnostics)
                .iter()
                .any(|c| c == "event_trace_reserved_name_diagnostics"),
            "non-reserved trace events must be allowed"
        );
    }

    #[test]
    fn agent_discriminator_allows_marker_inside_record() {
        // Sanity gate: `discriminator` on a record field is the
        // canonical use; must not fire the file-local diagnostic.
        let source = r#"
feature customer
  domain
    record Action
      kind: ActionKind discriminator
      customer_id: Customer.ID optional
"#;
        let diagnostics = diagnostics_for(source);
        let codes = diagnostic_codes(&diagnostics);
        assert!(
            !codes.iter().any(|c| c == "agent_discriminator_diagnostics"),
            "canonical record-field marker must not produce agent_discriminator_diagnostics; got: {codes:?}"
        );
    }

    #[test]
    fn lzx_rejects_cascade_and_unscoped_platform_views() {
        let source = r#"
surface web
  view list Table
    columns += score
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("put the experience name before the platform") })
        );
        assert!(messages.iter().any(|message| {
            message.contains("concrete `.lzx` surfaces must declare `uses experience <name>`")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("concrete platform views live under `audience ...` blocks")
        }));
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("`.lzx` forbids partial overrides") })
        );
    }

    #[test]
    fn lzx_warns_for_implicit_navigation_and_submit_targets() {
        let source = r#"
experience customer
  imports customer

  view list
    source customer.query.list
    opens detail

surface customer web
  uses experience customer

  audience public
    view capture Form
      fields name, email
      submit create
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("view navigation should bind route arguments explicitly")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("platform form submits should use an explicit command reference")
        }));
    }

    #[test]
    fn lzx_warns_for_route_references_without_view_route_contract() {
        let source = r#"
experience customer
  imports customer

  view detail
    source customer.query.by_id(id: route.id)
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("does not declare `route id: ...`")
        }));
    }

    #[test]
    fn lzx_warns_for_routed_actions_without_route_arguments() {
        let source = r#"
experience customer
  imports customer

  view detail
    route id: Customer.ID
    source customer.query.by_id(id: route.id)
    action archive -> customer.workflow.lifecycle.archive
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("actions in routed views should pass route arguments explicitly")
        }));
    }

    #[test]
    fn lzx_warns_for_web_primitives_in_mobile_projection() {
        let source = r#"
surface customer mobile
  uses experience customer

  audience sales
    view list Table
      columns name

    view detail SidePanel
      sections header
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.message.contains("mobile-native primitives"))
                .count(),
            2
        );
    }

    #[test]
    fn lzx_warns_for_legacy_extension_blocks_without_slot() {
        let source = r#"
experience customer_tags
  imports customer_tags, customer

  extends @anchor.customer_detail
    block @client.tag_editor
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("view extensions should place blocks under an explicit slot")
        }));
    }

    #[test]
    fn lzx_filename_suffix_must_match_surface_header() {
        let source = r#"
surface customer mobile
  uses experience customer

  audience sales
    view list CardList
"#;
        let uri = Url::parse("file:///workspace/features/customer/customer.web.lzx").unwrap();

        let diagnostics = diagnostics_for_uri(&uri, source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`customer.web.lzx` is a `web` projection")
        }));
    }

    #[test]
    fn lzx_platform_suffix_must_be_terminal() {
        let source = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
"#;
        let uri = Url::parse("file:///workspace/features/customer/customer.web.admin.lzx").unwrap();

        let diagnostics = diagnostics_for_uri(&uri, source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("abstract `.lzx` files declare `experience <name>`")
        }));
    }

    #[test]
    fn lzx_abstract_file_cannot_declare_concrete_surface() {
        let source = r#"
surface customer web
  uses experience customer

  audience admin
    view list Table
"#;
        let uri = Url::parse("file:///workspace/features/customer/customer.lzx").unwrap();

        let diagnostics = diagnostics_for_uri(&uri, source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("abstract `.lzx` files declare `experience <name>`")
        }));
    }

    #[test]
    fn canonical_warns_for_legacy_non_goals_shape() {
        // Iron-hand context vocabulary added the flat quoted-string
        // shape as a first-class authoring option, so the rule now
        // only flags `key: value` direct-keys (legacy partitioned
        // bareword entries that escaped the canonical groups).
        let source = r#"
feature customer
  purpose "Customers"

  non_goals
    user: "staff authentication"
    anti_pattern.generic_etl: "generic ETL"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("either bare quoted strings (flat shape) or grouped under")
        }));
    }

    #[test]
    fn canonical_accepts_flat_non_goals_shape() {
        // Iron-hand canonical form: bare quoted strings at indent 4.
        // The legacy `non-goals-shape` warning must NOT fire here.
        let source = r#"
feature customer
  purpose "Customers"

  non_goals
    "Full marketplace listing optimization"
    "Real-time chat (use messaging feature)"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            !diagnostics.iter().any(|diagnostic| {
                diagnostic
                    .message
                    .contains("non_goals` entries must be either bare quoted strings")
            }),
            "flat quoted-string form must not trip `non-goals-shape`"
        );
    }

    #[test]
    fn canonical_warns_for_unscoped_defaults_policy() {
        let source = r#"
feature outreach
  purpose "Outreach"

  defaults
    policy @actor.system
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("feature-level policy defaults should use `policy_for")
        }));
    }

    #[test]
    fn canonical_warns_for_legacy_validation_syntax() {
        let source = r#"
feature import
  purpose "Import"

  domain
    resource ImportRow
      raw: JSON required
      validate "./domain/validate_row.go"

    resource Customer
      tier: Text required
      validates tier "./hooks/validate_tier.go"
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(messages.iter().any(|message| {
            message.contains("validators are referenced through `validates @validator.<name>`")
        }));
    }

    #[test]
    fn canonical_warns_for_redundant_validates_scope_keyword() {
        let scoped_field = r#"
feature customer
  domain
    resource Customer
      tier: Text required
      validates field tier @validator.tier_check
"#;
        assert!(
            diagnostics_for(scoped_field)
                .iter()
                .any(|d| d.message.contains("drop the `field <name>` prefix"))
        );

        let scoped_resource = r#"
feature customer
  domain
    resource Customer
      tier: Text required
      validates resource @validator.row_check
"#;
        assert!(
            diagnostics_for(scoped_resource)
                .iter()
                .any(|d| d.message.contains("drop the `resource` prefix"))
        );
    }

    #[test]
    fn canonical_warns_for_self_in_command_target_context() {
        let source = r#"
feature customer_auth
  purpose "Auth"

  command enable_mfa
    route customer_id: ID
    target customer.query.by_id(id: route.customer_id)
    policy @actor.system
    creates CustomerMfaConfig
      customer = self
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("commands and declarative jobs should use `target`")
        }));
    }

    #[test]
    fn canonical_warns_when_required_field_is_checked_against_nil() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      owner: User required
      tier: CustomerTier = enterprise

    rule "enterprise customers require owner"
      deny Customer.activate when self.tier = CustomerTier.enterprise AND self.owner = nil
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`Customer.owner` is declared `required`")
        }));
    }

    #[test]
    fn canonical_warns_when_trace_event_is_used_as_trigger() {
        let source = r#"
feature customer_import
  purpose "Import"

  domain
    event.trace customer_webhook_received
      external_id: Text

  job react_to_trace
    trigger event customer_webhook_received
    handler "./jobs/react.go"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`event.trace` declarations are outside the reaction graph")
        }));
    }

    #[test]
    fn canonical_warns_for_event_consumer_payload_not_declared_by_producer() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id

    event customer_created
      email: @semantic.Email

feature audit
  purpose "Audit"

  uses customer

  domain
    resource AuditEvent
      subject_id: ID required

  job record_customer_created
    trigger event customer.customer_created
    idempotency by envelope.id
    creates AuditEvent
      subject_id = payload.account_id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
                "`payload.account_id` is not declared by event `customer.customer_created`",
            )
        }));
    }

    #[test]
    fn canonical_event_group_can_own_short_event_declarations() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      name: Text required

    event_group customer_* on Customer
      payload
        customer_id = id
        org_id = org.id

      event created
        email: @semantic.Email

feature audit
  purpose "Audit"

  uses customer

  domain
    resource AuditEvent
      subject_id: ID required

  job record_customer_created
    trigger event customer.customer_created
    tenant_from payload.org_id
    idempotency by envelope.id
    creates AuditEvent
      subject_id = payload.customer_id
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_warns_for_unknown_sql_return_type() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    query.sql lifetime_value
      returns CustomerLtv[]
      sql "./queries/customer_lifetime_value.sql"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("return type `CustomerLtv` should resolve")
        }));
    }

    #[test]
    fn canonical_accepts_sql_return_record_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    record CustomerLtv
      customer_id: ID
      amount: @semantic.Money

    query.sql lifetime_value
      returns CustomerLtv[]
      sql "./queries/customer_lifetime_value.sql"
"#;

        assert!(diagnostics_for(source).is_empty());
    }

    #[test]
    fn canonical_command_warns_for_undeclared_route_reference() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

  policies
    update: @role.admin

  command rename
    input name
    target query.by_id(id: route.id)
    policy @policy.update
    rate_limit "30 per minute per user"
    updates Customer
      name = input.name
"#;

        let diagnostics = diagnostics_for_lsp_only(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("references `route.id` but does not declare `route id: ...`")
        );
    }

    #[test]
    fn canonical_command_accepts_short_input_when_fields_exist() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required
      email: @semantic.Email required

  policies
    create: @role.admin

  command create
    input name, email
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Customer
      name = input.name
      email = input.email
"#;

        assert!(diagnostics_for_lsp_only(source).is_empty());
    }

    #[test]
    fn canonical_command_warns_for_short_input_not_on_resource() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

  policies
    create: @role.admin

  command create
    input display_name
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Customer
      name = input.display_name
"#;

        let diagnostics = diagnostics_for_lsp_only(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("uses short input `display_name`")
        );
    }

    #[test]
    fn canonical_command_warns_for_short_input_without_inference_resource() {
        let source = r#"
feature user
  purpose "User auth"

  policies
    login: @scope.public

  command login
    input email, password
    policy @policy.login
    rate_limit "5 per 10 minutes per ip"
    returns AuthSession
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic
                .message
                .contains("short inputs require a local `creates` or `updates` resource")
        }));
    }

    #[test]
    fn canonical_command_warns_for_short_input_on_delete_only_command() {
        let source = r#"
feature customer_tags
  purpose "Customer tags"

  domain
    resource CustomerTagAssignment
      customer: Customer required
      tag: CustomerTag required

    query.lookup assignment_by_customer_tag

  policies
    update: @role.admin

  command remove_tag
    input customer_id, tag_id
    target query.assignment_by_customer_tag(customer_id: input.customer_id, tag_id: input.tag_id)
    policy @policy.update
    rate_limit "60 per minute per user"
    deletes CustomerTagAssignment
"#;

        let diagnostics = diagnostics_for_lsp_only(source);

        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().all(|diagnostic| {
            diagnostic
                .message
                .contains("short inputs require a local `creates` or `updates` resource")
        }));
    }

    #[test]
    fn canonical_command_warns_for_short_input_with_multiple_inference_resources() {
        let source = r#"
feature inventory
  purpose "Inventory transfers"

  domain
    resource SourceStock
      amount: Integer required

    resource TargetStock
      amount: Integer required

  policies
    update: @role.admin

  command transfer
    route id: ID
    input amount
    policy @policy.update
    rate_limit "60 per minute per user"
    updates SourceStock
      amount = input.amount
    updates TargetStock
      amount = input.amount
"#;

        let diagnostics = diagnostics_for_lsp_only(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("short inputs require exactly one local `creates` or `updates` resource")
        );
    }

    #[test]
    fn canonical_command_accepts_typed_input_not_on_resource() {
        let source = r#"
feature customer_tags
  purpose "Customer tags"

  domain
    resource CustomerTagAssignment
      customer: Customer required
      tag: CustomerTag required

    query.lookup assignment_by_customer_tag

  policies
    update: @role.admin

  command remove_tag
    input
      customer_id: ID
      tag_id: ID
    target query.assignment_by_customer_tag(customer_id: input.customer_id, tag_id: input.tag_id)
    policy @policy.update
    rate_limit "60 per minute per user"
    deletes CustomerTagAssignment
"#;

        assert!(diagnostics_for_lsp_only(source).is_empty());
    }

    #[test]
    fn canonical_warns_when_validator_result_does_not_block_command() {
        let source = r#"
feature customer_auth
  purpose "Customer auth"

  domain
    resource CustomerMfaConfig

  policies
    update: @role.admin

  command enable_mfa
    input
      totp_code: Text required
    let totp_verified = @validator.verify_customer_totp(code: input.totp_code)
    policy @policy.update
    rate_limit "10 per minute per user"
    creates CustomerMfaConfig

  extensions
    validator verify_customer_totp: Validator[TotpVerifyInput]
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("is computed but not required") })
        );
    }

    #[test]
    fn canonical_warns_for_previously_without_mode() {
        let legacy = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer previously Account
"#;
        let canonical = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer previously migrated Account
"#;

        assert!(diagnostics_for(legacy).iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`previously` should declare `migrated` or `alias`")
        }));
        assert!(!diagnostics_for(canonical).iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`previously` should declare `migrated` or `alias`")
        }));
    }

    #[test]
    fn canonical_warns_for_legacy_ergonomic_syntax() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required
      email: Email required

    query list

  policies
    create: role_admin

  command create
    input name, email
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Customer
      name = input.name
      email = input.email

  job sync
    trigger event customer.customer_created
    idempotency event.id
    policy @actor.system
    handler "./jobs/sync.go"

  surface web admin
    view list Table
      source query.list

      cells
        email ext.email_cell
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages.iter().any(|message| {
                message.contains("query declarations should use an explicit mode")
            })
        );
        assert!(
            messages
                .iter()
                .any(|message| { message.contains("policy atoms should be namespaced") })
        );
        assert!(messages.iter().any(|message| {
            message.contains("semantic types should use the `@semantic.*` namespace")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("extension references should use capability namespaces")
        }));
        assert!(messages.iter().any(|message| {
            message.contains("`idempotency` should declare its source with `by`")
        }));
    }

    #[test]
    fn canonical_warns_for_unknown_query_mode() {
        let source = r#"
feature customer
  domain
    resource Customer
      name: Text required

  query.fancy something
"#;

        assert!(
            diagnostics_for(source)
                .iter()
                .any(|d| d.message.contains("unknown query mode"))
        );
    }

    #[test]
    fn canonical_formatter_preserves_full_capsule_fixture() {
        let source = include_str!("../../../examples/full-capsule/full-capsule.lzi");
        let formatted = format_canonical_source(source).expect("canonical source");

        assert_eq!(formatted, source);
    }

    #[test]
    fn canonical_warns_for_authored_command_policy_matrix_tests() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

  policies
    update: @role.admin

  command rename
    input
      name: Text
    policy @policy.update
    rate_limit "30 per hour per user"
    creates Customer
      name = input.name

    tests
      permits @role.admin
"#;

        let diagnostics = diagnostics_for_lsp_only(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("policy actor-matrix tests are generated")
        );
    }

    #[test]
    fn canonical_warns_for_explicit_default_list_order() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

    query.list list
      order created_at desc
      paginate 50
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("defaults to `order created_at desc`")
        );
    }

    #[test]
    fn canonical_warns_for_explicit_generated_filter_index() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  domain
    resource Customer
      status: CustomerStatus = lead

    constraints
      index org, status

    query.list list
      params
        status: CustomerStatus optional

      filters
        status when params.status
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            diagnostics[0]
                .message
                .contains("filters generate this tenant-aware index")
        );
    }

    #[test]
    fn canonical_warns_for_search_encoded_as_filter_equality() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      name: Text required

    query.list list
      params
        search: Text optional

      filters
        name = params.search when params.search
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("text matching should use `search params.search over ...`")
        );
    }

    #[test]
    fn canonical_warns_for_invalid_pagination_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

    query.lookup by_id by id: ID
      paginate 0
"#;

        let diagnostics = diagnostics_for(source);
        let messages: Vec<_> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();

        assert!(
            messages
                .iter()
                .any(|message| { message.contains("`paginate` is a `query.list` contract") })
        );
        assert!(
            messages.iter().any(|message| {
                message.contains("`paginate` should declare a positive integer")
            })
        );
    }

    #[test]
    fn canonical_warns_for_file_capability_without_contract() {
        let source = r#"
feature import_csv
  purpose "Import CSV"

  domain
    resource ImportBatch
      file: @cap.File required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`@cap.File` should declare `max_size:<size>` and `accept:<mime>`")
        }));
    }

    #[test]
    fn canonical_warns_for_invalid_file_capability_size() {
        let source = r#"
feature import_csv
  purpose "Import CSV"

  domain
    resource ImportBatch
      file: @cap.File(max_size:large,accept:text/csv) required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("`@cap.File` max_size should use a positive size literal")
        }));
    }

    #[test]
    fn canonical_warns_for_pii_resource_without_retention() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      email: @semantic.Email @pii.contact required
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("stores `@pii.*` fields and should declare `retention")
        }));
    }

    #[test]
    fn canonical_warns_for_invalid_retention_contract() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer
      email: @semantic.Email @pii.contact required
      retention seven-years then purge
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("retention duration should be `forever`")
        }));
    }

    #[test]
    fn canonical_warns_for_invalid_write_window_contract() {
        let source = r#"
feature billing
  purpose "Billing"

  command create
    write_window input.issued_at billing.open_period
    policy @role.admin
    rate_limit "30 per minute per user"
    creates Invoice
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("write-window guards use `write_window by")
        }));
    }

    #[test]
    fn canonical_warns_for_active_sessions_without_temporal_scope() {
        let source = r#"
feature user_auth
  purpose "User auth"

  domain
    query.list active_sessions
      params
        user_id: ID

      filters
        user.id = params.user_id
        expires_at != nil
"#;

        let diagnostics = diagnostics_for(source);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.message.contains("can include expired sessions") })
        );
    }

    #[test]
    fn canonical_warns_when_active_session_modifier_has_no_temporal_contract() {
        let source = r#"
feature user_auth
  purpose "User auth"

  domain
    query.list active_sessions
      modifier @query_modifier.active_session_scope

      params
        user_id: ID

      filters
        user.id = params.user_id
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should declare temporal validity")
        }));
    }

    #[test]
    fn canonical_warns_for_tenant_scheduled_job_without_fanout() {
        let source = r#"
feature customer
  purpose "Customers"

  defaults
    tenancy org

  job recompute_scores
    trigger schedule "0 2 * * *"
    handler "./jobs/recompute_scores.go"
"#;

        let diagnostics = diagnostics_for(source);

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("should declare `fanout tenants org`")
        }));
    }

    #[test]
    fn canonical_formatter_removes_blank_before_transition_children() {
        let source = r#"
feature customer
  purpose "Customers"

  workflow lifecycle on Customer.status
    policy @policy.update

    resume: paused -> active

      tests
        allows from paused
"#;

        let formatted = format_canonical_source(source).expect("canonical source");

        assert!(
            formatted.contains("    resume: paused -> active\n      tests"),
            "transition children should stay contiguous with the header:\n{formatted}"
        );
    }

    #[test]
    fn canonical_order_reports_late_uses() {
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

  uses org
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("`uses` appears after `domain`")
        );
    }

    #[test]
    fn canonical_order_reports_late_webhook_after_surface() {
        let source = r#"
registry
  env
    server STRIPE_WEBHOOK_SECRET: Secret required

feature billing
  purpose "Billing"

  domain
    resource Invoice

  surface web admin
    view list Table

  webhook stripe_invoice_paid
    path "/webhooks/stripe/invoice-paid"
    verify hmac sha256
      secret env.STRIPE_WEBHOOK_SECRET
      header "Stripe-Signature"
    idempotency by payload.provider_event_id
"#;

        let diagnostics = diagnostics_for(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("`webhook` appears after `surface`")
        );
    }

    #[test]
    fn canonical_formatter_reorders_feature_blocks() {
        let source = r#"
registry
  env
    server INBOUND_WEBHOOK_SECRET: Secret required

feature customer
  purpose "Customers"

  surface web admin
    view list Table

  uses org

  domain
    resource Customer

  webhook inbound
    path "/webhooks/inbound"
    verify hmac sha256
      secret env.INBOUND_WEBHOOK_SECRET
      header "X-Signature"
    idempotency by payload.id
"#;

        let formatted = format_canonical_source(source).expect("canonical source");

        assert!(
            formatted.find("  uses org").unwrap() < formatted.find("  domain").unwrap(),
            "uses should move before domain:\n{formatted}"
        );
        assert!(
            formatted.find("  webhook inbound").unwrap()
                < formatted.find("  surface web admin").unwrap(),
            "webhook should move before surface:\n{formatted}"
        );
        assert!(
            diagnostics_for(&formatted).is_empty(),
            "formatter should produce canonical order"
        );
    }

    // ----------------------------------------------------------------
    // Row 30 — Storage bucket cycle: hovers + closed-catalog completions
    // for `@cap.File(...)` argument keywords.
    // ----------------------------------------------------------------

    use super::{
        DESIGN_KEYWORDS, KEYWORDS, cap_file_value_completions, completion_items_for_uri,
        design_keyword_description, keyword_description,
    };
    use tower_lsp::lsp_types::Position;

    #[test]
    fn design_lzi_completion_surfaces_token_groups() {
        let uri = Url::parse("file:///workspace/design.lzi").unwrap();
        let items = completion_items_for_uri(&uri);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();

        for group in [
            "color",
            "typography",
            "space",
            "radius",
            "shadow",
            "motion",
            "breakpoint",
            "z",
        ] {
            assert!(
                labels.contains(&group),
                "`design.lzi` completions should include `{group}`"
            );
        }
    }

    #[test]
    fn feature_lzi_does_not_surface_design_keywords() {
        let uri = Url::parse("file:///workspace/features/customer/customer.lzi").unwrap();
        let items = completion_items_for_uri(&uri);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();

        for design_only in [
            "color",
            "typography",
            "space",
            "radius",
            "shadow",
            "motion",
            "breakpoint",
            "z",
        ] {
            assert!(
                !labels.contains(&design_only),
                "feature `.lzi` completions should not include design keyword `{design_only}`"
            );
        }
    }

    #[test]
    fn design_keyword_hovers_link_to_proposal() {
        for kw in DESIGN_KEYWORDS {
            let description = design_keyword_description(kw)
                .unwrap_or_else(|| panic!("hover for `{kw}` missing"));
            assert!(description.contains("docs/proposals/design-tokens.md"));
        }
    }

    #[test]
    fn keyword_hover_describes_cap_file_arguments() {
        for kw in ["max_size", "accept", "visibility", "signed_ttl"] {
            let description =
                keyword_description(kw).unwrap_or_else(|| panic!("hover for `{kw}` missing"));
            assert!(
                !description.is_empty(),
                "hover for `{kw}` must be non-empty"
            );
        }
    }

    // Encryption bucket cycle — hover catalog for the `encryption`
    // block. `key`, `source`, `algorithm` are already in the catalog
    // (claimed by sibling bucket cycles); only `encryption` and
    // `rotation` are new tokens. See
    // `docs/proposals/encryption-vocab.md` §LSP hovers.
    #[test]
    fn keyword_hover_describes_encryption_block() {
        let description = keyword_description("encryption").expect("encryption hover present");
        assert!(description.contains("@key."));
        assert!(description.contains("@cap.Encrypted"));
    }

    #[test]
    fn keyword_hover_describes_rotation_strategy() {
        let description = keyword_description("rotation").expect("rotation hover present");
        assert!(description.contains("manual"));
    }

    #[test]
    fn keyword_hover_describes_cap_file_decorator() {
        for kw in ["@cap.File", "cap.File"] {
            assert!(
                keyword_description(kw).is_some(),
                "hover for `{kw}` must be available"
            );
        }
    }

    #[test]
    fn keyword_hover_visibility_lists_closed_catalog() {
        let description = keyword_description("visibility").unwrap();
        assert!(description.contains("public"));
        assert!(description.contains("private"));
        assert!(description.contains("signed"));
    }

    #[test]
    fn keyword_hover_describes_tenant_migration_children() {
        let description = keyword_description("tenant_migration").unwrap();
        assert!(description.contains("target query."));
        assert!(description.contains("axis <tenant_axis>"));
        assert!(description.contains("idempotency <path>"));
        assert!(
            keyword_description("axis")
                .unwrap()
                .contains("defaults.tenancy")
        );
    }

    #[test]
    fn keywords_list_contains_storage_arguments() {
        for kw in ["max_size", "accept", "visibility", "signed_ttl"] {
            assert!(
                KEYWORDS.contains(&kw),
                "`KEYWORDS` should list `{kw}` so completions surface it"
            );
        }
    }

    #[test]
    fn cap_file_value_completion_for_visibility_offers_closed_catalog() {
        let source = "    output @cap.File(max_size:10mb,accept:text/csv,visibility:";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = cap_file_value_completions(source, position).expect("visibility offers");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["public", "private", "signed"]);
    }

    #[test]
    fn cap_file_value_completion_for_max_size_offers_units() {
        let source = "    file: @cap.File(max_size:25";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = cap_file_value_completions(source, position).expect("max_size offers");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["kb", "mb", "gb"]);
    }

    #[test]
    fn cap_file_value_completion_for_signed_ttl_offers_units() {
        let source =
            "    output @cap.File(max_size:10mb,accept:text/csv,visibility:signed,signed_ttl:1";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = cap_file_value_completions(source, position).expect("signed_ttl offers");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["s", "m", "h", "d"]);
    }

    #[test]
    fn cap_file_value_completion_for_accept_offers_mime_families() {
        let source = "    output @cap.File(max_size:10mb,accept:";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = cap_file_value_completions(source, position).expect("accept offers");
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "text",
                "image",
                "application",
                "audio",
                "video",
                "font",
                "*"
            ]
        );
    }

    #[test]
    fn cap_file_value_completion_returns_none_outside_capability() {
        let source = "    file: Text";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        assert!(cap_file_value_completions(source, position).is_none());
    }

    #[test]
    fn error_page_hover_and_status_completion_are_available() {
        let hover = rich_keyword_hover("error_page").expect("error_page hover");
        assert!(hover.contains("Closed catalog") || hover.contains("closed catalog"));

        let source = "  error_page 4";
        let position = Position {
            line: 0,
            character: source.len() as u32,
        };
        let items = context_aware_completions(source, position).expect("status completions");
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
        assert!(labels.contains(&"404"));
        assert!(labels.contains(&"503"));
    }

    #[test]
    fn error_page_child_completion_offers_template_and_audience() {
        let source = "app Acme\n  error_page 404\n    ";
        let position = Position {
            line: 2,
            character: 4,
        };
        let items = context_aware_completions(source, position).expect("child completions");
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, vec!["template", "audience"]);
    }

    #[test]
    fn error_page_audience_completion_offers_common_values() {
        let source = "app Acme\n  error_page 404\n    audience p";
        let position = Position {
            line: 2,
            character: "    audience p".len() as u32,
        };
        let items = context_aware_completions(source, position).expect("audience completions");
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
        assert!(labels.contains(&"public"));
    }

    // ----------------------------------------------------------------
    // Notifications expanded bucket cycle — hovers + closed-catalog
    // completions for `notification.digest` / `notification.throttle`.
    // ----------------------------------------------------------------

    #[test]
    fn keyword_hover_describes_notification_digest_children() {
        for kw in [
            "digest",
            "every",
            "group_by",
            "max_size",
            "template_strategy",
        ] {
            assert!(
                keyword_description(kw).is_some(),
                "hover for `{kw}` must be available"
            );
        }
    }

    #[test]
    fn keyword_hover_describes_notification_throttle_children() {
        for kw in [
            "throttle",
            "max_per",
            "per_recipient",
            "per_channel",
            "burst",
        ] {
            assert!(
                keyword_description(kw).is_some(),
                "hover for `{kw}` must be available"
            );
        }
    }

    #[test]
    fn keyword_hover_throttle_distinguishes_from_rate_limit() {
        let throttle = keyword_description("throttle").unwrap();
        assert!(
            throttle.contains("per-recipient") || throttle.contains("Distinct from"),
            "throttle hover must call out the distinction from scalar rate_limit; got `{throttle}`"
        );
    }

    #[test]
    fn keywords_list_contains_notification_subblocks() {
        for kw in [
            "digest",
            "throttle",
            "every",
            "group_by",
            "max_size",
            "template_strategy",
            "max_per",
            "per_recipient",
            "per_channel",
            "burst",
        ] {
            assert!(
                KEYWORDS.contains(&kw),
                "`KEYWORDS` should list `{kw}` so completions surface it"
            );
        }
    }

    #[test]
    fn notification_digest_template_strategy_catalog_has_two_entries() {
        use super::NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES;
        assert_eq!(NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES.len(), 2);
        for value in NOTIFICATION_DIGEST_TEMPLATE_STRATEGY_VALUES {
            assert!(
                super::notification_digest_template_strategy_detail(value).is_some(),
                "detail for `{value}` must be available"
            );
        }
    }

    #[test]
    fn keyword_hover_describes_webhook_event_registry_kind() {
        let hover = keyword_description("webhook_event").expect("webhook_event hover");
        assert!(hover.contains("outbound"), "{hover}");
        assert!(hover.contains("Distinct from inbound `webhook`"), "{hover}");
        assert!(keyword_description("previous_version").is_some());
    }

    #[test]
    fn keywords_list_contains_webhook_event_registry_kind() {
        for kw in [
            "webhook_event",
            "payload",
            "version",
            "previous_version",
            "deprecated",
        ] {
            assert!(
                KEYWORDS.contains(&kw),
                "`KEYWORDS` should list `{kw}` so completions surface it"
            );
        }
    }

    // ----------------------------------------------------------------
    // Cell C4 — LSP hover + closed-catalog completion for the
    // resource-level `conventions [..]` opt-in. Specification:
    // `docs/proposals/ir-resource-conventions-crud.md` §4.4.
    // ----------------------------------------------------------------

    #[test]
    fn keyword_hover_describes_conventions_slot() {
        let one_liner =
            keyword_description("conventions").expect("conventions keyword_description present");
        // Verbatim phrasing from the proposal §4.4 — the hover surface,
        // the docstring on `Resource.conventions`, and the doctor
        // diagnostic share this template.
        assert!(
            one_liner.contains("Resource-level conventions opt-in"),
            "conventions one-liner should open with the §4.4 phrasing; got: {one_liner}"
        );
        assert!(
            one_liner.contains("`conventions [<name1>, <name2>, ...]`"),
            "conventions one-liner should show the slot syntax verbatim; got: {one_liner}"
        );
        assert!(
            one_liner.contains("Today's catalog: `crud`, `me`"),
            "conventions one-liner should pin the two-member catalog; got: {one_liner}"
        );
        assert!(
            one_liner.contains("ir-resource-conventions-crud"),
            "conventions one-liner should anchor the crud proposal path; got: {one_liner}"
        );
        assert!(
            one_liner.contains("ir-resource-conventions-me"),
            "conventions one-liner should anchor the me proposal path; got: {one_liner}"
        );
    }

    #[test]
    fn rich_keyword_hover_describes_conventions_slot() {
        let rich = super::rich_keyword_hover("conventions")
            .expect("conventions rich_keyword_hover present");
        assert!(
            rich.contains("Resource-level conventions opt-in"),
            "rich hover should preserve the §4.4 phrasing; got: {rich}"
        );
        assert!(
            rich.contains("`crud`"),
            "rich hover should mention the `crud` bundle; got: {rich}"
        );
        assert!(
            rich.contains("Closed catalog") || rich.contains("**Closed catalog**"),
            "rich hover should label its closed-catalog section; got: {rich}"
        );
    }

    /// M3 — the rich hover must list both bundles in its closed-catalog
    /// section, anchor both proposal paths, and use a composition
    /// example (`conventions [crud, me]`) so the surface communicates
    /// inter-bundle composition (§6.1) at the editor surface.
    #[test]
    fn rich_keyword_hover_mentions_both_bundles() {
        let rich = super::rich_keyword_hover("conventions")
            .expect("conventions rich_keyword_hover present");
        assert!(
            rich.contains("`crud`"),
            "rich hover should mention the `crud` bundle; got:\n{rich}"
        );
        assert!(
            rich.contains("`me`"),
            "rich hover should mention the `me` bundle; got:\n{rich}"
        );
        assert!(
            rich.contains("ir-resource-conventions-crud"),
            "rich hover should anchor the crud proposal path; got:\n{rich}"
        );
        assert!(
            rich.contains("ir-resource-conventions-me"),
            "rich hover should anchor the me proposal path; got:\n{rich}"
        );
    }

    #[test]
    fn conventions_bundle_hover_on_crud_token_lists_synthesized_entries() {
        let source = r#"
feature customer
  resource Customer
    org: Org required
    name: Text required
    conventions [crud]
"#;
        let offset = source.find("crud").expect("crud token") + 1;
        let hover = super::convention_bundle_hover(
            source,
            super::position_for_offset(source, offset),
            "crud",
        )
        .expect("crud bundle hover");

        assert!(
            hover.contains("`conventions [crud]` synthesizes:"),
            "hover should name the bundle; got:\n{hover}"
        );
        assert!(
            hover.contains("`query.list list_<resource_snake>s`"),
            "hover should list the CRUD list query; got:\n{hover}"
        );
        assert!(
            hover.contains("`query.lookup lookup_<resource_snake>`"),
            "hover should list the CRUD lookup query; got:\n{hover}"
        );
        assert!(
            hover.contains("`command create_<resource_snake>`"),
            "hover should list create command; got:\n{hover}"
        );
        assert!(
            hover.contains("author wins"),
            "hover should explain author override behavior; got:\n{hover}"
        );
    }

    #[test]
    fn conventions_bundle_hover_on_me_token_lists_lookup_my() {
        let source = r#"
feature customer
  resource Customer
    org: Org required
    conventions [crud, me]
"#;
        let offset = source.find("me]").expect("me token") + 1;
        let hover =
            super::convention_bundle_hover(source, super::position_for_offset(source, offset), "me")
                .expect("me bundle hover");

        assert!(
            hover.contains("`conventions [me]` synthesizes:"),
            "hover should name the bundle; got:\n{hover}"
        );
        assert!(
            hover.contains("`query.lookup lookup_my_<resource_snake>`"),
            "hover should list lookup_my query; got:\n{hover}"
        );
        assert!(
            hover.contains("author wins"),
            "hover should explain author override behavior; got:\n{hover}"
        );
    }

    #[test]
    fn conventions_bundle_hover_does_not_fire_for_crud_outside_conventions_list() {
        let source = "feature crud\n";
        let offset = source.find("crud").expect("crud word") + 1;

        assert!(
            super::convention_bundle_hover(
                source,
                super::position_for_offset(source, offset),
                "crud",
            )
            .is_none(),
            "crud should only hover as a convention bundle inside `conventions [...]`"
        );
    }

    #[test]
    fn keywords_list_contains_conventions() {
        assert!(
            KEYWORDS.contains(&"conventions"),
            "`KEYWORDS` should list `conventions` so completions surface it"
        );
    }

    #[test]
    fn conventions_list_completion_inside_brackets_offers_crud_and_me() {
        // Cursor sits inside an open `conventions [` bracket list with
        // no closing `]` on the line. M3 extends the catalog to two
        // bundles; the completer surfaces both.
        let items =
            super::conventions_list_completions("    conventions [")
                .expect("completion should fire inside `conventions [` bracket list");
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["crud", "me"],
            "closed catalog should be `crud, me` (in declaration order)"
        );
    }

    #[test]
    fn conventions_list_completion_after_partial_token_still_offers_crud() {
        // Authoring `conventions [cr<cursor>` is the canonical typo
        // recovery path; the completer should still show `crud`.
        let items = super::conventions_list_completions("    conventions [cr")
            .expect("completion should fire inside `conventions [` with partial token");
        let labels: Vec<&str> =
            items.iter().map(|item| item.label.as_str()).collect();
        assert!(
            labels.contains(&"crud"),
            "closed catalog must still surface `crud`; got: {labels:?}"
        );
    }

    #[test]
    fn conventions_list_completion_outside_brackets_returns_none() {
        // The cursor is on the keyword itself, not inside `[..]`.
        assert!(
            super::conventions_list_completions("    conventions ").is_none(),
            "completion must not fire before the `[` opens the bracket list"
        );
        // The cursor is past a closed bracket list (parser would have
        // accepted it already); no further completions to offer.
        assert!(
            super::conventions_list_completions("    conventions [crud] ").is_none(),
            "completion must not fire after the closing `]`"
        );
    }

    // ----------------------------------------------------------------
    // IR Rate-Limit env-aware — Cell 3 LSP surface. Spec:
    // `docs/proposals/ir-rate-limit-env-aware.md` §11.3.
    // Hover updates the `rate_limit` keyword description to cover the
    // `in <env>` qualifier shape + the closed env catalog; completion
    // inside `rate_limit "..." in <|>` offers the 5-entry catalog.
    // ----------------------------------------------------------------

    #[test]
    fn hover_describes_rate_limit_env_qualifier() {
        // The keyword_description one-liner is the LSP hover seed for
        // the `rate_limit` keyword. Per the cell brief, the description
        // must mention the `in <env>` qualifier shape AND list the
        // closed env catalog so an LLM author hovering on the keyword
        // sees the full surface in one tooltip.
        let description = super::keyword_description("rate_limit")
            .expect("`rate_limit` keyword_description present");
        assert!(
            description.contains("in <env>"),
            "hover must mention `in <env>` qualifier shape; got: {description}"
        );
        assert!(
            description.contains("production"),
            "hover must list `production` in the closed catalog; got: {description}"
        );
        assert!(
            description.contains("staging"),
            "hover must list `staging` in the closed catalog; got: {description}"
        );
        assert!(
            description.contains("test"),
            "hover must list `test` in the closed catalog; got: {description}"
        );
        assert!(
            description.contains("dev"),
            "hover must list `dev` in the closed catalog; got: {description}"
        );
        assert!(
            description.contains("local"),
            "hover must list `local` in the closed catalog; got: {description}"
        );
        assert!(
            description.contains("default"),
            "hover must describe the default-line semantics; got: {description}"
        );
    }

    #[test]
    fn completion_inside_in_offers_env_catalog() {
        // Cursor sits at `rate_limit "5 per 10 minutes per ip" in <|>`.
        // The completer surfaces the 5-entry closed env catalog so an
        // author can pick `production` / `staging` / `test` / `dev` /
        // `local` without typing it from memory.
        let items =
            super::rate_limit_env_completions("  rate_limit \"5 per 10 minutes per ip\" in ")
                .expect("completion should fire inside `in <env>` slot");
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["production", "staging", "test", "dev", "local"],
            "closed env catalog should match `production, staging, test, dev, local`"
        );
        // Sanity: every item is a closed-catalog ENUM_MEMBER (so
        // editors render them distinctly from arbitrary keywords).
        assert!(
            items.iter().all(|i| i.kind
                == Some(super::CompletionItemKind::ENUM_MEMBER)),
            "all env completions should carry `ENUM_MEMBER` kind; got: {items:?}"
        );

        // After committing one env, the completer filters it out so the
        // author doesn't see duplicate offers. Cursor sits at
        // `rate_limit "..." in dev, <|>`.
        let items = super::rate_limit_env_completions(
            "  rate_limit \"5 per 10 minutes per ip\" in dev, ",
        )
        .expect("completion should fire after the comma");
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert!(
            !labels.contains(&"dev"),
            "already-committed `dev` should be filtered; got: {labels:?}"
        );
        assert!(
            labels.contains(&"staging"),
            "remaining catalog entries should still be offered; got: {labels:?}"
        );

        // Negative case: cursor outside the `in <env>` slot — e.g.
        // still mid-spec — must not fire (axis completion owns that).
        assert!(
            super::rate_limit_env_completions("  rate_limit \"5 per 10 minutes per ip\"")
                .is_none(),
            "completer must not fire when the `in` keyword is absent"
        );
        // Negative case: not a rate_limit line at all.
        assert!(
            super::rate_limit_env_completions("  audit default in ").is_none(),
            "completer must only fire on `rate_limit` lines"
        );
    }

    // ----------------------------------------------------------------
    // Cell O3 — `@owner_axis(through: <column>)` field annotation
    // hover + completion. Spec:
    // `docs/proposals/ir-resource-conventions-owner-scope.md`
    // §7.5 + §11.3. The hover one-liner is also surfaced verbatim by
    // the doctor diagnostic phrasing (§11.1 worded for messaging) so
    // the LSP, doctor, and inspect surfaces agree.
    // ----------------------------------------------------------------

    #[test]
    fn hover_describes_owner_axis_annotation() {
        // The verbatim one-liner from §11.3 is surfaced through the
        // `keyword_description` fallback (matches the `@cap.File` /
        // `cap.File` precedent — both `@owner_axis` and `owner_axis`
        // arms must resolve to the same description).
        let with_at = super::keyword_description("@owner_axis")
            .expect("`@owner_axis` keyword_description present");
        let without_at = super::keyword_description("owner_axis")
            .expect("`owner_axis` keyword_description present");
        assert_eq!(
            with_at, without_at,
            "both `@owner_axis` and `owner_axis` must resolve to the same one-liner"
        );
        assert!(
            with_at.contains("Field-level annotation: `@owner_axis(through: <column>)`"),
            "hover should open with the §11.3 verbatim sentence; got: {with_at}"
        );
        assert!(
            with_at.contains("ownership chain"),
            "hover should mention the ownership chain semantics; got: {with_at}"
        );
        assert!(
            with_at.contains("`ctx.User.ID`"),
            "hover should anchor the resolved actor key `ctx.User.ID`; got: {with_at}"
        );
        assert!(
            with_at.contains("ir-resource-conventions-owner-scope.md"),
            "hover should anchor the proposal path; got: {with_at}"
        );

        // The rich Markdown hover gates on the same key; ensure it
        // surfaces the worked example and the doctor codes for the
        // authoring rules (mirroring the `conventions` rich-hover
        // pattern). Cell brief §11.3.
        let rich = super::rich_keyword_hover("@owner_axis")
            .expect("`@owner_axis` rich_keyword_hover present");
        assert!(
            rich.contains("**`@owner_axis`**"),
            "rich hover should bold the annotation name; got:\n{rich}"
        );
        assert!(
            rich.contains("host: Host required @owner_axis(through: user)"),
            "rich hover should include the §11.2 worked Property example; got:\n{rich}"
        );
        assert!(
            rich.contains("owner_axis_on_non_fk"),
            "rich hover should reference the parser-level doctor code; got:\n{rich}"
        );
    }

    #[test]
    fn completion_inside_owner_axis_offers_fk_columns() {
        // Authoring shape: cursor sits at the `<|>` position inside
        // `@owner_axis(through: <|>)`. Per §7.5 + the cell brief, the
        // completer offers the FK fields on the current `resource`
        // block — fields whose type is a bare PascalCase identifier
        // (i.e. a reference to another resource), with the builtin
        // closed-catalog skip list (`Text`/`Integer`/`ID`/…) filtered out.
        let source = "\
feature catalog
  resources
    resource Property
      org: Org required
      host: Host required @owner_axis(through: )
      category: ServiceCategory optional
      name: Text required
      conventions [crud]
";
        // The `through:` keyword sits after `: ` — column position is
        // the byte index immediately after `through: ` on line index 4
        // (0-based; the `host:` line).
        let line_idx = 4u32;
        let line = source
            .lines()
            .nth(line_idx as usize)
            .expect("host line present");
        // Cursor right after `through: ` (the trailing space inside the
        // parens, before the closing `)`).
        let cursor = line.find("through: ").expect("through: present") + "through: ".len();
        let pos = super::Position {
            line: line_idx,
            character: cursor as u32,
        };
        let items =
            super::owner_axis_through_completions(source, pos).expect("completion should fire");
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        // The completer offers all FK fields on the resource. `Org`,
        // `Host`, and `ServiceCategory` are PascalCase resource refs
        // (FK). `name: Text` is filtered by the builtin skip list.
        assert!(
            labels.contains(&"org"),
            "FK field `org: Org` should be offered; got: {labels:?}"
        );
        assert!(
            labels.contains(&"host"),
            "FK field `host: Host` should be offered; got: {labels:?}"
        );
        assert!(
            labels.contains(&"category"),
            "FK field `category: ServiceCategory` should be offered; got: {labels:?}"
        );
        assert!(
            !labels.contains(&"name"),
            "builtin-typed field `name: Text` should NOT be offered; got: {labels:?}"
        );
        // Sanity: every item is a FIELD kind (so editors can tag them
        // differently from KEYWORD entries in the popup).
        assert!(
            items.iter().all(|i| i.kind
                == Some(super::CompletionItemKind::FIELD)),
            "all FK completions should carry `FIELD` kind; got: {items:?}"
        );
    }

    #[test]
    fn completion_outside_owner_axis_returns_none() {
        // Sibling negative case: cursor is on a different line entirely
        // (a plain `command` declaration), so `@owner_axis(...)` is not
        // active. The dedicated completer returns `None`, leaving the
        // global keyword list to take over.
        let source = "\
feature catalog
  resources
    resource Property
      host: Host required @owner_axis(through: user)
      conventions [crud]

  command create_property
    policy @policy.create
";
        let pos = super::Position {
            line: 6,
            character: 4,
        };
        assert!(
            super::owner_axis_through_completions(source, pos).is_none(),
            "completer must not fire outside `@owner_axis(...)`"
        );
    }

    // ----------------------------------------------------------------
    // Wave B — LSP hover + completion coverage for
    // `command`/`query.list`/`query.lookup`/`query.sql`/`query.view`/
    // `api`/`policy`/`effect`/`audit`/`rate_limit`. Each kind gets one hover
    // assertion and one completion assertion so the closed catalogs
    // surface to editors instead of being shape-only strings.
    // ----------------------------------------------------------------

    use super::{
        EFFECT_VERBS, KIND_CHILD_COMPLETIONS, RATE_LIMIT_AXES, block_kind_at,
        context_aware_completions, rich_keyword_hover,
    };
    use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

    /// Helper: assert the rich Markdown hover for `keyword` exists and
    /// contains every snippet in `expected_fragments`. Fragments
    /// double as a smoke test that required-children / optional-
    /// children / example / doc anchor all land in the output.
    fn assert_rich_hover_contains(keyword: &str, expected_fragments: &[&str]) {
        let rendered = rich_keyword_hover(keyword)
            .unwrap_or_else(|| panic!("rich hover for `{keyword}` must be present"));
        for fragment in expected_fragments {
            assert!(
                rendered.contains(fragment),
                "rich hover for `{keyword}` must contain `{fragment}`; got:\n{rendered}"
            );
        }
    }

    #[test]
    fn rich_hover_for_command_describes_required_and_optional_children() {
        assert_rich_hover_contains(
            "command",
            &[
                "**`command`**",
                "**Required children**",
                "policy @policy.",
                "creates",
                "**Optional children**",
                "rate_limit",
                "audit",
                "emits",
                "invalidates",
                "**Example**",
                "```lazuli",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_query_list_calls_out_default_order_and_paginate() {
        assert_rich_hover_contains(
            "query.list",
            &[
                "**`query.list`**",
                "order created_at desc",
                "paginate",
                "search",
                "cache",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_query_lookup_documents_single_key_and_composite_forms() {
        assert_rich_hover_contains(
            "query.lookup",
            &[
                "**`query.lookup`**",
                "by <field>: <Type>",
                "params",
                "key",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_query_sql_requires_returns_and_sql_path() {
        assert_rich_hover_contains(
            "query.sql",
            &[
                "**`query.sql`**",
                "**Required children**",
                "returns",
                "sql \"./queries",
                "record",
                "**Example**",
                "docs/invariants.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_query_view_requires_returns_and_file_source() {
        assert_rich_hover_contains(
            "query.view",
            &[
                "**`query.view`**",
                "**Required children**",
                "returns list of <Record>",
                "source @file.",
                "params",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_api_lists_method_path_output_policy_handler() {
        assert_rich_hover_contains(
            "api",
            &[
                "**`api`**",
                "method <GET|POST|PUT|PATCH|DELETE>",
                "path \"<url>\"",
                "output",
                "policy @policy.",
                "handler",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_policy_documents_forms_and_predicate_combinators() {
        assert_rich_hover_contains(
            "policy",
            &[
                "**`policy`**",
                "@policy.<name>",
                "@role.",
                "@scope.",
                "@actor.",
                "policies",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_effect_lists_closed_catalog_of_four_verbs() {
        assert_rich_hover_contains(
            "effect",
            &[
                "**`effect`**",
                "creates",
                "updates",
                "deletes",
                "returns",
                "One mutating effect per command",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_audit_lists_three_forms() {
        assert_rich_hover_contains(
            "audit",
            &[
                "**`audit`**",
                "`audit`",
                "audit <field>",
                "audit none",
                "emit_to",
                "**Example**",
                "docs/invariants.md",
            ],
        );
    }

    #[test]
    fn rich_hover_for_rate_limit_documents_grammar_and_axes() {
        assert_rich_hover_contains(
            "rate_limit",
            &[
                "**`rate_limit`**",
                "<N> per <window> per <axis>",
                "ip",
                "user",
                "org",
                "tenant",
                "rate_limit none",
                "**Example**",
                "docs/quickref.md",
            ],
        );
    }

    #[test]
    fn rich_hover_returns_none_for_unrelated_keywords() {
        // `domain` is a plain keyword that keeps its brief one-line
        // description; rich hover should not invent Markdown for it.
        assert!(
            rich_keyword_hover("domain").is_none(),
            "rich hover must stay scoped to LSP-extended kinds; `domain` should fall back to keyword_description"
        );
    }

    /// Helper: drive `context_aware_completions` and unwrap the
    /// returned items. Panics with a helpful message when the
    /// completion context isn't recognised so test failures point at
    /// the unrecognised path immediately.
    fn completions_at(source: &str, line: u32, character: u32) -> Vec<CompletionItem> {
        context_aware_completions(source, Position { line, character }).unwrap_or_else(|| {
            panic!("expected context-aware completion at line {line}:{character}")
        })
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn completion_inside_command_offers_effect_verbs_and_children() {
        let source = "feature customer\n  command create\n    policy @policy.create\n    \n";
        // Line 3 (0-indexed) is the indented blank line; cursor at
        // character 4 sits inside the indent.
        let items = completions_at(source, 3, 4);
        let labels = labels(&items);
        for child in [
            "creates",
            "updates",
            "deletes",
            "returns",
            "policy",
            "rate_limit",
            "audit",
            "emits",
            "invalidates",
            "input",
        ] {
            assert!(
                labels.contains(&child),
                "command completion must offer `{child}`; got {labels:?}"
            );
        }
        // Effect verbs lead the list inside `command`.
        assert_eq!(labels[..4], ["creates", "deletes", "returns", "updates"]);
    }

    #[test]
    fn completion_inside_query_list_offers_closed_catalog_children() {
        let source = "feature customer\n  query.list list\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in [
            "params", "filters", "search", "order", "paginate", "cache", "policy", "modifier",
            "scope",
        ] {
            assert!(
                labels.contains(&child),
                "query.list completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_inside_query_lookup_offers_params_and_key() {
        let source = "feature customer\n  query.lookup by_id\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in ["params", "key", "policy", "cache", "scope"] {
            assert!(
                labels.contains(&child),
                "query.lookup completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_inside_query_sql_offers_returns_sql_params() {
        let source = "feature customer\n  query.sql lifetime_value\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in ["returns", "sql", "params", "scope", "policy"] {
            assert!(
                labels.contains(&child),
                "query.sql completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_inside_query_view_offers_returns_source_params() {
        let source = "feature customer\n  query.view host_home_view\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in ["policy", "returns", "source", "params", "scope"] {
            assert!(
                labels.contains(&child),
                "query.view completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_inside_api_offers_method_path_output_policy_handler() {
        let source = "feature hello\n  api greet\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in [
            "method",
            "path",
            "output",
            "policy",
            "handler",
            "rate_limit",
            "input",
            "audit",
            "route",
        ] {
            assert!(
                labels.contains(&child),
                "api completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_inside_tenant_migration_offers_closed_body() {
        let source = "feature customer\n  tenant_migration backfill\n    \n";
        let items = completions_at(source, 2, 4);
        let labels = labels(&items);
        for child in [
            "target",
            "axis",
            "idempotency",
            "timeout",
            "retry",
            "handler",
        ] {
            assert!(
                labels.contains(&child),
                "tenant_migration completion must offer `{child}`; got {labels:?}"
            );
        }
    }

    #[test]
    fn completion_after_policy_namespace_offers_declared_categories() {
        let source = "feature customer\n  policies\n    create: @role.admin\n    read: @scope.same_org\n    update: @role.admin\n\n  command create\n    policy @policy.\n";
        // Cursor sits immediately after `@policy.` on line 7
        // (0-indexed). Compute the character position.
        let line = "    policy @policy.";
        let items = completions_at(source, 7, line.len() as u32);
        let mut labels = labels(&items);
        labels.sort();
        assert_eq!(labels, vec!["create", "read", "update"]);
    }

    #[test]
    fn completion_after_validator_namespace_offers_declared_extensions() {
        let source = "feature customer\n  extensions\n    validator verify_totp: Validator[Customer]\n    fn lifetime_value: Fn[Customer]\n    hook before_create: Hook[CreateCustomer]\n\n  command create\n    validate @validator.\n";
        let line = "    validate @validator.";
        let items = completions_at(source, 7, line.len() as u32);
        let labels = labels(&items);
        assert_eq!(labels, vec!["verify_totp"]);
    }

    #[test]
    fn completion_after_fn_namespace_offers_declared_fns() {
        let source = "feature customer\n  extensions\n    validator verify_totp: Validator[Customer]\n    fn lifetime_value: Fn[Customer]\n    hook before_create: Hook[CreateCustomer]\n\n  command create\n    let v = @fn.\n";
        let line = "    let v = @fn.";
        let items = completions_at(source, 7, line.len() as u32);
        let labels = labels(&items);
        assert_eq!(labels, vec!["lifetime_value"]);
    }

    #[test]
    fn completion_for_rate_limit_axis_offers_closed_catalog() {
        let source = "feature customer\n  command create\n    rate_limit \"30 per hour per ";
        // Cursor sits inside the open string after `per `.
        let line_text = "    rate_limit \"30 per hour per ";
        let items = completions_at(source, 2, line_text.len() as u32);
        let mut labels = labels(&items);
        labels.sort();
        let mut expected: Vec<&str> = RATE_LIMIT_AXES.to_vec();
        expected.sort();
        assert_eq!(labels, expected);
        // Each item carries an `ENUM_MEMBER` kind so VS Code and
        // Helix render the closed set as values, not keywords.
        for item in &items {
            assert_eq!(item.kind, Some(CompletionItemKind::ENUM_MEMBER));
        }
    }

    #[test]
    fn completion_falls_back_outside_known_blocks() {
        // Top-level cursor — not inside command/query/api/agent —
        // returns None so the global keyword list still surfaces.
        let source = "feature customer\n  \n";
        let result = context_aware_completions(
            source,
            Position {
                line: 1,
                character: 2,
            },
        );
        assert!(
            result.is_none(),
            "top-level / unknown context must fall back; got {result:?}"
        );
    }

    #[test]
    fn block_kind_detection_handles_nested_indent() {
        // A `command` block at indent 2 with a child line at indent
        // 4 — block_kind_at must walk back to the header.
        let source = "feature customer\n  command create\n    policy @policy.create\n    ";
        let kind = block_kind_at(
            source,
            Position {
                line: 3,
                character: 4,
            },
        );
        assert_eq!(kind, Some("command"));
    }

    #[test]
    fn block_kind_detection_distinguishes_query_kinds() {
        for (block_header, expected) in [
            ("query.list list", "query.list"),
            ("query.lookup by_id by id: ID", "query.lookup"),
            ("query.sql lifetime_value", "query.sql"),
            ("api greet", "api"),
            ("agent summarize", "agent"),
            ("command create", "command"),
        ] {
            let source = format!("feature x\n  {block_header}\n    ");
            let kind = block_kind_at(
                &source,
                Position {
                    line: 2,
                    character: 4,
                },
            );
            assert_eq!(
                kind,
                Some(expected),
                "header `{block_header}` should resolve to `{expected}` kind"
            );
        }
    }

    #[test]
    fn kind_child_completions_cover_seven_target_kinds() {
        let kinds: Vec<&str> = KIND_CHILD_COMPLETIONS.iter().map(|(k, _)| *k).collect();
        for required in [
            "command",
            "query.list",
            "query.lookup",
            "query.sql",
            "api",
            "tenant_migration",
        ] {
            assert!(
                kinds.contains(&required),
                "kind catalog must include `{required}`; got {kinds:?}"
            );
        }
    }

    #[test]
    fn effect_verbs_catalog_is_the_canonical_four() {
        let mut verbs = EFFECT_VERBS.to_vec();
        verbs.sort();
        assert_eq!(verbs, vec!["creates", "deletes", "returns", "updates"]);
    }

    // ── doctor file-local wire-up (R2.F) ─────────────────────────────────────
    //
    // Smoke tests verifying the `lazuli_doctor` sub-tree checks now fire
    // through the LSP, that the diagnostic `source` is `"lazuli-doctor"` for
    // click-through tooling, and that the doctor codes round-trip verbatim
    // (e.g. `HOOK-TARGET-001`, not a re-coded LSP version).

    fn doctor_diagnostics_with_code<'a>(
        diagnostics: &'a [Diagnostic],
        code: &str,
    ) -> Vec<&'a Diagnostic> {
        diagnostics
            .iter()
            .filter(|d| {
                matches!(
                    d.code.as_ref(),
                    Some(tower_lsp::lsp_types::NumberOrString::String(c)) if c == code
                )
            })
            .collect()
    }

    #[test]
    fn doctor_vocab_audit_001_surfaces_through_lsp() {
        // A write command without `audit` is the textbook VOCAB-AUDIT-001
        // trigger. `lower_feature_skeleton` lowers commands fully, so this
        // rule reliably round-trips through the LSP wire-up.
        //
        // NOTE: extension-shaped rules like HOOK-TARGET-001 cannot fire
        // here today — `lower_feature_skeleton` drops `extensions` /
        // `events` / `surfaces` / `escape_routes`. When the analyzer lifts
        // those into IR, add coverage for HOOK-TARGET-001 / VOCAB-EVENT-*.
        let source = r#"
feature widget
  purpose "Widgets"

  domain
    resource Widget

  policies
    create: @role.admin

  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Widget
"#;
        let diags = diagnostics_for(source);
        let hits = doctor_diagnostics_with_code(&diags, "VOCAB-AUDIT-001");
        assert!(
            !hits.is_empty(),
            "VOCAB-AUDIT-001 should fire through the LSP; got codes: {:?}",
            diags
                .iter()
                .filter_map(|d| d.code.as_ref())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            hits[0].source.as_deref(),
            Some("lazuli-doctor"),
            "doctor-sourced diagnostics must carry source=lazuli-doctor for click-through routing"
        );
        assert!(
            hits[0].message.contains("audit"),
            "doctor message must round-trip verbatim; got `{}`",
            hits[0].message
        );
    }

    #[test]
    fn doctor_diagnostic_source_distinguishes_from_canonical() {
        // Doctor diagnostics must use `source: "lazuli-doctor"`; existing
        // LSP shape diagnostics use `lazuli-canonical`. Both can coexist
        // in the Problems panel and be filtered.
        let source = r#"
feature widget
  purpose "Widgets"

  domain
    resource Widget

  policies
    create: @role.admin

  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Widget
"#;
        let diags = diagnostics_for(source);
        let doctor_sources: Vec<_> = diags
            .iter()
            .filter(|d| d.source.as_deref() == Some("lazuli-doctor"))
            .collect();
        assert!(
            !doctor_sources.is_empty(),
            "expected at least one source=lazuli-doctor diagnostic"
        );
    }

    #[test]
    fn doctor_clean_feature_emits_no_unexpected_doctor_diagnostics() {
        // A feature with no extensions / lifecycle / pollers / reports /
        // events and a write-command that explicitly opts out of audit
        // should not trip ANY of the wired doctor rules.
        let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

  policies
    create: @role.admin

  command create
    policy @policy.create
    creates Customer
    audit none "smoke fixture"
"#;
        let diags = diagnostics_for(source);
        let doctor_diags: Vec<_> = diags
            .iter()
            .filter(|d| d.source.as_deref() == Some("lazuli-doctor"))
            .collect();
        assert!(
            doctor_diags.is_empty(),
            "doctor-clean feature should not emit doctor diagnostics; got: {:?}",
            doctor_diags
                .iter()
                .map(|d| (d.code.clone(), d.message.clone()))
                .collect::<Vec<_>>()
        );
    }
