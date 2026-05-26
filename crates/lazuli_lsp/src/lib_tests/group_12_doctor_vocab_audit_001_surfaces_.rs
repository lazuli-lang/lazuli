//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers. Tests are grouped by line-range
//! buckets only; each bucket is ≤ 500 LOC so `clippy` and
//! `rust-analyzer` stay responsive.
#![allow(unused_imports)]
use super::*;

#[test]
fn doctor_vocab_audit_001_surfaces_through_lsp() {
    // A write command without `audit` is the textbook VOCAB-AUDIT-001
    // trigger. `lower_feature_skeleton` lowers commands fully, so this
    // rule reliably round-trips through the LSP wire-up.
    //
    // NOTE: extension-shaped rules like HOOK-TARGET-001 cannot fire
    // here today — `lower_feature_skeleton` drops `extensions` /
    // `events` / `surfaces` / `escape_routes`. When the analyzer lifts
    // those into IR, add coverage for HOOK-TARGET-001 / VOCAB-EVENT-*.
    let source = r#"
feature widget
  purpose "Widgets"

  domain
    resource Widget

  policies
    create: @role.admin

  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Widget
"#;
    let diags = diagnostics_for(source);
    let hits = doctor_diagnostics_with_code(&diags, "VOCAB-AUDIT-001");
    assert!(
        !hits.is_empty(),
        "VOCAB-AUDIT-001 should fire through the LSP; got codes: {:?}",
        diags
            .iter()
            .filter_map(|d| d.code.as_ref())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hits[0].source.as_deref(),
        Some("lazuli-doctor"),
        "doctor-sourced diagnostics must carry source=lazuli-doctor for click-through routing"
    );
    assert!(
        hits[0].message.contains("audit"),
        "doctor message must round-trip verbatim; got `{}`",
        hits[0].message
    );
}

#[test]
fn doctor_diagnostic_source_distinguishes_from_canonical() {
    // Doctor diagnostics must use `source: "lazuli-doctor"`; existing
    // LSP shape diagnostics use `lazuli-canonical`. Both can coexist
    // in the Problems panel and be filtered.
    let source = r#"
feature widget
  purpose "Widgets"

  domain
    resource Widget

  policies
    create: @role.admin

  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Widget
"#;
    let diags = diagnostics_for(source);
    let doctor_sources: Vec<_> = diags
        .iter()
        .filter(|d| d.source.as_deref() == Some("lazuli-doctor"))
        .collect();
    assert!(
        !doctor_sources.is_empty(),
        "expected at least one source=lazuli-doctor diagnostic"
    );
}

#[test]
fn doctor_clean_feature_emits_no_unexpected_doctor_diagnostics() {
    // A feature with no extensions / lifecycle / pollers / reports /
    // events and a write-command that explicitly opts out of audit
    // should not trip ANY of the wired doctor rules.
    let source = r#"
feature customer
  purpose "Customers"

  domain
    resource Customer

  policies
    create: @role.admin

  command create
    policy @policy.create
    creates Customer
    audit none "smoke fixture"
"#;
    let diags = diagnostics_for(source);
    let doctor_diags: Vec<_> = diags
        .iter()
        .filter(|d| d.source.as_deref() == Some("lazuli-doctor"))
        .collect();
    assert!(
        doctor_diags.is_empty(),
        "doctor-clean feature should not emit doctor diagnostics; got: {:?}",
        doctor_diags
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
}

