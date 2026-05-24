//! Integration tests for `DOCTOR-OVERRIDE-NEEDS-REASON-001`.
//!
//! Wave 0.5 — the meta-rule guards `[doctor.<category>].severity_override`
//! TOML entries against missing-justification escape hatches. Without it,
//! profile-level overrides accumulate as tribal-knowledge dark matter and
//! the framework loses its enforcement loop.

use std::path::Path;

use lazuli_doctor::test_discipline::override_needs_reason_001::{self, Finding, OverrideEntry};

fn entry(code: &str, reason: Option<&str>) -> OverrideEntry {
    OverrideEntry {
        category: "test_discipline".to_owned(),
        rule_code: code.to_owned(),
        severity: "warning".to_owned(),
        reason: reason.map(|s| s.to_owned()),
    }
}

#[test]
fn missing_reason_fires() {
    let findings = override_needs_reason_001::check(
        &[entry("TEST-MISSING-AUTHORED-001", None)],
        Path::new("Lazurite.toml"),
    );
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].rule_code, "TEST-MISSING-AUTHORED-001");
    assert_eq!(findings[0].category, "test_discipline");
    assert_eq!(Finding::CODE, "DOCTOR-OVERRIDE-NEEDS-REASON-001");
    assert!(
        findings[0].message().contains("reason"),
        "message must mention the missing `reason` field, got: {}",
        findings[0].message()
    );
}

#[test]
fn blank_reason_fires() {
    let findings = override_needs_reason_001::check(
        &[entry("TEST-MISSING-AUTHORED-001", Some("   \t  "))],
        Path::new("Lazurite.toml"),
    );
    assert_eq!(
        findings.len(),
        1,
        "whitespace-only reason should not satisfy the contract"
    );
}

#[test]
fn meaningful_reason_does_not_fire() {
    let findings = override_needs_reason_001::check(
        &[entry(
            "TEST-MISSING-AUTHORED-001",
            Some("legacy billing feature; refactor scheduled Q3 2026"),
        )],
        Path::new("Lazurite.toml"),
    );
    assert!(findings.is_empty());
}

#[test]
fn mixed_overrides_emit_one_finding_per_unjustified_rule() {
    let findings = override_needs_reason_001::check(
        &[
            entry("TEST-MISSING-AUTHORED-001", None),
            entry("TEST-PREDICATE-UNCOVERED-001", Some("")),
            entry("TEST-RESTATES-EFFECT-001", Some("documented")),
        ],
        Path::new("Lazurite.toml"),
    );
    assert_eq!(findings.len(), 2);
    let codes: Vec<&str> = findings.iter().map(|f| f.rule_code.as_str()).collect();
    assert!(codes.contains(&"TEST-MISSING-AUTHORED-001"));
    assert!(codes.contains(&"TEST-PREDICATE-UNCOVERED-001"));
}
