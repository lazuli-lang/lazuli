//! ENC-TENANCY-001 — feature uses `@cap.Encrypted(key:@key.tenant)` (or
//! `@cap.E2ee(key:@key.tenant)`) but its `defaults.tenancy` is not
//! `Org` (or another tenant-bearing axis). Tenant-scoped encryption
//! without tenant-scoped data is a contract gap.
//!
//! Severity: `warning` (strict + production).
//! Reference: docs/proposals/encryption-vocab.md §Doctor diagnostics.

use std::path::{Path, PathBuf};

use lazuli_ir::{CapabilityRef, Feature, Tenancy, TypeRef};

// ── output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    /// Resource declaring the offending tenant-scoped capability.
    pub resource: String,
    pub field: String,
    /// `@cap.Encrypted` or `@cap.E2ee`.
    pub capability: &'static str,
}

impl Finding {
    pub const CODE: &'static str = "ENC-TENANCY-001";

    pub fn message(&self) -> String {
        format!(
            "feature `{}` uses `{}(key:@key.tenant)` on `{}.{}` but its `defaults.tenancy` is not `org` (or another tenant axis) — tenant-scoped encryption without tenant-scoped data is a contract gap",
            self.feature, self.capability, self.resource, self.field
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

fn is_tenant_bearing(t: Option<&Tenancy>) -> bool {
    matches!(
        t,
        Some(Tenancy::Org) | Some(Tenancy::Team) | Some(Tenancy::Custom(_))
    )
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if is_tenant_bearing(feature.defaults.tenancy.as_ref()) {
        return vec![];
    }

    let mut findings = vec![];

    for resource in &feature.resources {
        // Resource-level tenancy may override feature defaults.
        let effective_tenancy = resource.tenancy.as_ref().or(feature.defaults.tenancy.as_ref());
        if is_tenant_bearing(effective_tenancy) {
            continue;
        }
        for field in &resource.fields {
            let TypeRef::Capability(cap) = &field.type_ref else {
                continue;
            };
            let (key_scope, capability) = match cap {
                CapabilityRef::Encrypted(e) => (e.key.as_str(), "@cap.Encrypted"),
                CapabilityRef::E2ee(e) => (e.key.as_str(), "@cap.E2ee"),
                _ => continue,
            };
            if key_scope == "@key.tenant" {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    field: field.name.clone(),
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

    #[test]
    fn positive_no_tenancy_with_tenant_key_fires() {
        let mut feature = empty_feature("customer");
        feature.resources.push(resource_with_fields(
            "Customer",
            vec![encrypted_field("external_id", "@key.tenant")],
        ));
        // defaults.tenancy = None (no tenant axis).
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "external_id");
    }

    #[test]
    fn negative_org_tenancy_passes() {
        let mut feature = empty_feature("customer");
        feature.defaults.tenancy = Some(Tenancy::Org);
        feature.resources.push(resource_with_fields(
            "Customer",
            vec![encrypted_field("external_id", "@key.tenant")],
        ));
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_app_key_scope_does_not_fire() {
        let mut feature = empty_feature("customer");
        feature.resources.push(resource_with_fields(
            "Customer",
            vec![encrypted_field("external_id", "@key.app")],
        ));
        // @key.app is not tenant-scoped; tenancy-mismatch doesn't apply.
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
