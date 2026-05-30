//! Integration tests for `@correctness.updates_missing_updated_at`
//! (cell RU3, codegen-correctness-cycle-2-2026-05-21).
//!
//! Three scenarios per the cell spec, exercised against the public
//! `check` API to keep the contract honest at the crate boundary:
//! 1. Resource WITH `updated_at: DateTime` + `Updates` command → no diagnostic.
//! 2. Resource WITHOUT `updated_at` + `Updates` command (timestamps opted
//!    out) → warning fires with the right resource name.
//! 3. Resource WITHOUT `updated_at` but NO `Updates` command → no diagnostic.

use std::path::Path;

use lazuli_doctor::correctness::updates_missing_updated_at::{self, Finding};
use lazuli_ir::Feature;

fn lower(source: &str) -> Feature {
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parse feature");
    lazuli_analyzer::lower_feature_skeleton(&features[0]).expect("lower feature")
}

#[test]
fn resource_with_updated_at_datetime_no_diagnostic() {
    let feature = lower(
        r#"
feature billing
  domain
    resource Customer
      id: ID required
      updated_at: DateTime required

  command update_customer
    route id: ID
    updates Customer
"#,
    );

    let findings = updates_missing_updated_at::check(&feature, Path::new("billing.lzi"));
    assert!(
        findings.is_empty(),
        "expected no diagnostic when updated_at: DateTime is declared, got {findings:?}"
    );
}

#[test]
fn resource_without_updated_at_warning_fires_with_resource_name() {
    let feature = lower(
        r#"
feature billing
  domain
    resource Customer
      id: ID required

  command update_customer
    route id: ID
    updates Customer
"#,
    );

    let findings = updates_missing_updated_at::check(&feature, Path::new("billing.lzi"));
    assert_eq!(
        findings.len(),
        1,
        "expected one finding when updates_at is missing, got {findings:?}"
    );
    let f = &findings[0];
    assert_eq!(f.feature, "billing");
    assert_eq!(f.resource, "Customer");
    assert_eq!(Finding::CODE, "UPDATES-MISSING-UPDATED-AT-001");
    assert_eq!(Finding::ID, "@correctness.updates_missing_updated_at");
    let msg = f.message();
    assert!(
        msg.contains("'billing.Customer'"),
        "message must anchor on feature.resource: {msg}"
    );
    assert!(
        msg.contains("'updates' commands"),
        "message must mention the updates effect: {msg}"
    );
    assert!(
        msg.contains("updated_at: DateTime required"),
        "message must hint canonical field shape: {msg}"
    );
    assert!(
        msg.contains("auto-stamps"),
        "message must reassure that the framework writes the column: {msg}"
    );
}

#[test]
fn resource_without_updated_at_but_no_updates_command_stays_silent() {
    let feature = lower(
        r#"
feature billing
  domain
    resource Customer
      id: ID required

  command create_customer
    creates Customer
"#,
    );

    let findings = updates_missing_updated_at::check(&feature, Path::new("billing.lzi"));
    assert!(
        findings.is_empty(),
        "no Updates effect means the diagnostic must stay silent, got {findings:?}"
    );
}
