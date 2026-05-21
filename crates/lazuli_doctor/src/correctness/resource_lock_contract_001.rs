//! RESOURCE-LOCK-CONTRACT-001 — `lock` decorator contract violation.
//!
//! Doctor correctness rule (NOT a vocab rule — fires on misuse of the
//! closed catalog, not style drift). Two violations:
//!
//! 1. `lock optimistic` referenced a `version_field` that does not name
//!    a field on the resource. Cross-checks `LockSpec::Optimistic` →
//!    `Resource.fields[*].name`.
//! 2. `lock optimistic` referenced a `version_field` whose type is not
//!    `Integer`. Optimistic locking compares monotonic version
//!    numbers; non-integer types would never work in practice.
//!
//! Severity: `error` — the IR is structurally valid (the parser
//! accepted the surface) but the semantic contract is broken, so any
//! downstream codegen would produce SQL that fails at runtime.
//!
//! Reference: docs/roadmap.md §1.5.

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, LockSpec, Resource, TypeRef};

// ── output ───────────────────────────────────────────────────────────────────

/// One finding: the resource's optimistic lock references a missing
/// or non-integer `version_field`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Feature containing the resource.
    pub feature: String,
    /// Resource declaring the `lock optimistic`.
    pub resource: String,
    /// `version_field` name referenced by the lock spec.
    pub version_field: String,
    /// Why the contract failed (`missing` / `not_integer`).
    pub reason: Reason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// `version_field` does not match any declared field on the resource.
    Missing,
    /// `version_field` is declared but its type is not `Integer`.
    NotInteger,
}

impl Finding {
    pub const CODE: &'static str = "RESOURCE-LOCK-CONTRACT-001";

    pub fn message(&self) -> String {
        match self.reason {
            Reason::Missing => format!(
                "resource `{}` declares `lock optimistic version_field: {}` but no field named `{}` exists on the resource. \
                 Add the field as `{}: Integer required` or change the lock's `version_field`.",
                self.resource, self.version_field, self.version_field, self.version_field
            ),
            Reason::NotInteger => format!(
                "resource `{}` declares `lock optimistic version_field: {}` but the field is not `Integer`. \
                 Optimistic locking compares monotonic counters; declare `{}: Integer required` to match the contract.",
                self.resource, self.version_field, self.version_field
            ),
        }
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run RESOURCE-LOCK-CONTRACT-001 over every resource in one feature.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .filter_map(|resource| inspect_resource(resource).map(|reason| (resource, reason)))
        .map(|(resource, (vf, reason))| Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            resource: resource.name.clone(),
            version_field: vf,
            reason,
        })
        .collect()
}

fn inspect_resource(resource: &Resource) -> Option<(String, Reason)> {
    let Some(LockSpec::Optimistic { version_field }) = resource.lock.as_ref() else {
        return None;
    };
    let Some(field) = resource.fields.iter().find(|f| &f.name == version_field) else {
        return Some((version_field.clone(), Reason::Missing));
    };
    if !is_integer_like(&field.type_ref) {
        return Some((version_field.clone(), Reason::NotInteger));
    }
    None
}

fn is_integer_like(type_ref: &TypeRef) -> bool {
    matches!(type_ref, TypeRef::Builtin(BuiltinType::Integer))
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, Field, FieldConstraints, Policies, Resource, TypeRef};

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "billing".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>, lock: Option<LockSpec>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
            lock,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            span_ref: None,
        }
    }

    #[test]
    fn positive_missing_version_field_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Invoice",
            vec![mk_field("total", TypeRef::Builtin(BuiltinType::Integer))],
            Some(LockSpec::Optimistic {
                version_field: "lock_version".into(),
            }),
        )]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::Missing);
        assert_eq!(findings[0].version_field, "lock_version");
        assert!(findings[0].message().contains("no field named"));
        assert_eq!(Finding::CODE, "RESOURCE-LOCK-CONTRACT-001");
    }

    #[test]
    fn positive_version_field_type_mismatch_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Invoice",
            vec![mk_field(
                "lock_version",
                TypeRef::Builtin(BuiltinType::Text),
            )],
            Some(LockSpec::Optimistic {
                version_field: "lock_version".into(),
            }),
        )]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::NotInteger);
        assert!(findings[0].message().contains("not `Integer`"));
    }

    #[test]
    fn negative_valid_optimistic_lock_does_not_fire() {
        let feature = mk_feature(vec![mk_resource(
            "Invoice",
            vec![mk_field(
                "lock_version",
                TypeRef::Builtin(BuiltinType::Integer),
            )],
            Some(LockSpec::Optimistic {
                version_field: "lock_version".into(),
            }),
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_pessimistic_lock_skipped() {
        let feature = mk_feature(vec![mk_resource(
            "Invoice",
            vec![],
            Some(LockSpec::Pessimistic),
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_row_level_lock_skipped() {
        let feature = mk_feature(vec![mk_resource(
            "Invoice",
            vec![],
            Some(LockSpec::RowLevel),
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_no_lock_declared_does_not_fire() {
        let feature = mk_feature(vec![mk_resource("Invoice", vec![], None)]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
