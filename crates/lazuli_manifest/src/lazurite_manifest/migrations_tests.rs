//! Tests for `[migrations]` and `[seeds]` resolution helpers.
//! Lives alongside `migrations.rs`.

#![cfg(test)]

use super::{Manifest, ManifestError, MigrationStrategy};

fn parse_manifest(contents: &str) -> Result<Manifest, ManifestError> {
    let manifest: Manifest = toml::from_str(contents)?;
    manifest.validate()?;
    Ok(manifest)
}

/// Frente 1 — `[migrations]` defaults apply when block is absent.
#[test]
fn migrations_or_default_when_block_absent() {
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

    let migrations = manifest.migrations_or_default();
    assert_eq!(migrations.generated, "dist/go/migrations");
    assert_eq!(migrations.manual, "migrations");
    assert!(matches!(migrations.strategy, MigrationStrategy::Auto));
}

/// Frente 1 — partial `[migrations]` block fills missing fields
/// with canonical defaults.
#[test]
fn migrations_partial_block_fills_defaults() {
    let manifest = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[migrations]
strategy = "manual"
"#,
    )
    .unwrap();

    let migrations = manifest.migrations_or_default();
    assert_eq!(migrations.generated, "dist/go/migrations");
    assert_eq!(migrations.manual, "migrations");
    assert!(matches!(migrations.strategy, MigrationStrategy::Manual));
}

/// Frente 1 — `[seeds]` defaults apply when block is absent.
#[test]
fn seeds_or_default_when_block_absent() {
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

    let seeds = manifest.seeds_or_default();
    assert_eq!(seeds.dir, "seeds");
    assert!(!seeds.auto);
}
