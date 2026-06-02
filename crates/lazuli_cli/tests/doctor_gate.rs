//! W0-4 (META-THEME) — end-to-end proof that the blocking doctor gate is
//! wired into the default `lazuli generate` / `lazuli check` loop at a
//! FAIL-CLOSED severity.
//!
//! The audits' meta-finding: "the gates exist but aren't wired into the
//! default loop at a blocking severity." Before this gate, `lazuli generate
//! go` would happily emit a Go tree at exit 0 even when the doctor's
//! Correctness + Error rules found a real bug (an empty-bindings `creates`,
//! a handler-signature mismatch). This test pins the corrected contract:
//!
//! - (a) a fixture WITH an ERROR finding → `generate go` exits non-zero AND
//!       writes no tree (refuse-emit); `check` exits non-zero.
//! - (b) the same fixture with `--no-gate` → exits zero, prints a loud
//!       bypass notice, and DOES emit.
//! - (c) a warnings-only fixture → exits zero (warnings never block).
//! - (d) a per-finding `# doctor:allow <CODE>` waiver suppresses that
//!       finding so it no longer reaches the gate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lazuli")
}

/// Unique tempdir under the OS temp root.
fn tmp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lazuli-gate-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

const APP_LZI: &str = "app GateFix\n  uses payments\n  targets\n    backend go\n  environments\n    local\n  urls\n    api local \"http://localhost:8080\"\n  runtime\n    unit api\n      serves commands, apis\n      healthcheck \"/healthz\"\n";

/// A feature whose `creates Payment` binds no columns → fires the
/// error-severity `CREATES-EMPTY-BINDINGS-001` (a Correctness error).
const FEATURE_WITH_ERROR: &str = "feature payments\n  purpose \"gate fixture\"\n  domain\n    resource Payment\n      amount: Money required\n      currency: Currency required\n  policies\n    view: @role.admin\n    author: @actor.system\n  command capture_payment\n    input\n      amount: Money required\n      currency: Currency required\n    policy @policy.author\n    rate_limit none\n      reason \"fixture\"\n    creates Payment\n";

/// Same feature but the `creates` binds its columns → no error.
const FEATURE_CLEAN: &str = "feature payments\n  purpose \"gate fixture\"\n  domain\n    resource Payment\n      amount: Money required\n      currency: Currency required\n  policies\n    view: @role.admin\n    author: @actor.system\n  command capture_payment\n    input\n      amount: Money required\n      currency: Currency required\n    policy @policy.author\n    rate_limit none\n      reason \"fixture\"\n    creates Payment\n      amount = input.amount\n      currency = input.currency\n";

fn make_pkg(root: &Path, feature: &str) {
    write(&root.join("app.lzi"), APP_LZI);
    write(&root.join("features/payments/payments.lzi"), feature);
}

/// (a) generate go on an error fixture: non-zero exit + refuse-emit.
#[test]
fn generate_go_fails_closed_on_error_and_refuses_emit() {
    let root = tmp("gen-err");
    make_pkg(&root, FEATURE_WITH_ERROR);
    let out_dir = root.join("dist/go");

    let output = Command::new(bin())
        .args(["generate", "go"])
        .arg(&root)
        .arg("--out")
        .arg(&out_dir)
        .arg("--allow-version-mismatch")
        .output()
        .expect("run generate go");

    assert!(
        !output.status.success(),
        "generate go must FAIL on an error finding; got success"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("CREATES-EMPTY-BINDINGS-001"),
        "gate output must name the blocking error on stderr; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("doctor gate") && stderr.contains("error(s)"),
        "gate must print an error-first count banner; stderr:\n{stderr}"
    );
    assert!(
        !out_dir.exists(),
        "refuse-emit: no tree must be written when the gate blocks (found {})",
        out_dir.display()
    );
    let _ = fs::remove_dir_all(&root);
}

/// (b) --no-gate bypasses loudly and DOES emit.
#[test]
fn generate_go_no_gate_bypasses_loudly_and_emits() {
    let root = tmp("gen-bypass");
    make_pkg(&root, FEATURE_WITH_ERROR);
    let out_dir = root.join("dist/go");

    let output = Command::new(bin())
        .args(["generate", "go"])
        .arg(&root)
        .arg("--out")
        .arg(&out_dir)
        .arg("--allow-version-mismatch")
        .arg("--no-gate")
        .output()
        .expect("run generate go --no-gate");

    assert!(
        output.status.success(),
        "generate go --no-gate must succeed; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("BYPASSED"),
        "the bypass must be LOUD (a BYPASSED notice); stderr:\n{stderr}"
    );
    assert!(
        out_dir.exists(),
        "--no-gate must still emit the tree at {}",
        out_dir.display()
    );
    let _ = fs::remove_dir_all(&root);
}

