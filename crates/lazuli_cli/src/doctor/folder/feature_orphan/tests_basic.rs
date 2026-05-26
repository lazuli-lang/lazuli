//! Basic canon + polyglot-scope coverage for `feature_orphan`.
//!
//! Covers the legacy `app/features/<feat>/{web,mobile}/` layout, the
//! legacy `frontends/<target>/` wrapper, the polyglot-monorepo
//! scope-discipline arms (`apps/`, `packages/`, `brand/`, `scripts/`,
//! top-level `src/`), the skip-list (test/story files, `node_modules`),
//! and the deterministic-walk contract.
//!
//! Post-Wave-H 7+6 catalog coverage lives in the sibling
//! `tests_wave_h.rs`.

#![cfg(test)]

use super::*;
use super::test_support::{touch, TempDir};

#[test]
fn canonical_feature_view_does_not_fire() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "app/features/slug/web/views/admin/list.tsx");

    let findings = check(tmp.path());

    assert!(findings.is_empty());
}

#[test]
fn canonical_feature_cell_does_not_fire() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "app/features/slug/web/cells/type_badge.tsx");

    let findings = check(tmp.path());

    assert!(findings.is_empty());
}

#[test]
fn canonical_app_ui_does_not_fire() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "frontends/web/ui/button.tsx");

    let findings = check(tmp.path());

    assert!(findings.is_empty());
}

#[test]
fn orphan_top_level_src_is_out_of_scope() {
    // `src/` at the project root is a polyglot-sibling concern (Astro,
    // Vite, etc.) — not Lazuli-owned. Doctor must not flag files there;
    // an orphan inside `app/components/` is the correct in-scope signal.
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "src/components/SlugTable.tsx");

    let findings = check(tmp.path());

    assert!(findings.is_empty());
}

#[test]
fn orphan_app_components_fires() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "app/components/Foo.tsx");

    let findings = check(tmp.path());

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].path, PathBuf::from("app/components/Foo.tsx"));
}

#[test]
fn polyglot_apps_sibling_is_out_of_scope() {
    // Polyglot monorepo: `apps/website/` is an Astro/Vite sibling owned
    // by the pnpm workspace, not Lazuli. Doctor must not descend.
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "apps/website/src/content/copy.ts");
    touch(tmp.path(), "apps/example-app/src/main.tsx");

    let findings = check(tmp.path());

    assert!(findings.is_empty());
}

#[test]
fn polyglot_packages_sibling_is_out_of_scope() {
    // `packages/<pkg>/` are pnpm-workspace siblings (design tokens,
    // shared utilities, etc.) — not Lazuli-owned.
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "packages/design-tokens/src/index.ts");
    touch(tmp.path(), "packages/design-tokens/scripts/build-css.ts");
    touch(tmp.path(), "brand/assets/index.ts");
    touch(tmp.path(), "scripts/seed.ts");

    let findings = check(tmp.path());

    assert!(findings.is_empty());
}

#[test]
fn orphans_inside_lazuli_owned_tree_still_fire() {
    // Polyglot siblings stay invisible; orphans inside the Lazuli-owned
    // subtree (`app/...`, `frontends/...`) still fire.
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "apps/website/src/orphan.ts");
    touch(tmp.path(), "packages/util/src/orphan.ts");
    touch(tmp.path(), "app/shared/ui/Bad.tsx");
    touch(tmp.path(), "frontends/web/junk/Bad.tsx");
    touch(tmp.path(), "app/features/slug/web/cells/ok.tsx");
    touch(tmp.path(), "frontends/web/ui/button.tsx");

    let findings = check(tmp.path());

    assert_eq!(findings.len(), 2, "found: {:?}", findings);
    assert_eq!(findings[0].path, PathBuf::from("app/shared/ui/Bad.tsx"));
    assert_eq!(
        findings[1].path,
        PathBuf::from("frontends/web/junk/Bad.tsx")
    );
}

#[test]
fn test_files_are_skipped() {
    let tmp = TempDir::new().unwrap();
    touch(
        tmp.path(),
        "app/features/slug/web/views/admin/list.test.tsx",
    );

    let findings = check(tmp.path());

    assert!(findings.is_empty());
}

#[test]
fn node_modules_skipped() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "node_modules/pkg/components/Button.tsx");

    let findings = check(tmp.path());

    assert!(findings.is_empty());
}

#[test]
fn deterministic_order() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "frontends/web/junk/Zed.tsx");
    touch(tmp.path(), "frontends/web/junk/Alpha.tsx");
    touch(tmp.path(), "app/components/Foo.tsx");

    let first = check(tmp.path());
    let second = check(tmp.path());

    assert_eq!(first, second);
    assert_eq!(
        first.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
        vec![
            PathBuf::from("app/components/Foo.tsx"),
            PathBuf::from("frontends/web/junk/Alpha.tsx"),
            PathBuf::from("frontends/web/junk/Zed.tsx"),
        ]
    );
}
