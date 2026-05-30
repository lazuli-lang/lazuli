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

