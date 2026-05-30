//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

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
        diagnostic
            .message
            .contains("`@policy.*` references should resolve to a feature-local policy category")
    }));
}

#[test]
fn canonical_uniform_inline_atom_on_command_is_clean() {
    // SPEC-07 — the command/workflow-only `@policy.*` asymmetry is GONE:
    // a namespaced inline atom (`@role.admin`) is uniformly accepted on
    // EVERY callable, exactly as it always was on jobs/webhooks/
    // escape_routes. The old per-construct warning must no longer fire.
    let source = r#"
feature customer
  purpose "Customers"

  command create
    policy @role.admin
    creates Customer
"#;

    let diagnostics = diagnostics_for(source);

    assert!(
        !diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("commands and workflows should reference feature-local policy categories")
        }),
        "the retired command/workflow-only @policy asymmetry must not fire"
    );
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
        messages
            .iter()
            .any(|message| { message.contains("webhooks must declare `idempotency by payload.") })
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
