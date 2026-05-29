//! Tests for `[frontends.<name>]` parsing + validation
//! (`reject_unknown_frontend_target`, `reject_frontend_out_collision`).
//! Lives alongside `frontends.rs`.

#![cfg(test)]

use super::{FrontendTarget, Manifest, ManifestError};

fn parse_manifest(contents: &str) -> Result<Manifest, ManifestError> {
    let manifest: Manifest = toml::from_str(contents)?;
    manifest.validate()?;
    Ok(manifest)
}

#[test]
fn parse_with_frontends() {
    let manifest = parse_manifest(
        r#"
[project]
name = "marketplace"
module = "github.com/acme/marketplace"
schema = 1

[lazuli]
runtime = "0.1.0"

[frontends.mobile]
target = "expo"
out = "dist/ts-mobile"
audiences = ["buyer", "seller"]

[frontends.web-seller]
target = "tanstack-vite"
out = "dist/ts-web-seller"
audiences = ["seller"]

[frontends.admin]
target = "tanstack-vite"
out = "dist/ts-admin"
audiences = ["admin"]
"#,
    )
    .unwrap();

    assert_eq!(manifest.frontends.len(), 3);
    assert!(matches!(
        manifest.frontends["mobile"].target,
        FrontendTarget::Expo
    ));
    assert_eq!(manifest.frontends["web-seller"].out, "dist/ts-web-seller");
}

#[test]
fn reject_unknown_frontend_target() {
    let err = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[frontends.mobile]
target = "react-native"
out = "dist/ts-mobile"
audiences = ["traveler"]
"#,
    )
    .unwrap_err();

    assert!(matches!(err, ManifestError::Toml(_)));
}

#[test]
fn reject_frontend_out_collision() {
    let err = parse_manifest(
        r#"
[project]
name = "myapp"
module = "github.com/myorg/myapp"
schema = 1

[lazuli]
runtime = "0.1.0"

[frontends.mobile]
target = "expo"
out = "dist/ts"
audiences = ["traveler"]

[frontends.web]
target = "tanstack-vite"
out = "dist/ts"
audiences = ["host"]
"#,
    )
    .unwrap_err();

    assert!(
        matches!(err, ManifestError::FrontendOutCollision(name, out) if name == "web" && out == "dist/ts")
    );
}
