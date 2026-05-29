//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

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
fn sessions_unknown_kind_flags_typo_with_suggestion() {
    // `cokie` (vs `cookie`) is silently dropped by the parser — the typo
    // catalog turns it into a precise suggestion.
    let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cokie
        same_site strict
"#;
    let diagnostics = diagnostics_for(source);
    let hits = diagnostics_with_code(&diagnostics, "sessions-unknown-kind");
    assert_eq!(
        hits.len(),
        1,
        "expected one sessions-unknown-kind diagnostic for `cokie`; got {} (full set: {:#?})",
        hits.len(),
        diagnostics,
    );
    assert!(hits[0].message.contains("cokie"));
    assert!(
        hits[0].message.contains("cookie"),
        "diagnostic must suggest `cookie`; got `{}`",
        hits[0].message
    );
}

#[test]
fn sessions_cookie_unknown_attribute_flags_typo_with_suggestion() {
    // `same_sight` (vs `same_site`) inside the cookie sub-block.
    let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      cookie
        same_sight strict
        secure true
"#;
    let diagnostics = diagnostics_for(source);
    let hits = diagnostics_with_code(&diagnostics, "sessions-cookie-unknown-kind");
    assert_eq!(
        hits.len(),
        1,
        "expected one sessions-cookie-unknown-kind diagnostic for `same_sight`; got {} (full set: {:#?})",
        hits.len(),
        diagnostics,
    );
    assert!(hits[0].message.contains("same_sight"));
    assert!(
        hits[0].message.contains("same_site"),
        "diagnostic must suggest `same_site`; got `{}`",
        hits[0].message
    );
}

#[test]
fn sessions_unknown_kind_silent_for_valid_body_and_cookie() {
    // A fully valid sessions block — scalars, rotation sub-block, and the
    // cookie sub-block with all six attributes — must stay quiet on both
    // the sessions-body and sessions-cookie catalogs.
    let source = r#"
feature customer_auth
  auth
    identity Customer.email

    sessions
      resource CustomerSession
      ttl "7 days"
      access_ttl "15 minutes"
      refresh true
      rotation
        refresh_ttl "30 days"
        grace "30 seconds"
        theft_detection_action revoke_session_family
      cookie
        name "lazuli_session"
        same_site strict
        secure true
        http_only false
        domain ".example.com"
        path "/app"
"#;
    let diagnostics = diagnostics_for(source);
    let body_hits = diagnostics_with_code(&diagnostics, "sessions-unknown-kind");
    let cookie_hits = diagnostics_with_code(&diagnostics, "sessions-cookie-unknown-kind");
    assert!(
        body_hits.is_empty() && cookie_hits.is_empty(),
        "valid sessions + cookie body must not fire either typo diagnostic; got body={:#?} cookie={:#?}",
        body_hits,
        cookie_hits,
    );
}

#[test]
fn canonical_order_accepts_full_capsule_fixture() {
    let diagnostics = diagnostics_for(include_str!(
        "../../../../examples/full-capsule/full-capsule.lzi"
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
        diagnostic
            .message
            .contains("feature requirements currently use `integration <name>: <CapabilityType>`")
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

