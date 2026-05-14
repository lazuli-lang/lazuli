//! ENC-KEY-MISSING-001 — field declares `@cap.Encrypted(key:@key.<scope>)`
//! or `@cap.E2ee(key:@key.<scope>)` but the app has no matching
//! `encryption.key @key.<scope>` binding.
//!
//! Severity: `error` (strict + production).
//! Reference: docs/proposals/encryption-vocab.md §Doctor diagnostics.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{AppManifest, CapabilityRef, Feature, TypeRef};

// ── output ────────────────────────────────────────────────────────────────────

/// One ENC-KEY-MISSING-001 finding: a field references a `@key.<scope>`
/// not declared in `app.encryption_bindings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    /// Resource declaring the field.
    pub resource: String,
    /// Field name.
    pub field: String,
    /// `@key.<scope>` reference the field carries, verbatim.
    pub key_scope: String,
    /// Which capability kind triggered (`@cap.Encrypted` or `@cap.E2ee`).
    pub capability: &'static str,
}

impl Finding {
    pub const CODE: &'static str = "ENC-KEY-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "field `{}.{}` declares `{}(key:{})` but the app declares no `encryption.key {}` binding — add a binding in `app.lzi` (or `registry.lzi`)",
            self.resource, self.field, self.capability, self.key_scope, self.key_scope
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run ENC-KEY-MISSING-001 for all `@cap.Encrypted` / `@cap.E2ee`
/// fields in one feature, joined against the app's binding catalog.
pub fn check(feature: &Feature, app: &AppManifest, path: &Path) -> Vec<Finding> {
    let declared: HashSet<&str> = app
        .encryption_bindings
        .iter()
        .map(|b| b.scope.as_str())
        .collect();

    let mut findings = vec![];

    for resource in &feature.resources {
        for field in &resource.fields {
            let TypeRef::Capability(cap) = &field.type_ref else {
                continue;
            };
            let (key_scope, capability) = match cap {
                CapabilityRef::Encrypted(e) => (e.key.as_str(), "@cap.Encrypted"),
                CapabilityRef::E2ee(e) => (e.key.as_str(), "@cap.E2ee"),
                _ => continue,
            };

            if !declared.contains(key_scope) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                    key_scope: key_scope.to_owned(),
                    capability,
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::encryption::test_support::*;

    fn feature_with_encrypted(scope: &str) -> Feature {
        let mut feature = empty_feature("customer");
        feature.resources.push(resource_with_fields(
            "Customer",
            vec![encrypted_field("external_id", scope)],
        ));
        feature
    }

    #[test]
    fn positive_missing_binding_fires() {
        let app = empty_app();
        let feature = feature_with_encrypted("@key.tenant");

        let findings = check(&feature, &app, Path::new("features/customer.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Customer");
        assert_eq!(findings[0].field, "external_id");
        assert_eq!(findings[0].key_scope, "@key.tenant");
        assert_eq!(findings[0].capability, "@cap.Encrypted");
        assert!(findings[0].message().contains("@key.tenant"));
    }

    #[test]
    fn negative_binding_declared_does_not_fire() {
        let mut app = empty_app();
        app.encryption_bindings = vec![make_binding(
            "@key.tenant",
            "CRYPT_KEY_TENANT_{tenant_id}",
        )];
        let feature = feature_with_encrypted("@key.tenant");

        let findings = check(&feature, &app, Path::new("f.lzi"));
        assert!(findings.is_empty());
    }

    #[test]
    fn positive_e2ee_field_without_binding_fires() {
        let app = empty_app();
        let mut feature = empty_feature("notes");
        feature.resources.push(resource_with_fields(
            "Note",
            vec![e2ee_field("body", "@key.user")],
        ));

        let findings = check(&feature, &app, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].capability, "@cap.E2ee");
    }
}
