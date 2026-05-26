//! Unit tests for the `extract_import_specifier` and
//! `parse_feature_import` helpers — string-level parsers without any
//! filesystem dependency.
//!
//! Integration tests against `check()` live in `tests_integration.rs`.

#![cfg(test)]

use super::*;

#[test]
fn extract_import_specifier_handles_named_import() {
    assert_eq!(
        extract_import_specifier("import { X } from \"@/features/a/web/cells/x\";"),
        Some("@/features/a/web/cells/x")
    );
}

#[test]
fn extract_import_specifier_handles_default_import_with_single_quotes() {
    assert_eq!(
        extract_import_specifier("import X from '@/features/a/web/cells/x';"),
        Some("@/features/a/web/cells/x")
    );
}

#[test]
fn extract_import_specifier_ignores_commented_import() {
    assert_eq!(
        extract_import_specifier("// import { X } from \"@/features/a/web/cells/x\";"),
        None
    );
}

#[test]
fn extract_import_specifier_handles_type_import() {
    assert_eq!(
        extract_import_specifier("import type { X } from \"@/features/a/web/cells/x\";"),
        Some("@/features/a/web/cells/x")
    );
}

#[test]
fn parse_feature_import_handles_web_target() {
    assert_eq!(
        parse_feature_import("@/features/account/web/cells/avatar"),
        Some(("account", "web"))
    );
}

#[test]
fn parse_feature_import_handles_mobile_target() {
    assert_eq!(
        parse_feature_import("@/features/account/mobile/views/admin/login"),
        Some(("account", "mobile"))
    );
}

#[test]
fn parse_feature_import_ignores_non_frontend_target() {
    assert_eq!(
        parse_feature_import("@/features/account/handlers/verify_password"),
        None
    );
}
