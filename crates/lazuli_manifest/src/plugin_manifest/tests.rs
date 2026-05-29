//! Tests for the plugin-manifest loader + alias-map builder.
//!
//! Moved out of `mod.rs` for the rails-style size budget; logically
//! these are still the production tests for the entire
//! `plugin_manifest::` module.

#![cfg(test)]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::lazurite_manifest::{LazuliPin, Manifest, Plugin, Project};

use super::{
    build_alias_map, PluginManifestError, PLUGIN_MANIFEST_FILENAME,
};

fn make_manifest(plugins: BTreeMap<String, Plugin>) -> Manifest {
    Manifest {
        project: Project {
            name: "test".to_owned(),
            module: "github.com/example/test".to_owned(),
            schema: 1,
        },
        lazuli: LazuliPin {
            runtime: "0.1.0".to_owned(),
            path: None,
        },
        lazurite: None,
        plugins,
        generate: Default::default(),
        frontends: BTreeMap::new(),
        migrations: None,
        seeds: None,
        dev: None,
        doctor: None,
        testing: None,
    }
}

fn write_manifest(dir: &Path, body: &str) {
    fs::write(dir.join(PLUGIN_MANIFEST_FILENAME), body).unwrap();
}

#[test]
fn missing_manifest_yields_empty_map() {
    let tmp = tempfile::tempdir().unwrap();
    let mut plugins = BTreeMap::new();
    plugins.insert(
        "@lazuli/plugin-empty".to_owned(),
        Plugin::Local {
            path: tmp.path().display().to_string(),
        },
    );
    let manifest = make_manifest(plugins);
    let map = build_alias_map(Some(&manifest), tmp.path()).unwrap();
    assert!(map.is_empty());
}

#[test]
fn happy_path_loads_one_alias() {
    let tmp = tempfile::tempdir().unwrap();
    write_manifest(
        tmp.path(),
        r#"
[plugin]
name = "scalars-br"
namespace = "@lazuli/plugin-scalars-br"

[[semantic_types]]
name = "BrazilianCPF"
alias = "@semantic.BrazilianCPF"
carrier_type = "String"
validator = "ValidateCPF"
formatter = "FormatCPF"
"#,
    );
    let mut plugins = BTreeMap::new();
    plugins.insert(
        "@lazuli/plugin-scalars-br".to_owned(),
        Plugin::Local {
            path: tmp.path().display().to_string(),
        },
    );
    let manifest = make_manifest(plugins);
    let map = build_alias_map(Some(&manifest), tmp.path()).unwrap();
    let entry = map.get("@semantic.BrazilianCPF").expect("alias resolved");
    assert_eq!(entry.plugin_namespace, "@lazuli/plugin-scalars-br");
    assert_eq!(entry.plugin_short_name, "scalars-br");
    assert_eq!(entry.name, "BrazilianCPF");
    assert_eq!(entry.validator, "ValidateCPF");
    assert_eq!(entry.formatter.as_deref(), Some("FormatCPF"));
    assert_eq!(entry.carrier, lazuli_ir::BuiltinType::Text);
}

