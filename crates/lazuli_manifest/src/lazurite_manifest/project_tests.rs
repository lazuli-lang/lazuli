//! Tests for `[project]` + `[plugins]` validation
//! (schema gate + `@lazuli/plugin-*` namespace gate).
//! Lives alongside `project.rs`.

#![cfg(test)]

use super::{Manifest, ManifestError};

fn parse_manifest(contents: &str) -> Result<Manifest, ManifestError> {
    let manifest: Manifest = toml::from_str(contents)?;
    manifest.validate()?;
    Ok(manifest)
}

#[test]
fn reject_non_plugin_namespace() {
    let err = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@runtime/foo" = { module = "github.com/acme/foo", version = "v0.1.0" }
"#,
    )
    .unwrap_err();

    assert!(matches!(err, ManifestError::InvalidPluginNamespace(key) if key == "@runtime/foo"));
}

#[test]
fn reject_unsupported_schema() {
    let err = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 2

[lazuli]
runtime = "0.1.0"
"#,
    )
    .unwrap_err();

    assert!(matches!(err, ManifestError::UnsupportedSchema(2)));
}
