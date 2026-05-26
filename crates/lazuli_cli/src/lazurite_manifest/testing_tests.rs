//! Tests for `[testing.*]` parsing + `Manifest::testing_*_resolved`
//! helpers. Covers `default_layers` fallback, `testing.ts` runner
//! defaults, and layout-derived defaults for both Vitest and
//! Playwright. Lives alongside `testing.rs`.

#![cfg(test)]

use super::{Manifest, ManifestError};

fn parse_manifest(contents: &str) -> Result<Manifest, ManifestError> {
    let manifest: Manifest = toml::from_str(contents)?;
    manifest.validate()?;
    Ok(manifest)
}

/// Frente 1 — `[testing] default_layers` defaults to
/// `["handler_go", "view_extensibility"]` when missing.
#[test]
fn testing_default_layers_when_block_absent() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
    )
    .unwrap();

    let layers = manifest.testing_default_layers();
    assert_eq!(layers, vec!["handler_go", "view_extensibility"]);
}

/// Frente 1 — authored `default_layers` wins over the canonical
/// default.
#[test]
fn testing_default_layers_authored_wins() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[testing]
default_layers = ["spec_predicate"]
"#,
    )
    .unwrap();

    let layers = manifest.testing_default_layers();
    assert_eq!(layers, vec!["spec_predicate"]);
}

/// Frente 1 — `[testing.ts]` runner defaults to `"vitest"`.
#[test]
fn testing_ts_runner_defaults_to_vitest() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[testing.ts]
"#,
    )
    .unwrap();

    let ts = manifest
        .testing
        .as_ref()
        .and_then(|t| t.ts.as_ref())
        .expect("ts block");
    assert_eq!(ts.runner, "vitest");
}

/// Frente 1 — `testing_ts_resolved` returns layout-derived
/// defaults when block is omitted.
#[test]
fn testing_ts_resolved_layout_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("app").join("web")).unwrap();
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
    )
    .unwrap();
    let ts = manifest.testing_ts_resolved(tmp.path()).expect("resolved");
    assert_eq!(ts.runner, "vitest");
    assert_eq!(ts.config.as_deref(), Some("app/web/vite.config.ts"));
    assert_eq!(ts.discovery_root.as_deref(), Some("app/web/src"));
}

/// Frente 1 — authored fields win over layout defaults; missing
/// fields are filled.
#[test]
fn testing_ts_resolved_authored_wins_with_layout_fill() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("app").join("web")).unwrap();
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[testing.ts]
discovery_root = "custom/src"
"#,
    )
    .unwrap();
    let ts = manifest.testing_ts_resolved(tmp.path()).expect("resolved");
    // discovery_root authored wins.
    assert_eq!(ts.discovery_root.as_deref(), Some("custom/src"));
    // config filled from layout.
    assert_eq!(ts.config.as_deref(), Some("app/web/vite.config.ts"));
}

/// Frente 1 — no layout AND no authored block → None
/// (back-compat skip path for non-canonical projects).
#[test]
fn testing_ts_resolved_none_when_neither() {
    let tmp = tempfile::tempdir().unwrap();
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
    )
    .unwrap();
    assert!(manifest.testing_ts_resolved(tmp.path()).is_none());
}

/// Frente 1 — `testing_playwright_resolved` mirrors the ts
/// behavior with playwright-specific paths.
#[test]
fn testing_playwright_resolved_layout_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("app").join("web")).unwrap();
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
    )
    .unwrap();
    let pw = manifest
        .testing_playwright_resolved(tmp.path())
        .expect("resolved");
    assert_eq!(pw.config.as_deref(), Some("app/web/playwright.config.ts"));
    assert_eq!(pw.discovery_root.as_deref(), Some("app/web/e2e"));
    assert_eq!(pw.workers, Some(4));
}
