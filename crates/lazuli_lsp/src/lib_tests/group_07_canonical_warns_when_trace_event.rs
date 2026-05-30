//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

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
        diagnostic
            .message
            .contains("`payload.account_id` is not declared by event `customer.customer_created`")
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
