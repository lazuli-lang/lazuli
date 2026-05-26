//! Tests for `checks/scalar_fixtures` — kept beside production but in
//! a sibling file so the parent module stays under the 500-LOC ceiling
//! (Rails-style R9 split).

use super::*;
use std::fs;
use std::path::Path;

fn write(path: &Path, source: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, source).unwrap();
}

fn write_lazurite(root: &Path, plugin_path: &Path) {
    write(
        &root.join(WORKSPACE_MANIFEST),
        &format!(
            r#"
[project]
name = "fixture-app"
module = "example.com/fixture-app"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@lazuli/plugin-scalars-br" = {{ path = "{}" }}
"#,
            plugin_path.display().to_string().replace('\\', "\\\\")
        ),
    );
}

fn write_plugin_manifest(plugin: &Path, semantic_type: &str) {
    write(
        &plugin.join(PLUGIN_MANIFEST),
        &format!(
            r#"
[plugin]
name = "scalars-br"
namespace = "@lazuli/plugin-scalars-br"

[[semantic_types]]
name = "{semantic_type}"
alias = "@semantic.{semantic_type}"
carrier_type = "String"
validator = "Validate{semantic_type}"
"#
        ),
    );
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<&'static str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn diagnostic_001_unknown_semantic_reference() {
    let tmp = tempfile::tempdir().unwrap();
    write(
        &tmp.path().join("features/customer/customer.lzi"),
        r#"
feature customer
  domain
    resource Customer
      cpf: @semantic.BrazilianCPF
"#,
    );

    let diagnostics = check(tmp.path());

    assert_eq!(codes(&diagnostics), vec!["SCALAR-FIXTURES-001"]);
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert_eq!(
        &tmp.path().join("features/customer/customer.lzi"),
        &diagnostics[0].path
    );
    let span = diagnostics[0].span.expect("reference span");
    assert_eq!(
        &fs::read_to_string(&diagnostics[0].path).unwrap()[span.start..span.end],
        "@semantic.BrazilianCPF"
    );
}

#[test]
fn diagnostic_002_manifest_semantic_types_without_fixtures_export() {
    let tmp = tempfile::tempdir().unwrap();
    let plugin = tmp.path().join("plugins/scalars-br");
    write_lazurite(tmp.path(), &plugin);
    write_plugin_manifest(&plugin, "BrazilianCPF");
    write(
        &plugin.join(PACKAGE_JSON),
        r#"{ "name": "@lazuli/scalars-br", "exports": { ".": "./index.ts" } }"#,
    );

    let diagnostics = check(tmp.path());

    assert_eq!(codes(&diagnostics), vec!["SCALAR-FIXTURES-002"]);
    assert_eq!(diagnostics[0].path, plugin.join(PLUGIN_MANIFEST));
}

#[test]
fn diagnostic_003_manifest_type_missing_from_fixtures_map() {
    let tmp = tempfile::tempdir().unwrap();
    let plugin = tmp.path().join("plugins/scalars-br");
    write_lazurite(tmp.path(), &plugin);
    write_plugin_manifest(&plugin, "BrazilianCPF");
    write(
        &plugin.join(PACKAGE_JSON),
        r#"{ "name": "@lazuli/scalars-br", "exports": { "./fixtures": "./fixtures.ts" } }"#,
    );
    write(
        &plugin.join("fixtures.ts"),
        r#"
export const fixtures = {
  BrazilianCNPJ: {
    valid: ["11222333000181"],
  },
};
"#,
    );

    let diagnostics = check(tmp.path());

    assert_eq!(codes(&diagnostics), vec!["SCALAR-FIXTURES-003"]);
    assert!(diagnostics[0].message.contains("BrazilianCPF"));
}

#[test]
fn happy_path_has_no_scalar_fixture_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let plugin = tmp.path().join("plugins/scalars-br");
    write_lazurite(tmp.path(), &plugin);
    write_plugin_manifest(&plugin, "BrazilianCPF");
    write(
        &plugin.join(PACKAGE_JSON),
        r#"{ "name": "@lazuli/scalars-br", "exports": { "./fixtures": "./fixtures.ts" } }"#,
    );
    write(
        &plugin.join("fixtures.ts"),
        r#"
export const fixtures = {
  BrazilianCPF: {
    valid: ["11144477735"],
  },
};
"#,
    );
    write(
        &tmp.path().join("features/customer/customer.lzi"),
        r#"
feature customer
  domain
    resource Customer
      cpf: @semantic.BrazilianCPF
      email: @semantic.Email
"#,
    );

    let diagnostics = check(tmp.path());

    assert!(diagnostics.is_empty(), "{diagnostics:#?}");
}
