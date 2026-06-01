//! 0019 — plugin-resolution unify: the single-stage drift guard + the
//! loud-failure contract.
//!
//! These tests pin the FOUNDATION of the Plugin Platform: both module
//! loaders resolve plugin `@semantic.*` refs through ONE stage, project
//! roots are found by walking UP, and a declared-but-unwired plugin
//! fails loud at the resolution boundary instead of as an opaque
//! downstream codegen error.

use std::path::{Path, PathBuf};

use lazuli_cli::module_loader::{
    build_module_from_path, build_module_with_source_from_path, find_project_root,
    resolve_module_plugins,
};
use lazuli_ir::{CommandInput, Module, TypeRef};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// The resolving fixture: `Lazurite.toml [plugins]` → a local mini
/// plugin declaring `@semantic.Foo` (carrier String), features under
/// `app/`.
fn good_fixture_root() -> PathBuf {
    fixtures_dir().join("plugin_resolution")
}

/// Collect every distinct `@semantic.*` alias still carried as
/// `UserDefined` after resolution — the residual the single stage must
/// drive to zero on a fully-wired app.
fn residual_semantic_aliases(module: &Module) -> Vec<String> {
    let mut out = std::collections::BTreeSet::new();
    let mut visit = |tr: &TypeRef| {
        let mut t = tr;
        loop {
            match t {
                TypeRef::UserDefined(q) if q.name.starts_with("@semantic.") => {
                    out.insert(q.name.clone());
                    break;
                }
                TypeRef::Many(inner) => t = inner,
                _ => break,
            }
        }
    };
    for feature in &module.features {
        for resource in &feature.resources {
            for field in &resource.fields {
                visit(&field.type_ref);
            }
        }
        for record in &feature.records {
            for field in &record.fields {
                visit(&field.type_ref);
            }
        }
        for command in &feature.commands {
            if let CommandInput::Typed(slots) = &command.input {
                for slot in slots {
                    visit(&slot.type_ref);
                }
            }
        }
    }
    out.into_iter().collect()
}

fn is_resolved_foo(tr: &TypeRef) -> bool {
    matches!(
        tr,
        TypeRef::Builtin(lazuli_ir::BuiltinType::SemanticPluginType { name, .. }) if name == "Foo"
    )
}

/// The DRIFT GUARD: `build_module_from_path` and
/// `build_module_with_source_from_path`, given the SAME plugin-using
/// input dir, both yield a module whose `@semantic.Foo` refs are ALL
/// resolved to `SemanticPluginType` (zero residual `UserDefined`).
#[test]
fn both_loaders_resolve_plugin_semantics() {
    let input = good_fixture_root();

    let module_a = build_module_from_path(&input).expect("plain loader builds");
    let (module_b, _sm, _ids) =
        build_module_with_source_from_path(&input).expect("source-map loader builds");

    assert!(
        residual_semantic_aliases(&module_a).is_empty(),
        "build_module_from_path left unresolved @semantic.* refs: {:?}",
        residual_semantic_aliases(&module_a)
    );
    assert!(
        residual_semantic_aliases(&module_b).is_empty(),
        "build_module_with_source_from_path left unresolved @semantic.* refs (drift!): {:?}",
        residual_semantic_aliases(&module_b)
    );

    // Both must have actually resolved the Foo carrier (positive
    // control — guards against "resolved" meaning "no fields at all").
    let foo_resolved_a = module_a
        .features
        .iter()
        .flat_map(|f| &f.resources)
        .flat_map(|r| &r.fields)
        .any(|field| is_resolved_foo(&field.type_ref));
    let foo_resolved_b = module_b
        .features
        .iter()
        .flat_map(|f| &f.resources)
        .flat_map(|r| &r.fields)
        .any(|field| is_resolved_foo(&field.type_ref));
    assert!(foo_resolved_a, "plain loader did not resolve @semantic.Foo");
    assert!(
        foo_resolved_b,
        "source-map loader did not resolve @semantic.Foo"
    );
}

/// Project-root detection walks UP: resolving with `input = <root>/app`
/// finds the plugins declared in `<root>/Lazurite.toml`.
#[test]
fn resolves_from_subdir_via_upward_search() {
    let root = good_fixture_root();
    let subdir = root.join("app");

    // find_project_root ascends to the manifest root.
    let found = find_project_root(&subdir).expect("upward search finds project root");
    assert_eq!(
        found.canonicalize().unwrap(),
        root.canonicalize().unwrap(),
        "find_project_root should ascend from app/ to the Lazurite.toml root"
    );

    // And resolution actually wires the plugin from the subdir input.
    let mut module = build_module_from_path(&subdir).expect("subdir builds");
    // Re-running the stage on the subdir input is a no-op idempotent
    // confirmation that the upward search resolves the alias.
    resolve_module_plugins(&mut module, &subdir).expect("resolve from subdir");
    assert!(
        residual_semantic_aliases(&module).is_empty(),
        "subdir input failed to resolve plugins via upward search: {:?}",
        residual_semantic_aliases(&module)
    );
}

