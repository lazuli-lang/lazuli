//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

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
