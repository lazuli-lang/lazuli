//! Integration tests for `VOCAB-TESTS-MISSING-001`.
//!
//! Wave 0 — the rule has shipped a unit test since 2026-05-15 but was
//! never invoked from `DoctorPackage::diagnostics()` (Issue Zero of
//! `docs/proposals/tdd-bdd-first-2026-05-23.md`). These tests exercise
//! the source-parse + analyzer-lower + check path that the CLI
//! dispatcher now follows, so the rule observes the same `Feature`
//! shape that `lazuli doctor` will receive in the real pipeline.

use std::path::{Path, PathBuf};

use lazuli_doctor::vocab::vocab_tests_missing_001::{self, Finding};

fn fixtures_root() -> PathBuf {
    // The doctor crate lives at `crates/lazuli_doctor/`; fixtures sit
    // at `<workspace>/examples/vocab-fixtures/`. `CARGO_MANIFEST_DIR`
    // resolves to the crate dir, so two `..` segments reach the
    // workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("vocab-fixtures")
}

fn findings_for(fixture: &str) -> Vec<Finding> {
    let path = fixtures_root().join(fixture);
    let source =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let skeletons =
        lazuli_syntax::parse_feature_skeletons(&source).expect("parse feature skeletons");
    assert_eq!(
        skeletons.len(),
        1,
        "{} should contain exactly one feature",
        fixture
    );
    let feature = lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature");
    vocab_tests_missing_001::check(&feature, &path)
}

/// Positive fixture: feature with resource + command + zero `tests`
/// blocks. The rule fires exactly once (per-feature, not per-command).
#[test]
fn positive_fixture_fires_once() {
    let findings = findings_for("vocab-tests-missing-001-positive.lzi");
    assert_eq!(
        findings.len(),
        1,
        "expected one VOCAB-TESTS-MISSING-001 finding on the positive fixture, got {findings:?}"
    );
    assert_eq!(findings[0].feature, "post");
    assert_eq!(Finding::CODE, "VOCAB-TESTS-MISSING-001");
}

/// Negative fixture: same feature shape, but the `create` command
/// carries a `tests` block. The lint is per-feature, so a single test
/// block anywhere in the feature suppresses it.
#[test]
fn negative_fixture_is_silent() {
    let findings = findings_for("vocab-tests-missing-001-negative.lzi");
    assert!(
        findings.is_empty(),
        "expected no findings on the negative fixture, got {findings:?}"
    );
}

/// Issue Zero of the tdd-bdd-first proposal — confirm the rule's
/// message mentions the planned `# doctor:allow` opt-out so authors
/// know how to override without editing the rule code.
#[test]
fn finding_message_documents_opt_out() {
    let findings = findings_for("vocab-tests-missing-001-positive.lzi");
    let message = findings[0].message();
    assert!(
        message.contains("doctor:allow"),
        "message should mention the planned `# doctor:allow` opt-out, got: {message}"
    );
}
