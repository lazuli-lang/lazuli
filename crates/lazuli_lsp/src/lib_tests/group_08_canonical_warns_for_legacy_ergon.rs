//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

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
        messages
            .iter()
            .any(|message| { message.contains("query declarations should use an explicit mode") })
    );
    assert!(
        messages
            .iter()
            .any(|message| { message.contains("policy atoms should be namespaced") })
    );
    // SPEC-04 — the `type-namespace` warning on bare semantic types is retired:
    // bare PascalCase is now canonical and `lazuli fmt` normalizes the @-form,
    // so there is no longer a warning to assert here.
    assert!(messages.iter().any(|message| {
        message.contains("extension references should use capability namespaces")
    }));
    assert!(
        messages.iter().any(|message| {
            message.contains("`idempotency` should declare its source with `by`")
        })
    );
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
    let source = include_str!("../../../../examples/full-capsule/full-capsule.lzi");
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
        messages
            .iter()
            .any(|message| { message.contains("`paginate` should declare a positive integer") })
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