#[test]
fn duplicate_alias_across_plugins_yields_conflict() {
    let tmp_a = tempfile::tempdir().unwrap();
    let tmp_b = tempfile::tempdir().unwrap();
    write_manifest(
        tmp_a.path(),
        r#"
[plugin]
name = "alpha"
namespace = "@lazuli/plugin-alpha"

[[semantic_types]]
name = "MyType"
alias = "@semantic.MyType"
carrier_type = "String"
validator = "ValidateAlpha"
"#,
    );
    write_manifest(
        tmp_b.path(),
        r#"
[plugin]
name = "beta"
namespace = "@lazuli/plugin-beta"

[[semantic_types]]
name = "MyType"
alias = "@semantic.MyType"
carrier_type = "String"
validator = "ValidateBeta"
"#,
    );
    let mut plugins = BTreeMap::new();
    plugins.insert(
        "@lazuli/plugin-alpha".to_owned(),
        Plugin::Local {
            path: tmp_a.path().display().to_string(),
        },
    );
    plugins.insert(
        "@lazuli/plugin-beta".to_owned(),
        Plugin::Local {
            path: tmp_b.path().display().to_string(),
        },
    );
    let manifest = make_manifest(plugins);
    let err = build_alias_map(Some(&manifest), tmp_a.path()).unwrap_err();
    match err {
        PluginManifestError::Conflict { alias, plugins } => {
            assert_eq!(alias, "@semantic.MyType");
            assert_eq!(plugins, vec!["@lazuli/plugin-alpha", "@lazuli/plugin-beta"]);
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
}

#[test]
fn unsupported_carrier_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    write_manifest(
        tmp.path(),
        r#"
[plugin]
name = "scalars-br"
namespace = "@lazuli/plugin-scalars-br"

[[semantic_types]]
name = "WideThing"
alias = "@semantic.WideThing"
carrier_type = "Integer"
validator = "ValidateWide"
"#,
    );
    let mut plugins = BTreeMap::new();
    plugins.insert(
        "@lazuli/plugin-scalars-br".to_owned(),
        Plugin::Local {
            path: tmp.path().display().to_string(),
        },
    );
    let manifest = make_manifest(plugins);
    let err = build_alias_map(Some(&manifest), tmp.path()).unwrap_err();
    assert!(matches!(
        err,
        PluginManifestError::UnsupportedCarrier { ref carrier_type, .. } if carrier_type == "Integer"
    ));
}

#[test]
fn name_alias_mismatch_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    write_manifest(
        tmp.path(),
        r#"
[plugin]
name = "scalars-br"
namespace = "@lazuli/plugin-scalars-br"

[[semantic_types]]
name = "Foo"
alias = "@semantic.Bar"
carrier_type = "String"
validator = "ValidateThing"
"#,
    );
    let mut plugins = BTreeMap::new();
    plugins.insert(
        "@lazuli/plugin-scalars-br".to_owned(),
        Plugin::Local {
            path: tmp.path().display().to_string(),
        },
    );
    let manifest = make_manifest(plugins);
    let err = build_alias_map(Some(&manifest), tmp.path()).unwrap_err();
    assert!(matches!(err, PluginManifestError::NameAliasMismatch { .. }));
}

#[test]
fn namespace_mismatch_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    // Namespace mismatch is only enforced when the manifest
    // actually declares semantic types — otherwise the manifest
    // is a non-semantic contract (e.g. storage adapter) and is
    // tolerated by the loader.
    write_manifest(
        tmp.path(),
        r#"
[plugin]
name = "scalars-br"
namespace = "@lazuli/plugin-something-else"

[[semantic_types]]
name = "MyType"
alias = "@semantic.MyType"
carrier_type = "String"
validator = "ValidateMine"
"#,
    );
    let mut plugins = BTreeMap::new();
    plugins.insert(
        "@lazuli/plugin-scalars-br".to_owned(),
        Plugin::Local {
            path: tmp.path().display().to_string(),
        },
    );
    let manifest = make_manifest(plugins);
    let err = build_alias_map(Some(&manifest), tmp.path()).unwrap_err();
    assert!(matches!(err, PluginManifestError::NamespaceMismatch { .. }));
}

#[test]
fn non_semantic_manifest_tolerated() {
    // A plugin whose `manifest.toml` follows a different sibling
    // contract (e.g. storage-only adapter using top-level keys)
    // must not break the alias-map build. The loader skips
    // cleanly; doctor's other plugin lints cover unrelated
    // breakage.
    let tmp = tempfile::tempdir().unwrap();
    write_manifest(
        tmp.path(),
        r#"
name = "@lazuli/plugin-object-store"
version = "0.1.0"
implements = ["storage.ObjectStore"]
"#,
    );
    let mut plugins = BTreeMap::new();
    plugins.insert(
        "@lazuli/plugin-object-store".to_owned(),
        Plugin::Local {
            path: tmp.path().display().to_string(),
        },
    );
    let manifest = make_manifest(plugins);
    let map = build_alias_map(Some(&manifest), tmp.path()).unwrap();
    assert!(map.is_empty());
}

#[test]
fn none_manifest_yields_empty_map() {
    let tmp = tempfile::tempdir().unwrap();
    let map = build_alias_map(None, tmp.path()).unwrap();
    assert!(map.is_empty());
}
