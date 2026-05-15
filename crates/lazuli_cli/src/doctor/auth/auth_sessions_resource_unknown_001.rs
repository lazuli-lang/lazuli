//! auth_sessions_resource_unknown_001 — `auth sessions resource <X>`
//! does not name a resource declared in the same feature.
//!
//! Same-feature resolution only: cross-feature resolution through the
//! `uses` graph is the integration walker's responsibility in
//! `doctor.rs::auth_diagnostics`. This module enforces the local axis
//! so unit tests can pin the contract.
//!
//! Severity: `error`. Sessions referencing a non-existent resource
//! crash the runtime emitter at codegen time.
//!
//! Reference: docs/proposals/auth-lowering-scope.md §"Closed-cycle criterion"
//! Reference: docs/proposals/bucket-auth-cycle.md §IR.cross-refs

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    /// Resource name authored in `auth sessions resource <X>`.
    pub session_resource: String,
}

impl Finding {
    pub const CODE: &'static str = "auth_sessions_resource_unknown_001";

    pub fn message(&self) -> String {
        format!(
            "auth.sessions.resource `{}` does not name a resource declared in feature `{}`.",
            self.session_resource, self.feature,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run auth_sessions_resource_unknown_001 on a single feature.
///
/// Returns a single finding (per spec — at most one `auth sessions` block
/// per feature) when the resource name does not resolve locally. Empty
/// otherwise — same-feature resolution; `uses`-relative resolution is
/// handled by the integration walker.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let Some(auth) = feature.auth.as_ref() else {
        return Vec::new();
    };
    let Some(sessions) = auth.sessions.as_ref() else {
        return Vec::new();
    };
    let name = sessions.resource.name.as_str();
    let known = feature.resources.iter().any(|r| r.name == name);
    if known {
        return Vec::new();
    }
    vec![Finding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
        session_resource: name.to_owned(),
    }]
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Auth, AuthIdentity, AuthSessions, BuiltinType, Defaults, Feature, Field, FieldConstraints,
        FieldRef, Policies, QualifiedName, Resource, TypeRef,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
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
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_resource(name: &str) -> Resource {
        Resource {
            name: name.to_owned(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![
                mk_field("id", TypeRef::Builtin(BuiltinType::Id)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            invariants: vec![],
        }
    }

    fn mk_feature(session_resource_name: &str, resources: Vec<Resource>) -> Feature {
        Feature {
            name: "customer_auth".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
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
                        resource: qn(session_resource_name),
                        field: "email".to_owned(),
                    },
                },
                password: None,
                sessions: Some(AuthSessions {
                    resource: qn(session_resource_name),
                    ttl: "7 days".to_owned(),
                    refresh: false,
                    extra_columns: vec![],
                }),
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
    fn positive_fires_when_session_resource_not_declared() {
        let feature = mk_feature("MissingSession", vec![mk_resource("CustomerSession")]);
        let findings = check(&feature, Path::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "auth_sessions_resource_unknown_001");
        assert_eq!(findings[0].session_resource, "MissingSession");
        assert!(findings[0].message().contains("MissingSession"));
        assert!(findings[0].message().contains("customer_auth"));
    }

    #[test]
    fn negative_does_not_fire_when_session_resource_declared_locally() {
        let feature = mk_feature("CustomerSession", vec![mk_resource("CustomerSession")]);
        let findings = check(&feature, Path::new("x.lzi"));
        assert!(findings.is_empty(), "got: {findings:?}");
    }

    #[test]
    fn edge_no_sessions_block_does_not_fire() {
        // Auth declared without `sessions` block — no axis to check.
        let feature = Feature {
            name: "no_sessions".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
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
                        resource: qn("Customer"),
                        field: "email".to_owned(),
                    },
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
        };
        let findings = check(&feature, Path::new("x.lzi"));
        assert!(findings.is_empty());
    }
}
