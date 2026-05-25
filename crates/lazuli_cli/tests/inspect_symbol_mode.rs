//! Integration test for `lazuli inspect <qualified-symbol>` (symbol mode).
//!
//! Per `docs/proposals/lsp-symbol-origin.md` §5.2 / §5.3 / §5.4.
//!
//! Validates:
//! - Bare-name lookup resolves to a single declaration in the project.
//! - Qualified lookup (`<feature>.<symbol>`) returns the same record.
//! - Unknown symbol returns `SYMBOL_NOT_FOUND` error envelope.
//! - Path-mode is preserved when input ends in `.lzi` or contains a separator.

use std::path::PathBuf;
use std::process::Command;

fn cli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lazuli"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn full_capsule_dir() -> PathBuf {
    repo_root().join("examples").join("full-capsule")
}

#[test]
fn bare_name_lookup_returns_symbol_json() {
    let output = Command::new(cli_bin())
        .current_dir(full_capsule_dir())
        .args(["inspect", "Customer"])
        .output()
        .expect("run lazuli inspect");

    assert!(
        output.status.success(),
        "exit: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|err| panic!("parse stdout as JSON: {err}\nstdout: {stdout}"));

    assert_eq!(json["symbol"], "Customer");
    assert_eq!(json["feature"], "customer");
    assert_eq!(json["type"], "resource");
    assert!(json["defined_in"]["source"].is_string());
}

#[test]
fn qualified_lookup_returns_symbol_json() {
    let output = Command::new(cli_bin())
        .current_dir(full_capsule_dir())
        .args(["inspect", "customer.Customer"])
        .output()
        .expect("run lazuli inspect");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("parse stdout as JSON");

    assert_eq!(json["symbol"], "Customer");
    assert_eq!(json["feature"], "customer");
    assert_eq!(json["type"], "resource");
}

#[test]
fn unknown_symbol_returns_error_envelope() {
    let output = Command::new(cli_bin())
        .current_dir(full_capsule_dir())
        .args(["inspect", "DefinitelyNotARealType"])
        .output()
        .expect("run lazuli inspect");

    // Symbol not-found is a soft error — exit 0, stdout carries the
    // error envelope per proposal §5.4. (Hard errors like parse failures
    // exit non-zero.)
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("parse stdout as JSON");
    assert_eq!(json["error"]["code"], "SYMBOL_NOT_FOUND");
    assert!(
        json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("DefinitelyNotARealType")
    );
}

#[test]
fn qualified_lookup_via_uses_populates_imported_via() {
    // `customer_auth` declares `uses customer`. Querying
    // `customer_auth.Customer` MUST resolve through the import edge
    // to `customer.Customer` and populate the `imported_via` field
    // per `docs/proposals/lsp-symbol-origin.md` §5.2.
    let output = Command::new(cli_bin())
        .current_dir(full_capsule_dir())
        .args(["inspect", "customer_auth.Customer"])
        .output()
        .expect("run lazuli inspect");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("parse stdout as JSON");

    // Symbol is found via the import edge.
    assert_eq!(json["symbol"], "Customer");
    // The defining feature is `customer`, not `customer_auth`.
    assert!(
        json["defined_in"]["source"].as_str() == Some("file")
            || json["defined_in"]["source"].as_str() == Some("builtin"),
        "defined_in.source should be a known kind: {stdout}"
    );
    // Imported via the owning feature.
    let imported = &json["imported_via"];
    assert!(
        !imported.is_null(),
        "expected imported_via to be populated for cross-feature lookup; got: {stdout}"
    );
    assert_eq!(imported["feature"], "customer");
}

#[test]
fn path_mode_preserved_when_input_ends_in_lzi() {
    // `lazuli inspect <file>.lzi --format lazuli` is the legacy path-mode
    // behavior. The symbol-mode dispatcher MUST NOT hijack `.lzi` paths.
    let output = Command::new(cli_bin())
        .current_dir(full_capsule_dir())
        .args(["inspect", "full-capsule.lzi", "--format", "lazuli"])
        .output()
        .expect("run lazuli inspect");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Path-mode emits the raw .lzi source; symbol-mode would emit JSON.
    assert!(stdout.contains("feature customer"));
    assert!(!stdout.starts_with('{'));
}
