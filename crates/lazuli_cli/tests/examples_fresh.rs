//! FRESHNESS GATE — every example meant to be valid must still parse + check
//! against the *current* language.
//!
//! This exists because `examples/` silently rotted: `billing.lzi`,
//! `field-permissions.lzi`, and `event-outbox-smoke.lzi` kept retired syntax
//! (path-based `verify`, `query.lookup key`, `rate_limit none`) long after the
//! grammar moved on, and nothing caught it — only `examples/curated/` was
//! gated (by `lazuli-dev examples validate`), leaving the other ~80 example
//! files unguarded. This test closes that gap: it runs `lazuli check
//! --security-profile prototype` on every `.lzi` under `examples/` and fails
//! if any in-scope example no longer checks clean.
//!
//! ## Keeping exclusions honest
//!
//! [`EXCLUDED_DIRS`] is the ONLY escape hatch, and every entry carries a
//! documented reason. Keep it tight: a silent exclusion is exactly the blind
//! spot that let the drift above happen. An example that is *intentionally*
//! invalid (an anti-pattern fixture, a fail-compare fixture) or *aspirational*
//! (uses surface that has not shipped yet) belongs to a directory listed here;
//! everything else MUST stay current.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Path fragments whose `.lzi` files are deliberately NOT held to the
/// "checks clean against current syntax" bar. Each entry needs a reason.
const EXCLUDED_DIRS: &[&str] = &[
    // Anti-pattern fixtures demonstrate authoring mistakes; they exist to be
    // *flagged* by the doctor, not to model clean code.
    "anti-patterns",
    // `*-fail-compare` fixtures are intentional negative cases for the
    // money/compare smoke checks.
    "fail-compare",
    // Aspirational production fixture: its `GAPS.md` documents surface that has
    // not shipped (e.g. the `subscription` app block, RBAC catalog). It is a
    // roadmap artifact, not a teaching example, and is expected to fail check
    // until the gaps land.
    "production-grade",
];

/// Workspace root, derived from this crate's manifest dir (`crates/lazuli_cli`).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/lazuli_cli has a workspace root two levels up")
        .to_path_buf()
}

/// Recursively collect every `.lzi` file under `dir`.
fn collect_lzi(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_lzi(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("lzi") {
            out.push(path);
        }
    }
}

fn is_excluded(path: &Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/");
    EXCLUDED_DIRS.iter().any(|frag| s.contains(frag))
}

#[test]
fn every_valid_example_checks_clean() {
    let root = workspace_root();
    let examples = root.join("examples");
    assert!(
        examples.is_dir(),
        "examples/ not found at {}",
        examples.display()
    );

    let mut lzi = Vec::new();
    collect_lzi(&examples, &mut lzi);
    lzi.sort();
    let in_scope: Vec<_> = lzi.iter().filter(|p| !is_excluded(p)).collect();
    assert!(
        in_scope.len() >= 30,
        "expected to find the example corpus; only {} in-scope .lzi found — \
         did the examples/ layout move?",
        in_scope.len()
    );

    let bin = env!("CARGO_BIN_EXE_lazuli");
    let mut failures = Vec::new();
    for path in in_scope {
        let output = Command::new(bin)
            .arg("check")
            .arg(path)
            .arg("--security-profile")
            .arg("prototype")
            // The example may live under a project whose Lazurite.toml pins a
            // version; freshness, not version-pinning, is what this gate tests.
            .arg("--allow-version-mismatch")
            .output()
            .expect("failed to spawn lazuli check");
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let first_err = stderr
                .lines()
                .find(|l| l.contains("error"))
                .unwrap_or_else(|| stderr.lines().next().unwrap_or(""));
            let rel = path.strip_prefix(&root).unwrap_or(path);
            failures.push(format!("  {} :: {}", rel.display(), first_err.trim()));
        }
    }

    assert!(
        failures.is_empty(),
        "{} example(s) no longer check clean against current syntax — fix them to \
         the current grammar (consult docs/keyword-reference.md + a passing example), \
         or, if intentionally invalid, add the directory to EXCLUDED_DIRS with a reason:\n{}",
        failures.len(),
        failures.join("\n"),
    );
}