/// A `[plugins]` entry whose path has no `manifest.toml` → the residual
/// scan converts the unresolved `@semantic.Foo` into an anchored loud
/// error naming the alias + the declared plugin (not a silent no-op).
#[test]
fn declared_plugin_missing_manifest_fails_loud() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    // Manifest declares a plugin whose path exists but has NO manifest.toml.
    std::fs::create_dir_all(root.join("plugins/ghost")).unwrap();
    std::fs::write(
        root.join("Lazurite.toml"),
        r#"[project]
name = "missing-manifest"
module = "example.test/missing-manifest"
schema = 1

[lazuli]
runtime = "0.1.0"

[plugins]
"@lazuli/plugin-ghost" = { path = "plugins/ghost" }
"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("features/widget")).unwrap();
    std::fs::write(
        root.join("features/widget/widget.lzi"),
        "feature widget\n  domain\n    resource Widget\n      foo: @semantic.Foo required\n",
    )
    .unwrap();

    let err = build_module_from_path(root).expect_err("must fail loud");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("@semantic.Foo"),
        "loud error must name the unresolved alias, got: {msg}"
    );
    assert!(
        msg.contains("@lazuli/plugin-ghost"),
        "loud error must name the declared plugin(s), got: {msg}"
    );
}

/// A manifest with `carrier_type = "Integer"` → the `build_alias_map`
/// UnsupportedCarrier error is propagated LOUDLY (today swallowed by an
/// `if let Ok`).
#[test]
fn unsupported_carrier_fails_loud() {
    let root = fixtures_dir().join("plugin_resolution_bad_carrier");
    let err = build_module_from_path(&root).expect_err("unsupported carrier must fail loud");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("Integer") || msg.to_lowercase().contains("carrier"),
        "loud error must surface the unsupported carrier, got: {msg}"
    );
}

/// Single-file `lazuli check <one.lzi>` (no project root) → the stage is
/// a silent no-op: `@semantic.*` stays `UserDefined`, no error.
#[test]
fn single_file_check_stays_silent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // A lone .lzi in a directory with NO Lazurite.toml anywhere above
    // would walk to the FS root; to keep the test hermetic we assert on
    // the resolver directly with a path that has no ancestor manifest.
    let lzi = tmp.path().join("solo.lzi");
    std::fs::write(
        &lzi,
        "feature solo\n  domain\n    resource Solo\n      foo: @semantic.Foo required\n",
    )
    .unwrap();

    // No Lazurite.toml in tmp (tempdir is isolated). find_project_root
    // must return None (assuming no ancestor manifest); if the CI root
    // somehow has one this would be a different signal — assert the
    // contract via resolve_module_plugins returning Ok and leaving the
    // alias unresolved.
    if find_project_root(&lzi).is_none() {
        let mut module = build_module_from_path(&lzi).expect("single-file builds");
        resolve_module_plugins(&mut module, &lzi).expect("single-file resolve is Ok");
        assert_eq!(
            residual_semantic_aliases(&module),
            vec!["@semantic.Foo".to_owned()],
            "single-file check must leave @semantic.Foo unresolved for the doctor"
        );
    }
}

/// An app with NO `[plugins]` block resolves to `Ok(())` with no new
/// errors (common-case regression — the bulk of apps).
#[test]
fn no_plugins_app_unchanged() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    std::fs::write(
        root.join("Lazurite.toml"),
        r#"[project]
name = "no-plugins"
module = "example.test/no-plugins"
schema = 1

[lazuli]
runtime = "0.1.0"
"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("features/plain")).unwrap();
    std::fs::write(
        root.join("features/plain/plain.lzi"),
        "feature plain\n  domain\n    resource Plain\n      title: Text required\n",
    )
    .unwrap();

    let mut module = build_module_from_path(root).expect("no-plugins app builds");
    resolve_module_plugins(&mut module, root).expect("no-plugins resolve is Ok");
    assert!(
        residual_semantic_aliases(&module).is_empty(),
        "no-plugins app should have no @semantic.* refs at all"
    );
}
