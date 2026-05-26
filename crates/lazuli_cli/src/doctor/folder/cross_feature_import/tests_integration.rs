//! End-to-end integration tests for `cross_feature_import::check` —
//! drive the full filesystem walker against synthetic feature
//! fixtures and assert on the surfaced findings.
//!
//! Helper-fn unit tests live in `tests_helpers.rs`.

#![cfg(test)]

use super::test_support::{write_file, TempDir};
use super::*;

#[test]
fn same_feature_internal_import_does_not_fire() {
    let temp = TempDir::new().unwrap();
    write_file(
        temp.path(),
        "features/slug/web/views/admin/list.tsx",
        "import Badge from \"@/features/slug/web/cells/type_badge\";\n",
    );

    assert!(check(temp.path()).is_empty());
}

#[test]
fn cross_feature_cell_import_fires() {
    let temp = TempDir::new().unwrap();
    let source = write_file(
        temp.path(),
        "features/slug/web/views/admin/list.tsx",
        "import Avatar from \"@/features/account/web/cells/avatar\";\n",
    );

    let findings = check(temp.path());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].source_file, source);
    assert_eq!(findings[0].source_feature, "slug");
    assert_eq!(findings[0].target_feature, "account");
    assert_eq!(
        findings[0].import_specifier,
        "@/features/account/web/cells/avatar"
    );
    assert!(findings[0].message.contains("slot bindings"));
    assert_eq!(Finding::CODE, "cross-feature-direct-import");
}

#[test]
fn cross_feature_view_import_fires() {
    let temp = TempDir::new().unwrap();
    write_file(
        temp.path(),
        "features/slug/web/views/admin/list.tsx",
        "import Login from \"@/features/account/web/views/admin/login\";\n",
    );

    let findings = check(temp.path());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].target_feature, "account");
}

#[test]
fn import_from_dist_does_not_fire() {
    let temp = TempDir::new().unwrap();
    write_file(
        temp.path(),
        "features/slug/web/views/admin/list.tsx",
        "import { Account } from \"@/dist/ts-web/account/account.gen\";\n",
    );

    assert!(check(temp.path()).is_empty());
}

#[test]
fn import_from_app_ui_does_not_fire() {
    let temp = TempDir::new().unwrap();
    write_file(
        temp.path(),
        "features/slug/web/views/admin/list.tsx",
        "import { Button } from \"@/app/ui/button\";\n",
    );

    assert!(check(temp.path()).is_empty());
}

#[test]
fn relative_cross_feature_fires() {
    let temp = TempDir::new().unwrap();
    write_file(
        temp.path(),
        "features/slug/web/views/admin/list.tsx",
        "import Avatar from \"../../account/web/cells/avatar\";\n",
    );

    let findings = check(temp.path());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].source_feature, "slug");
    assert_eq!(findings[0].target_feature, "account");
    assert_eq!(
        findings[0].import_specifier,
        "../../account/web/cells/avatar"
    );
}

#[test]
fn import_type_form_works() {
    let temp = TempDir::new().unwrap();
    write_file(
        temp.path(),
        "features/slug/web/views/admin/list.tsx",
        "import type { X } from '@/features/account/web/cells/x';\n",
    );

    let findings = check(temp.path());
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].target_feature, "account");
}

#[test]
fn commented_import_does_not_fire() {
    let temp = TempDir::new().unwrap();
    write_file(
        temp.path(),
        "features/slug/web/views/admin/list.tsx",
        "// import { X } from \"@/features/account/web/cells/x\";\n",
    );

    assert!(check(temp.path()).is_empty());
}

#[test]
fn deterministic_order() {
    let temp = TempDir::new().unwrap();
    write_file(
        temp.path(),
        "features/slug/web/views/admin/z.tsx",
        "import Z from \"@/features/account/web/cells/z\";\n",
    );
    write_file(
        temp.path(),
        "features/slug/web/views/admin/a.tsx",
        "import A from \"@/features/billing/web/cells/a\";\n",
    );

    let first = check(temp.path());
    let second = check(temp.path());
    assert_eq!(first, second);
    assert_eq!(first.len(), 2);
    assert!(first[0].source_file.ends_with("a.tsx"));
    assert!(first[1].source_file.ends_with("z.tsx"));
}
