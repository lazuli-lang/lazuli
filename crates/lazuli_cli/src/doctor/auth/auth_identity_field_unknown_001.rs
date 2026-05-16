//! auth_identity_field_unknown_001 — `auth identity <Resource>.<field>`
//! does not resolve to an identity-shaped field in the same feature.
//!
//! Three sub-cases trigger a finding:
//!   1. The resource is not declared in the feature.
//!   2. The resource exists but the named field is missing.
//!   3. The field exists but is not identity-shaped (must be tagged
//!      `@semantic.Email` / `@semantic.Phone`, declared `ID`, or carry
//!      the typed `unique` axis).
//!
//! Same-feature resolution only: cross-feature identity references
//! through `uses` are the integration walker's responsibility
//! (`doctor.rs::auth_diagnostics`).
//!
//! Severity: `error`. Login resolution against a missing field is a
//! correctness bug.
//!
//! Reference: docs/proposals/auth-lowering-scope.md §"Closed-cycle criterion"
//! Reference: docs/proposals/bucket-auth-cycle.md §IR.cross-refs

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, Field, TypeRef};

// ── output ────────────────────────────────────────────────────────────────────

/// Why the identity reference failed. Closed catalog — extending requires
/// updating consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    ResourceNotFound,
    FieldNotFound,
    FieldNotIdentityShaped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub identity_resource: String,
    pub identity_field: String,
    pub reason: Reason,
}

impl Finding {
    pub const CODE: &'static str = "auth_identity_field_unknown_001";

    pub fn message(&self) -> String {
        match self.reason {
            Reason::ResourceNotFound => format!(
                "auth.identity `{}.{}` does not resolve: resource not found in feature `{}`.",
                self.identity_resource, self.identity_field, self.feature,
            ),
            Reason::FieldNotFound => format!(
                "auth.identity `{}.{}` does not resolve: field not found on `{}`.",
                self.identity_resource, self.identity_field, self.identity_resource,
            ),
            Reason::FieldNotIdentityShaped => format!(
                "auth.identity `{}.{}` does not resolve: field is not identity-shaped \
                 (missing @semantic.Email / @semantic.Phone / unique).",
                self.identity_resource, self.identity_field,
            ),
        }
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let Some(auth) = feature.auth.as_ref() else {
        return Vec::new();
    };
    let resource_name = auth.identity.field.resource.name.as_str();
    let field_name = auth.identity.field.field.as_str();

    let Some(resource) = feature.resources.iter().find(|r| r.name == resource_name) else {
        return vec![Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            identity_resource: resource_name.to_owned(),
            identity_field: field_name.to_owned(),
            reason: Reason::ResourceNotFound,
        }];
    };

    let Some(field) = resource.fields.iter().find(|f| f.name == field_name) else {
        return vec![Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            identity_resource: resource_name.to_owned(),
            identity_field: field_name.to_owned(),
            reason: Reason::FieldNotFound,
        }];
    };

    if !is_identity_shaped(field) {
        return vec![Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            identity_resource: resource_name.to_owned(),
            identity_field: field_name.to_owned(),
            reason: Reason::FieldNotIdentityShaped,
        }];
    }

    Vec::new()
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Identity-shaped means: tagged `@semantic.Email` / `@semantic.Phone`,
/// declared `ID`, or carries the typed `unique` axis. Mirrors
/// `doctor.rs::is_identity_shaped` semantics over typed IR fields.
fn is_identity_shaped(field: &Field) -> bool {
    match &field.type_ref {
        TypeRef::Builtin(BuiltinType::SemanticEmail | BuiltinType::SemanticPhone) => true,
        TypeRef::Builtin(BuiltinType::Id) => true,
        _ => field.unique,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Auth, AuthIdentity, BuiltinType, Defaults, Feature, Field, FieldConstraints, FieldRef,
        Policies, QualifiedName, Resource, TypeRef,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef, unique: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
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

            lock: None,

            composite_key: None,
        }
    }

    fn mk_feature(
        identity_resource: &str,
        identity_field: &str,
        resources: Vec<Resource>,
    ) -> Feature {
        Feature {
            name: "customer_auth".to_owned(),
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
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            pollers: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            auth: Some(Auth {
                identity: AuthIdentity {
                    field: FieldRef {
                        resource: qn(identity_resource),
                        field: identity_field.to_owned(),
                    },
                    public_contract: None,
                },
                password: None,
                sessions: None,
                mfa: None,
                oauth: vec![],
                span_ref: None,
            }),
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn positive_fires_when_resource_missing() {
        let resource = mk_resource(
            "Customer",
            vec![mk_field(
                "email",
                TypeRef::Builtin(BuiltinType::SemanticEmail),
                true,
            )],
        );
        let feature = mk_feature("Missing", "email", vec![resource]);
        let findings = check(&feature, Path::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::ResourceNotFound);
        assert!(findings[0].message().contains("resource not found"));
    }

    #[test]
    fn negative_does_not_fire_on_semantic_email_field() {
        let resource = mk_resource(
            "Customer",
            vec![mk_field(
                "email",
                TypeRef::Builtin(BuiltinType::SemanticEmail),
                false,
            )],
        );
        let feature = mk_feature("Customer", "email", vec![resource]);
        let findings = check(&feature, Path::new("x.lzi"));
        assert!(findings.is_empty(), "got: {findings:?}");
    }

    #[test]
    fn edge_fires_when_field_is_non_unique_text() {
        // The field exists on the resource but is plain Text without
        // unique — not identity-shaped.
        let resource = mk_resource(
            "Customer",
            vec![mk_field("note", TypeRef::Builtin(BuiltinType::Text), false)],
        );
        let feature = mk_feature("Customer", "note", vec![resource]);
        let findings = check(&feature, Path::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::FieldNotIdentityShaped);
        assert!(findings[0].message().contains("identity-shaped"));
    }
}
