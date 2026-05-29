//! Tests for `[generate.go]` resolution helpers — defaults-when-absent
//! and partial-block-fills-defaults. Lives next to `generate.rs`.

#![cfg(test)]

use super::{Manifest, ManifestError};

fn parse_manifest(contents: &str) -> Result<Manifest, ManifestError> {
    let manifest: Manifest = toml::from_str(contents)?;
    manifest.validate()?;
    Ok(manifest)
}

/// Frente 1 — `[generate.go]` defaults apply when block is absent.
#[test]
fn generate_go_or_default_when_block_absent() {
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

    let go = manifest.generate_go_or_default();
    assert_eq!(go.out, "dist/go");
    assert!(go.gofmt);
    assert!(go.strict);
    assert!(go.emit_main);
    assert!(go.submodule);
}

/// Frente 1 — partial `[generate.go]` block fills missing fields
/// with canonical defaults.
#[test]
fn generate_go_partial_block_fills_defaults() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[generate.go]
out = "build/server"
"#,
    )
    .unwrap();

    let go = manifest.generate_go_or_default();
    assert_eq!(go.out, "build/server");
    // Other fields default to the canonical values.
    assert!(go.gofmt);
    assert!(go.strict);
    assert!(go.emit_main);
    assert!(go.submodule);
}
