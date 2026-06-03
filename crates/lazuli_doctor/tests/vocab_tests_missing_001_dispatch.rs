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

// ── Bug A — command `tests {}` lowering reaches the rule ─────────────────────────
//
// Before the fix `lower_command_decl` hardcoded `tests: None`, so an
// authored command `tests { allows as @role.X }` was dropped and never
// reached `ir::Command.tests`. These tests parse + lower inline source
// (no fixture file) so the `# doctor:allow` comment escape hatch can
// never mask the result.

fn lower(source: &str) -> lazuli_ir::Feature {
    let skeletons =
        lazuli_syntax::parse_feature_skeletons(source).expect("parse feature skeletons");
    lazuli_analyzer::lower_feature_skeleton(&skeletons[0]).expect("lower feature")
}

#[test]
fn command_tests_lower_to_actor_assertions() {
    let source = r#"
feature billing
  resource Invoice
    number: Text required
  command void_invoice
    policy @policy.admin
    tests
      allows as @role.admin
      denies as @role.guest
"#;
    let feature = lower(source);
    let cmd = feature
        .commands
        .iter()
        .find(|c| c.name == "void_invoice")
        .expect("void_invoice command");
    let block = cmd
        .tests
        .as_ref()
        .expect("command tests lower to a substantive TestBlock (Bug A)");
    assert_eq!(
        block.assertions,
        vec![
            lazuli_ir::TestAssertion::AllowsAs {
                actor: "@role.admin".to_owned()
            },
            lazuli_ir::TestAssertion::DeniesAs {
                actor: "@role.guest".to_owned()
            },
        ]
    );
}

#[test]
fn feature_with_command_test_block_is_silent() {
    let source = r#"
feature billing
  resource Invoice
    number: Text required
  command void_invoice
    policy @policy.admin
    tests
      allows as @role.admin
"#;
    // A command with a substantive tests block silences the rule. Use a
    // guaranteed-nonexistent path so the `# doctor:allow` escape hatch
    // cannot mask the result.
    let findings = vocab_tests_missing_001::check(
        &lower(source),
        Path::new("definitely_nonexistent_buga_fixture.lzi"),
    );
    assert!(
        findings.is_empty(),
        "a command with a substantive tests block should silence VOCAB-TESTS-MISSING-001: {findings:?}"
    );
}

#[test]
fn spec_actor_matrix_credits_command_allows_as() {
    let source = r#"
feature billing
  policies
    admin: @role.admin
  resource Invoice
    number: Text required
  command void_invoice
    policy @policy.admin
    tests
      allows as @role.admin
"#;
    let feature = lower(source);
    let layer = lazuli_doctor::coverage::spec_actor_matrix::compute(std::slice::from_ref(&feature));
    assert_eq!(layer.total, 1, "one (command, @role.admin) pair");
    assert_eq!(
        layer.covered, 1,
        "the command's `allows as @role.admin` test must credit the pair"
    );
}

// ── Pilot shape: `when`-predicate-only command tests silence the rule ─────────────
//
// The dominant authored inline-test form in BOTH pilots is the
// `allows when <pred>` / `denies when <pred>` pair (pauta: 99 of each;
// hostpoint: 7+8). Their typed closed-`Predicate` parser is not yet
// wired, so they lower to the sanctioned `TestAssertion::Raw` fallback
// (non-empty) instead of being dropped — see
// `lazuli_analyzer::test_lowering`. This is the exact pilot shape that
// the legitimate `@doctor.allow(VOCAB-TESTS-MISSING-001, ...)` waivers
// were written for ("Command `tests` blocks are captured as raw lines
// and do not lower to TestBlock assertions"); those waivers are now
// retireable. This test locks the lowering→recognition contract end-to-
// end (parse → lower → check) so the waiver class cannot silently
// re-open.

#[test]
fn feature_with_when_only_command_tests_is_silent() {
    // Mirrors `agency.lzi` / `customer_management.lzi`: the command's
    // only coverage is `allows when` / `denies when` predicate lines.
    let source = r#"
feature agency
  resource Agency
    legal_name: Text required
  command create_agency
    input
      legal_name: Text required
    policy @policy.author
    creates Agency
      legal_name = input.legal_name
    tests
      allows when input.legal_name == "Acme Ads Ltda"
      denies when input.legal_name == ""
"#;
    let feature = lower(source);
    let cmd = feature
        .commands
        .iter()
        .find(|c| c.name == "create_agency")
        .expect("create_agency command");
    let block = cmd
        .tests
        .as_ref()
        .expect("`when`-predicate command tests lower to a substantive TestBlock (Raw fallback)");
    assert_eq!(
        block.assertions,
        vec![
            lazuli_ir::TestAssertion::Raw {
                line: "allows when input.legal_name == \"Acme Ads Ltda\"".to_owned()
            },
            lazuli_ir::TestAssertion::Raw {
                line: "denies when input.legal_name == \"\"".to_owned()
            },
        ],
        "authored `when` lines must lower to non-empty Raw assertions, not be dropped"
    );
    // End-to-end: the rule sees the lowered block and stays silent. A
    // guaranteed-nonexistent path keeps the `# doctor:allow` escape hatch
    // from masking the result — the silence is from real lowering.
    let findings = vocab_tests_missing_001::check(
        &feature,
        Path::new("definitely_nonexistent_when_only_fixture.lzi"),
    );
    assert!(
        findings.is_empty(),
        "a feature whose only inline tests are `when`-predicate lines must NOT fire \
         VOCAB-TESTS-MISSING-001 (the pilot waiver class is retireable): {findings:?}"
    );
}

#[test]
fn feature_with_no_tests_still_fires() {
    // Negative control for the gate: a feature with a resource + command
    // but ZERO inline `tests` blocks still fires. This is the
    // `account.lzi` / `supplier.lzi` shape whose waivers must stay.
    let source = r#"
feature supplier
  resource Supplier
    name: Text required
  command create_supplier
    input
      name: Text required
    policy @policy.author
    creates Supplier
      name = input.name
"#;
    let findings = vocab_tests_missing_001::check(
        &lower(source),
        Path::new("definitely_nonexistent_no_tests_fixture.lzi"),
    );
    assert_eq!(
        findings.len(),
        1,
        "a feature with subjects but no inline tests must still fire: {findings:?}"
    );
    assert_eq!(findings[0].feature, "supplier");
}
