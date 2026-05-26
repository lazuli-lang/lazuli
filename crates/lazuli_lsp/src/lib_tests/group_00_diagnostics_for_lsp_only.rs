//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

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

// `diagnostics_with_code` lives in `super` (mod.rs) so all sub-modules
// share it via `use super::*` above.

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

