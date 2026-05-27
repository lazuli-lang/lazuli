//! Integration test for `lazuli inspect --format=lazuli` rendering the
//! `conventions [<bundle>, ...]` resource-level slot.
//!
//! Closes the parser gap reported by the the canonical pilot traveler
//! conventions-adoption swarm: the canonical-indent slice now lifts the
//! conventions opt-in into IR and the `--format=lazuli` text projection
//! runs Cell C4's `render_features_summary` so each opted-in resource
//! surfaces `(conventions: <bundle>)` and every synth-derived entry
//! carries the originating bundle tag.
//!
//! Spec references:
//! - `docs/proposals/ir-resource-conventions-crud.md` §11 (the C4
//!   `lazuli inspect features` summary shape).
//! - `docs/proposals/ir-resource-conventions-me.md` §8 (the §8 Customer
//!   `(conventions: me)` rendering).

use std::path::PathBuf;
use std::process::Command;

fn cli_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lazuli"))
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("conventions")
}

#[test]
fn inspect_format_lazuli_renders_crud_conventions_annotation() {
    // Customer fixture opts the resource into `conventions [crud]`.
    // The lazuli-format projection must annotate the resource header
    // with `(conventions: crud)` and tag every synth-derived
    // command/query with `[conv:crud]`.
    let fixture = fixtures_dir().join("customer.lzi");
    let output = Command::new(cli_bin())
        .args([
            "inspect",
            "--format",
            "lazuli",
            fixture.to_str().expect("fixture path is utf-8"),
        ])
        .output()
        .expect("run lazuli inspect");

    assert!(
        output.status.success(),
        "exit: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Resource annotation — the C4 §11 surface.
    assert!(
        stdout.contains("Customer (conventions: crud)"),
        "expected resource header to carry `(conventions: crud)` annotation; got:\n{stdout}"
    );

    // Synth-derived entries carry the originating bundle tag. The five
    // canonical CRUD shapes must all appear with `[conv:crud]`.
    for synth in [
        "create_customer",
        "update_customer",
        "delete_customer",
        "lookup_customer",
        "list_customers",
    ] {
        assert!(
            stdout.contains(synth) && stdout.contains("[conv:crud]"),
            "expected synth entry `{synth}` with `[conv:crud]` tag in:\n{stdout}"
        );
    }

    // The output must not be a raw source echo — the renderer adds the
    // `resources:` / `commands:` / `queries:` section headers (Cell C4
    // §11). A regression to the legacy `print!("{source}")` path-mode
    // behaviour would skip these section labels.
    assert!(
        stdout.contains("resources:")
            && stdout.contains("commands:")
            && stdout.contains("queries:"),
        "expected features-summary section headers (`resources:` / `commands:` / `queries:`); got:\n{stdout}"
    );
}
