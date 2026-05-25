//! Integration tests for `TEST-VIEW-E2E-MISSING-001` (Wave 3.5).
//!
//! Exercises the rule end-to-end against a real `.lzx` parse, mirroring
//! how `lazuli doctor` will dispatch it once wired into the
//! `test_discipline_diagnostics()` aggregator. Lives in
//! `crates/lazuli_cli/tests/` rather than `crates/lazuli_doctor/tests/`
//! because the doctor crate's `lib test` build is currently broken on
//! `main` from an unrelated `Resource` IR change — putting tests here
//! keeps the rule observable until the doctor-side baseline is fixed.

use std::fs;
use std::path::{Path, PathBuf};

use lazuli_doctor::test_discipline::test_view_e2e_missing_001::{self, Finding};
use tempfile::TempDir;

const POSITIVE_LZX: &str = "\
experience customer
  imports customer

  view list
    source customer.query.list
    action create -> customer.command.create
    opens detail(id: row.id)

  view detail
    source customer.query.by_id

  view lead_capture
    submit customer.command.capture_lead
";

const NEGATIVE_LZX: &str = "\
experience customer
  imports customer

  view list
    source customer.query.list
";

fn parse(source: &str) -> lazuli_syntax::LzxDocument {
    lazuli_syntax::parse_lzx_document(source).expect("parse fixture lzx")
}

fn run(source: &str, project_root: &Path, source_path: &Path) -> Vec<Finding> {
    let document = parse(source);
    test_view_e2e_missing_001::check(&document, source_path, source, project_root)
}

#[test]
fn positive_fixture_fires_for_every_view_without_spec() {
    let tmp = TempDir::new().expect("tempdir");
    let project_root = tmp.path();
    let findings = run(
        POSITIVE_LZX,
        project_root,
        Path::new("examples/wave-3-5/positive.lzx"),
    );
    assert_eq!(
        findings.len(),
        3,
        "three views, no specs scaffolded → three findings: {:?}",
        findings
    );

    let view_names: Vec<&str> = findings.iter().map(|f| f.view.as_str()).collect();
    assert!(view_names.contains(&"list"));
    assert!(view_names.contains(&"detail"));
    assert!(view_names.contains(&"lead_capture"));

    let first = &findings[0];
    assert_eq!(Finding::CODE, "TEST-VIEW-E2E-MISSING-001");
    assert_eq!(first.experience, "customer");
    assert!(first.message().contains("file-pair only"));
    assert!(
        first.expected_spec.strip_prefix(project_root).is_ok(),
        "expected_spec resolves under project_root: {}",
        first.expected_spec.display()
    );
}

#[test]
fn negative_fixture_silent_when_spec_exists() {
    let tmp = TempDir::new().expect("tempdir");
    let project_root = tmp.path();
    let e2e_dir = project_root.join("e2e").join("customer");
    fs::create_dir_all(&e2e_dir).expect("create e2e dir");
    fs::write(
        e2e_dir.join("list.spec.ts"),
        "// authored by hand; do not regenerate.\n",
    )
    .expect("write list spec");

    let findings = run(
        NEGATIVE_LZX,
        project_root,
        Path::new("examples/wave-3-5/negative.lzx"),
    );
    assert!(
        findings.is_empty(),
        "spec exists → no findings, got {findings:?}"
    );
}

#[test]
fn rule_fires_only_for_missing_specs_when_some_exist() {
    let tmp = TempDir::new().expect("tempdir");
    let project_root = tmp.path();
    let e2e_dir = project_root.join("e2e").join("customer");
    fs::create_dir_all(&e2e_dir).expect("create e2e dir");
    // Scaffold only `list.spec.ts`; the other two views must still fire.
    fs::write(e2e_dir.join("list.spec.ts"), "// scaffolded\n").expect("write list spec");

    let findings = run(
        POSITIVE_LZX,
        project_root,
        Path::new("examples/wave-3-5/partial.lzx"),
    );
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().any(|f| f.view == "detail"));
    assert!(findings.iter().any(|f| f.view == "lead_capture"));
    assert!(
        findings.iter().all(|f| f.view != "list"),
        "list has a paired spec; must not fire"
    );
}

#[test]
fn expected_spec_path_uses_canonical_layout() {
    let p = test_view_e2e_missing_001::expected_spec_path(Path::new("/proj"), "customer", "detail");
    let expected: PathBuf = PathBuf::from("/proj")
        .join("e2e")
        .join("customer")
        .join("detail.spec.ts");
    assert_eq!(p, expected);
}

#[test]
fn anchor_points_at_view_line_inside_lzx() {
    let tmp = TempDir::new().expect("tempdir");
    let findings = run(
        POSITIVE_LZX,
        tmp.path(),
        Path::new("examples/wave-3-5/positive.lzx"),
    );
    let list = findings
        .iter()
        .find(|f| f.view == "list")
        .expect("list finding present");
    assert_eq!(list.line, 4, "view `list` is on line 4 of POSITIVE_LZX");
    assert!(list.column >= 1);
}
