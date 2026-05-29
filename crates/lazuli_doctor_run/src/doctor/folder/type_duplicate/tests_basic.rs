//! Basic redeclaration + skip-list + helper coverage for `type_duplicate`.
//!
//! Covers the legacy `app/features/<feat>/{web,mobile}/` and `app/lib/`
//! redeclaration shapes, the `dist/`/`node_modules/` skip-list, the
//! deterministic-walk contract, and the `extract_declared_type_names`
//! helper. Post-Wave-H plural+singular coverage and Wave S2 import-block
//! awareness live in `tests_wave_h.rs` and `tests_import_blocks.rs`.

#![cfg(test)]

use super::test_support::{TempDir, write};
use super::*;

#[test]
fn redeclared_interface_fires() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "dist/ts-web/slug/slug.gen.ts",
        "export interface Slug { id: string }\n",
    );
    write(
        dir.path(),
        "features/foo/web/views/admin/list.tsx",
        "interface Slug { name: string }\n",
    );

    let findings = check(dir.path());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].type_name, "Slug");
    assert!(findings[0].message.contains("redeclares type `Slug`"));
}

#[test]
fn redeclared_type_alias_fires() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "dist/ts-web/slug/slug.gen.ts",
        "export type Slug = { id: string }\n",
    );
    write(
        dir.path(),
        "app/lib/types.ts",
        "export type Slug = { name: string }\n",
    );

    let findings = check(dir.path());
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].generated_origin,
        dir.path().join("dist/ts-web/slug/slug.gen.ts")
    );
}

#[test]
fn unique_user_type_does_not_fire() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "dist/ts-web/slug/slug.gen.ts",
        "export interface Slug { id: string }\n",
    );
    write(
        dir.path(),
        "app/lib/types.ts",
        "type MyLocal = { id: string }\n",
    );

    assert!(check(dir.path()).is_empty());
}

#[test]
fn import_of_generated_does_not_fire() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "dist/ts-web/slug/slug.gen.ts",
        "export interface Slug { id: string }\n",
    );
    write(
        dir.path(),
        "features/foo/web/views/admin/list.tsx",
        "import type { Slug } from \"@/dist/ts-web/slug/slug.gen\";\n",
    );

    assert!(check(dir.path()).is_empty());
}

#[test]
fn dist_is_not_scanned_as_user_source() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "dist/ts-web/slug/slug.gen.ts",
        "export interface Slug { id: string }\n",
    );

    assert!(check(dir.path()).is_empty());
}

#[test]
fn nested_features_scanned() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "dist/ts-mobile/slug/slug.gen.ts",
        "export interface Slug { id: string }\n",
    );
    write(
        dir.path(),
        "features/foo/web/cells/bar.tsx",
        "export interface Slug { name: string }\n",
    );

    let findings = check(dir.path());
    assert_eq!(findings.len(), 1);
    assert_eq!(
        findings[0].user_file,
        dir.path().join("features/foo/web/cells/bar.tsx")
    );
}

#[test]
fn node_modules_not_scanned() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "dist/ts-web/slug/slug.gen.ts",
        "export interface Slug { id: string }\n",
    );
    write(
        dir.path(),
        "node_modules/x/y.d.ts",
        "export interface Slug { name: string }\n",
    );

    assert!(check(dir.path()).is_empty());
}

#[test]
fn deterministic_order() {
    let dir = TempDir::new().unwrap();
    write(
        dir.path(),
        "dist/ts-web/slug/slug.gen.ts",
        "export interface Slug { id: string }\nexport type Widget = { id: string }\n",
    );
    write(
        dir.path(),
        "app/lib/z.ts",
        "type Widget = {}\ninterface Slug {}\n",
    );
    write(
        dir.path(),
        "features/foo/web/cells/a.tsx",
        "interface Slug {}\n",
    );

    let first = check(dir.path())
        .into_iter()
        .map(|f| (f.user_file, f.type_name))
        .collect::<Vec<_>>();
    let second = check(dir.path())
        .into_iter()
        .map(|f| (f.user_file, f.type_name))
        .collect::<Vec<_>>();
    assert_eq!(first, second);
}

#[test]
fn extract_export_interface() {
    assert_eq!(
        extract_declared_type_names("export interface Slug { id: string }"),
        vec!["Slug"]
    );
}

#[test]
fn extract_type_alias() {
    assert_eq!(
        extract_declared_type_names("type Slug = { id: string }"),
        vec!["Slug"]
    );
}

#[test]
fn extract_ignores_mixed_line_not_matching() {
    assert!(extract_declared_type_names("const x: Slug = value").is_empty());
}
