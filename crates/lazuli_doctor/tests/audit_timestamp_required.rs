//! Integration tests for `UPDATES-MISSING-UPDATED-AT-001`.
//!
//! These exercise the source parse + analyzer lower + doctor check path so
//! the rule observes the same `Feature.defaults.timestamps`,
//! `Resource.timestamps`, commands, and fields that `lazuli doctor` receives.

use std::path::Path;

use lazuli_doctor::correctness::updates_missing_updated_at::{self, Finding};

fn findings_for(source: &str) -> Vec<Finding> {
    let skeletons = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
    assert_eq!(skeletons.len(), 1, "fixture should contain one feature");
    let feature = lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature");
    updates_missing_updated_at::check(&feature, Path::new("features/billing/billing.lzi"))
}

#[test]
fn updates_resource_without_audit_timestamp_fires() {
    let findings = findings_for(
        r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Decimal required

  command update_invoice
    route id: ID
    updates Invoice
"#,
    );

    assert_eq!(findings.len(), 1, "expected one finding, got {findings:?}");
    assert_eq!(findings[0].feature, "billing");
    assert_eq!(findings[0].resource, "Invoice");
    assert_eq!(Finding::CODE, "UPDATES-MISSING-UPDATED-AT-001");
    assert!(
        findings[0]
            .message()
            .contains("resource 'billing.Invoice' is targeted by 'updates' commands"),
        "message should name the affected resource: {}",
        findings[0].message()
    );
}

#[test]
fn feature_defaults_timestamps_satisfy_contract() {
    let findings = findings_for(
        r#"
feature billing
  defaults
    timestamps
  domain
    resource Invoice
      id: ID required
      amount: Decimal required

  command update_invoice
    route id: ID
    updates Invoice
"#,
    );

    assert!(
        findings.is_empty(),
        "expected no findings, got {findings:?}"
    );
}

#[test]
fn explicit_updated_at_datetime_satisfies_contract() {
    let findings = findings_for(
        r#"
feature billing
  domain
    resource Invoice
      id: ID required
      amount: Decimal required
      updated_at: DateTime required

  command update_invoice
    route id: ID
    updates Invoice
"#,
    );

    assert!(
        findings.is_empty(),
        "expected no findings, got {findings:?}"
    );
}
