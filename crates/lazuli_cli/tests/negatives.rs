//! NEGATIVE FIXTURE GATE — the negative twin of `examples_fresh.rs`.
//!
//! `examples_fresh.rs` proves every *valid* example still checks clean and
//! deliberately EXCLUDES the intentional-negative fixtures. But exclusion
//! is not assertion: an excluded negative fixture can silently start
//! passing (the very bug it guards regresses to "no longer detected") and
//! nothing notices, because the freshness gate skips it.
//!
//! This test pins each intentional-negative fixture to the EXACT diagnostic
//! code it is supposed to fire (overnight-2026-06-02/07-test-coverage
//! §6, cell NEG-FIXTURE-PIN-001 — and the W3-2 directive to strengthen
//! negative fixtures from "an error occurs" to "this specific code fires").
//!
//! Each case asserts the canonical code is PRESENT in the JSON report (a
//! subset assertion — environment-dependent codes like LAZULI-VERSION-*
//! are intentionally not pinned, only the rule the fixture exists to
//! guard). The codes were ground-truthed by running the CLI at this
//! commit; if a fixture stops firing its code, this test fails loudly
//! rather than the regression merging clean.

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/lazuli_cli has a workspace root two levels up")
        .to_path_buf()
}

/// Run `lazuli <subcommand> <target> --format json` and return the set of
/// diagnostic `rule` codes (any severity) in the report.
fn diagnostic_codes(subcommand: &str, target: &Path) -> Vec<String> {
    let bin = env!("CARGO_BIN_EXE_lazuli");
    let output = Command::new(bin)
        .arg(subcommand)
        .arg(target)
        .arg("--security-profile")
        .arg("prototype")
        .arg("--allow-version-mismatch")
        .arg("--format")
        .arg("json")
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn lazuli {subcommand}: {e}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_rule_codes(&stdout)
}

/// Minimal JSON walk that pulls every `"rule":"..."` value out of the
/// doctor `findings` array without taking a serde_json dev-dependency on
/// this integration test. The DoctorReport finding shape is
/// `{"rule":"CODE", "severity":..., ...}`.
fn parse_rule_codes(json: &str) -> Vec<String> {
    let mut codes = Vec::new();
    let needle = "\"rule\":";
    let mut rest = json;
    while let Some(idx) = rest.find(needle) {
        rest = &rest[idx + needle.len()..];
        let rest_trimmed = rest.trim_start();
        if let Some(after_quote) = rest_trimmed.strip_prefix('"') {
            if let Some(end) = after_quote.find('"') {
                codes.push(after_quote[..end].to_owned());
                rest = &after_quote[end + 1..];
                continue;
            }
        }
    }
    codes
}

fn assert_fires(subcommand: &str, rel: &str, expected_code: &str) {
    let root = workspace_root();
    let target = root.join(rel);
    assert!(
        target.exists(),
        "negative fixture missing at {} — did the examples/ layout move?",
        target.display()
    );
    let codes = diagnostic_codes(subcommand, &target);
    assert!(
        codes.iter().any(|c| c == expected_code),
        "intentional-negative fixture `{rel}` no longer fires `{expected_code}` via `lazuli {subcommand}` \
         — the rule it guards may have regressed. Observed codes: {codes:?}"
    );
}

#[test]
fn anti_patterns_fires_runtime_reachable_stub() {
    // The polymorphic `target.<field>` source lowers to a 501 runtime stub;
    // the doctor must flag it so it never ships green.
    assert_fires(
        "doctor",
        "examples/anti-patterns/comment-polymorphic-target.lzi",
        "RUNTIME-REACHABLE-STUB-001",
    );
}

#[test]
fn anti_patterns_fires_cross_feature_type_unresolved() {
    // The fixture references an undeclared `User` type across features.
    assert_fires(
        "doctor",
        "examples/anti-patterns/comment-polymorphic-target.lzi",
        "cross_feature_type_unresolved",
    );
}

#[test]
fn money_smoke_fail_compare_fires_money_compare() {
    // Direct money comparison without the safe-compare vocab — the whole
    // reason this `*-fail-compare` fixture exists.
    assert_fires(
        "doctor",
        "examples/money-smoke-fail-compare",
        "MONEY-COMPARE-001",
    );
}

#[test]
fn production_grade_fires_app_unknown_kind() {
    // The deliberate `subscription` app block kind that has not shipped.
    assert_fires("doctor", "examples/production-grade", "app-unknown-kind");
}
