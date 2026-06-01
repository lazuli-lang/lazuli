//! auth_password_algorithm_hash_mismatch_001 — `auth password algorithm <X>`
//! diverges from `@cap.Hashed(algorithm:<Y>)` on the session resource's
//! hash-shaped field.
//!
//! When a feature authors both `auth password algorithm <X>` and an
//! `auth sessions resource <Resource>` whose `Resource` carries a field
//! decorated with `@cap.Hashed(algorithm:<Y>)`, the two axes MUST agree.
//! Otherwise the runtime password verifier and the on-disk refresh-token
//! hash drift apart and login fails silently.
//!
//! Severity: `error` (production + strict). This is a correctness bug,
//! not style drift.
//!
//! Reference: docs/proposals/auth-lowering-scope.md §"Closed-cycle criterion"
//! Reference: docs/proposals/bucket-auth-cycle.md §IR.cross-refs

use std::path::{Path, PathBuf};

use lazuli_ir::{CapabilityRef, Feature, HashAlgorithm, TypeRef};

// ── output ────────────────────────────────────────────────────────────────────

/// One auth_password_algorithm_hash_mismatch_001 finding: the authored
/// algorithm string and the resource's hash decorator algorithm diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    /// Verbatim `auth password algorithm <X>` value.
    pub password_algorithm: String,
    /// Session resource name (`auth sessions resource <X>`).
    pub session_resource: String,
    /// Hash-shaped field on the session resource.
    pub session_field: String,
    /// Algorithm read from `@cap.Hashed(algorithm:<Y>)`.
    pub resource_algorithm: String,
}

impl Finding {
    /// Stable doctor rule code surfaced to the user.
    pub const CODE: &'static str = "auth_password_algorithm_hash_mismatch_001";

    /// Render the remediation message naming both authored algorithms.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // let msg = finding.message();
    /// ```
    pub fn message(&self) -> String {
        format!(
            "auth.password.algorithm `{}` must match `@cap.Hashed(algorithm:{})` \
             on the session resource's hash field (found `{}` on `{}.{}`).",
            self.password_algorithm,
            self.password_algorithm,
            self.resource_algorithm,
            self.session_resource,
            self.session_field,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run auth_password_algorithm_hash_mismatch_001 on a single feature.
///
/// No I/O is performed here. The caller (doctor walker) emits the
/// `DoctorDiagnostic` using `Finding::CODE` + `Finding::message()`.
///
/// Same-feature resolution only: cross-feature `auth sessions resource X`
/// where `X` lives in a feature this one `uses` is the integration
/// pipeline's responsibility (see `doctor.rs::auth_diagnostics`).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_ir::Feature;
/// use lazuli_cli::doctor::auth::auth_password_algorithm_hash_mismatch_001::check;
///
/// // let findings = check(&feature, Path::new("app.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let Some(auth) = feature.auth.as_ref() else {
        return Vec::new();
    };
    let Some(password) = auth.password.as_ref() else {
        return Vec::new();
    };
    let Some(sessions) = auth.sessions.as_ref() else {
        return Vec::new();
    };
    let pw_algo = password.algorithm.trim();
    if pw_algo.is_empty() {
        return Vec::new();
    }

    let session_resource_name = sessions.resource.name.as_str();
    let Some(resource) = feature
        .resources
        .iter()
        .find(|r| r.name == session_resource_name)
    else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for field in &resource.fields {
        if let Some(axis) = cap_hashed_algorithm(&field.type_ref)
            && axis != pw_algo
        {
            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                password_algorithm: pw_algo.to_owned(),
                session_resource: session_resource_name.to_owned(),
                session_field: field.name.clone(),
                resource_algorithm: axis.to_owned(),
            });
            break;
        }
    }
    out
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Read the `algorithm:<X>` axis out of a typed `@cap.Hashed(...)`. Mirrors
/// `doctor.rs::cap_hashed_algorithm` but lives here so the rule module is
/// self-contained.
fn cap_hashed_algorithm(type_ref: &TypeRef) -> Option<&'static str> {
    match type_ref {
        TypeRef::Capability(CapabilityRef::Hashed(h)) => Some(match h.algorithm {
            HashAlgorithm::Argon2id => "argon2id",
            HashAlgorithm::Bcrypt => "bcrypt",
        }),
        _ => None,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use lazuli_ir::{
        Auth, AuthIdentity, AuthPassword, AuthSessions, BuiltinType, Defaults, Feature, Field,
        FieldConstraints, FieldRef, HashedCapability, Policies, QualifiedName, Resource,
    };

    use super::*;

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn hashed(algorithm: HashAlgorithm) -> TypeRef {
        TypeRef::Capability(CapabilityRef::Hashed(HashedCapability { algorithm }))
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
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn mk_session_resource(hash_algorithm: HashAlgorithm) -> Resource {
        Resource {
            name: "CustomerSession".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![
                mk_field("refresh_token_hash", hashed(hash_algorithm)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
        }
    }

    fn mk_feature(password_algorithm: &str, session_resource: Resource) -> Feature {
        Feature {
            name: "customer_auth".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![session_resource],
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
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            auth: Some(Auth {
                identity: AuthIdentity {
                    field: FieldRef {
                        resource: qn("CustomerSession"),
                        field: "email".to_owned(),
                    },
                    public_contract: None,
                },
                password: Some(AuthPassword {
                    algorithm: password_algorithm.to_owned(),
                    hash: "@fn.h".to_owned(),
                    verify: "@fn.v".to_owned(),
                    rate_limit: None,
                }),
                sessions: Some(AuthSessions {
                    resource: qn("CustomerSession"),
                    ttl: "7 days".to_owned(),
                    refresh: false,
                    extra_columns: vec![],
                    access_ttl: None,
                    rotation: None,
                    cookie: None,
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
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_fires_when_password_bcrypt_diverges_from_resource_argon2id() {
        let feature = mk_feature("bcrypt", mk_session_resource(HashAlgorithm::Argon2id));
        let findings = check(&feature, Path::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "auth_password_algorithm_hash_mismatch_001");
        assert_eq!(findings[0].password_algorithm, "bcrypt");
        assert_eq!(findings[0].resource_algorithm, "argon2id");
        assert_eq!(findings[0].session_field, "refresh_token_hash");
        assert!(findings[0].message().contains("bcrypt"));
        assert!(findings[0].message().contains("argon2id"));
    }

    #[test]
    fn negative_does_not_fire_when_algorithms_agree() {
        let feature = mk_feature("argon2id", mk_session_resource(HashAlgorithm::Argon2id));
        let findings = check(&feature, Path::new("x.lzi"));
        assert!(
            findings.is_empty(),
            "agreement must not fire; got {findings:?}"
        );
    }

    #[test]
    fn edge_no_hash_field_does_not_fire() {
        // Session resource carries no `@cap.Hashed` field at all — the
        // diagnostic has no axis to read, so it must stay silent rather
        // than guess.
        let resource = Resource {
            name: "CustomerSession".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![
                mk_field("token", TypeRef::Builtin(BuiltinType::Text)),
                mk_field("expires_at", TypeRef::Builtin(BuiltinType::DateTime)),
            ],
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
            lifecycle: None,
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
        };
        let feature = mk_feature("bcrypt", resource);
        let findings = check(&feature, Path::new("x.lzi"));
        assert!(findings.is_empty());
    }
}