/// (b') LAZULI_NO_GATE=1 env honored identically to --no-gate.
#[test]
fn generate_go_env_bypass_honored() {
    let root = tmp("gen-env");
    make_pkg(&root, FEATURE_WITH_ERROR);
    let out_dir = root.join("dist/go");

    let output = Command::new(bin())
        .args(["generate", "go"])
        .arg(&root)
        .arg("--out")
        .arg(&out_dir)
        .arg("--allow-version-mismatch")
        .env("LAZULI_NO_GATE", "1")
        .output()
        .expect("run generate go with LAZULI_NO_GATE=1");

    assert!(
        output.status.success(),
        "LAZULI_NO_GATE=1 must bypass the gate; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&root);
}

/// (c) a clean (warnings-only) fixture passes the gate and emits.
#[test]
fn generate_go_passes_on_warnings_only() {
    let root = tmp("gen-clean");
    make_pkg(&root, FEATURE_CLEAN);
    let out_dir = root.join("dist/go");

    let output = Command::new(bin())
        .args(["generate", "go"])
        .arg(&root)
        .arg("--out")
        .arg(&out_dir)
        .arg("--allow-version-mismatch")
        .output()
        .expect("run generate go (clean)");

    assert!(
        output.status.success(),
        "a warnings-only fixture must pass the gate; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out_dir.exists(), "clean run must emit the tree");
    let _ = fs::remove_dir_all(&root);
}

/// (a-check) `lazuli check` fails closed on a file-local error.
#[test]
fn check_fails_closed_on_error() {
    let root = tmp("check-err");
    // A public/mutating command with no rate_limit fires the file-local
    // `command-rate-limit` ERROR.
    let feature = "feature payments\n  domain\n    resource Payment\n      amount: Money required\n  policies\n    view: @role.admin\n    author: @actor.system\n  command capture_payment\n    input\n      amount: Money required\n    policy @policy.author\n    creates Payment\n      amount = input.amount\n";
    let target = root.join("payments.lzi");
    write(&target, feature);

    let output = Command::new(bin())
        .arg("check")
        .arg(&target)
        .args(["--security-profile", "strict", "--allow-version-mismatch"])
        .output()
        .expect("run check");
    assert!(
        !output.status.success(),
        "check must fail on an error finding; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("check:") && stderr.contains("error(s)"),
        "check must print an error-first count banner; stderr:\n{stderr}"
    );

    // --no-gate bypasses to exit 0.
    let output = Command::new(bin())
        .arg("check")
        .arg(&target)
        .args([
            "--security-profile",
            "strict",
            "--allow-version-mismatch",
            "--no-gate",
        ])
        .output()
        .expect("run check --no-gate");
    assert!(
        output.status.success(),
        "check --no-gate must exit 0; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("BYPASSED"),
        "check --no-gate must be loud"
    );
    let _ = fs::remove_dir_all(&root);
}

/// (d) a per-finding `# doctor:allow <CODE>` waiver suppresses the
/// individual finding so it no longer reaches the gate.
///
/// Uses `MIGRATION-ALTER-MISSING-001` (an error-severity Correctness rule
/// that honors `# doctor:allow`) at the `production` profile: the IR adds a
/// `color` column that the on-disk baseline migration never applies.
#[test]
fn doctor_allow_waiver_suppresses_finding_through_the_gate() {
    let root = tmp("allow");
    write(&root.join("app.lzi"), "app AllowFix\n  uses widgets\n  targets\n    backend go\n  environments\n    local\n  urls\n    api local \"http://localhost:8080\"\n  runtime\n    unit api\n      serves commands, apis\n      healthcheck \"/healthz\"\n");
    write(
        &root.join("dist/go/migrations/001_widgets_widget.sql"),
        "CREATE TABLE widgets_widget (\n  id uuid PRIMARY KEY,\n  name text NOT NULL\n);\n",
    );
    let feature_path = root.join("features/widgets/widgets.lzi");
    let feature_no_allow = "feature widgets\n  domain\n    resource Widget\n      name: Text required\n      color: Text required\n  policies\n    view: @scope.public\n";
    write(&feature_path, feature_no_allow);

    // Without the waiver: the code is present in the JSON report.
    let codes_before = doctor_codes(&root, "production");
    assert!(
        codes_before.iter().any(|c| c == "MIGRATION-ALTER-MISSING-001"),
        "fixture must fire MIGRATION-ALTER-MISSING-001 without the waiver; got: {codes_before:?}"
    );

    // With the waiver on the feature: the code is gone.
    let feature_with_allow = "feature widgets\n  # doctor:allow MIGRATION-ALTER-MISSING-001 — reason \"fixture: intentional\"\n  domain\n    resource Widget\n      name: Text required\n      color: Text required\n  policies\n    view: @scope.public\n";
    write(&feature_path, feature_with_allow);
    let codes_after = doctor_codes(&root, "production");
    assert!(
        !codes_after.iter().any(|c| c == "MIGRATION-ALTER-MISSING-001"),
        "the per-finding `# doctor:allow` must suppress the finding; got: {codes_after:?}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Run `lazuli doctor <root> --format json` and return the rule codes.
fn doctor_codes(root: &Path, profile: &str) -> Vec<String> {
    let output = Command::new(bin())
        .arg("doctor")
        .arg(root)
        .args(["--security-profile", profile, "--allow-version-mismatch", "--format", "json"])
        .output()
        .expect("run doctor --format json");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or(serde_json::Value::Null);
    json.get("findings")
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| f.get("rule").and_then(|r| r.as_str()).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
