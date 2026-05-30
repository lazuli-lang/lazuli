//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

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
            include_str!("../../../../examples/full-capsule/app.lzi"),
        ),
        (
            "full-capsule-registry.lzi",
            include_str!("../../../../examples/full-capsule/registry.lzi"),
        ),
        (
            "full-capsule-profiles.lzi",
            include_str!("../../../../examples/full-capsule/profiles.lzi"),
        ),
        (
            "full-capsule-workspace.lzi",
            include_str!("../../../../examples/full-capsule/workspace.lzi"),
        ),
        (
            "full-capsule-contract-ai.lzi",
            include_str!("../../../../examples/full-capsule/contracts/ai.lzi"),
        ),
        (
            "audit-log.lzi",
            include_str!("../../../../examples/audit-log.lzi"),
        ),
        (
            "billing.lzi",
            include_str!("../../../../examples/billing.lzi"),
        ),
        (
            "comment.lzi",
            include_str!("../../../../examples/comment.lzi"),
        ),
        (
            "customer-capsule.lzi",
            include_str!("../../../../examples/customer-capsule.lzi"),
        ),
        (
            "extension-points.lzi",
            include_str!("../../../../examples/extension-points.lzi"),
        ),
        (
            "field-permissions.lzi",
            include_str!("../../../../examples/field-permissions.lzi"),
        ),
        (
            "full-capsule.lzi",
            include_str!("../../../../examples/full-capsule/full-capsule.lzi"),
        ),
        (
            "import-csv.lzi",
            include_str!("../../../../examples/import-csv.lzi"),
        ),
        (
            "linear-issue.lzi",
            include_str!("../../../../examples/linear-issue.lzi"),
        ),
        (
            "notification.lzi",
            include_str!("../../../../examples/notification.lzi"),
        ),
        (
            "org-team.lzi",
            include_str!("../../../../examples/org-team.lzi"),
        ),
        (
            "user-auth.lzi",
            include_str!("../../../../examples/user-auth.lzi"),
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
            include_str!("../../../../examples/customer-capsule.lzx"),
        ),
        (
            "customer-capsule.web.lzx",
            include_str!("../../../../examples/customer-capsule.web.lzx"),
        ),
        (
            "full-capsule.lzx",
            include_str!("../../../../examples/full-capsule/full-capsule.lzx"),
        ),
        (
            "full-capsule.admin.web.lzx",
            include_str!("../../../../examples/full-capsule/full-capsule.admin.web.lzx"),
        ),
        (
            "full-capsule.public.web.lzx",
            include_str!("../../../../examples/full-capsule/full-capsule.public.web.lzx"),
        ),
        (
            "full-capsule.account.web.lzx",
            include_str!("../../../../examples/full-capsule/full-capsule.account.web.lzx"),
        ),
        (
            "full-capsule.sales.mobile.lzx",
            include_str!("../../../../examples/full-capsule/full-capsule.sales.mobile.lzx"),
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
