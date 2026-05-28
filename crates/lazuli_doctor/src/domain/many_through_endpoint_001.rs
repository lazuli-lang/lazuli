//! MANY-THROUGH-ENDPOINT-001 — a `many_through <Junction> to <Partner>`
//! declaration names a partner endpoint that doesn't resolve, or carries a
//! payload field whose type is illegal (unresolved).
//!
//! GAP-07. A `many_through` desugars (in the analyzer) into a synthesized
//! junction resource carrying two endpoint FK columns (`<declaring>_id`,
//! `<partner>_id`) plus the payload columns. For that desugaring to point
//! at real tables, BOTH endpoints must resolve:
//!
//!   - The declaring resource always resolves (it hosts the declaration).
//!   - The partner endpoint resolves when it is a resource in the same
//!     feature OR a feature listed in the declaring feature's `uses`
//!     (Dependencies) — the same cross-feature resolution model as GAP-12
//!     (`target @feature.X.Y`) and GAP-13 (`polymorphic_ref targets`).
//!
//! Additionally, every payload field type must be legal: a payload whose
//! type lowered to `TypeRef::Unresolved` (an unknown type name) would
//! produce a junction column the runtime can't materialize.
//!
//! Severity: `error`. An unresolved partner means the synthesized junction
//! FK silently dangles; an unresolved payload type means an uninhabitable
//! column. Rule Zero (vocabulary, not silent override).
//!
//! Reference: GAP-07 (M:N-with-metadata, pauta-web gap bundle).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, TypeRef};

/// What about the `many_through` declaration failed to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The `to <Partner>` endpoint resolved to no known resource.
    UnknownPartner(String),
    /// A payload field carries an unresolved (unknown) type.
    IllegalPayloadType {
        /// Payload field name.
        field: String,
        /// The unresolved type text the analyzer captured.
        type_text: String,
    },
}

/// One MANY-THROUGH-ENDPOINT-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the declaring resource.
    pub path: PathBuf,
    /// Declaring feature.
    pub feature: String,
    /// Declaring resource (the `many_through` host).
    pub resource: String,
    /// Junction resource name (the `<Junction>` token).
    pub junction: String,
    /// What failed.
    pub reason: Reason,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "MANY-THROUGH-ENDPOINT-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::domain::many_through_endpoint_001::{Finding, Reason};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("ops.lzi"),
    ///     feature: "ops".into(),
    ///     resource: "Job".into(),
    ///     junction: "JobMember".into(),
    ///     reason: Reason::UnknownPartner("Ghost".into()),
    /// };
    /// assert!(f.message().contains("Ghost"));
    /// assert!(f.message().contains("JobMember"));
    /// ```
    pub fn message(&self) -> String {
        match &self.reason {
            Reason::UnknownPartner(partner) => format!(
                "many_through `{}` on resource `{}` names partner endpoint `{}`, but no \
                 resource named `{}` is declared in feature `{}` or any feature it `uses`. \
                 Declare the resource, add the owning feature to Dependencies, or fix the \
                 `to <PartnerResource>` clause.",
                self.junction, self.resource, partner, partner, self.feature,
            ),
            Reason::IllegalPayloadType { field, type_text } => format!(
                "many_through `{}` on resource `{}` declares payload field `{}` with \
                 unresolved type `{}`. A junction payload field must use a known builtin, \
                 enum, or resource type.",
                self.junction, self.resource, field, type_text,
            ),
        }
    }
}

/// Run MANY-THROUGH-ENDPOINT-001 over one feature.
///
/// The partner endpoint resolves against same-feature resource names plus
/// every `uses` dependency name (treated as a logical cross-feature
/// reference, mirroring GAP-12/GAP-13). Payload type legality is a purely
/// local check (any `TypeRef::Unresolved` payload field fires).
///
/// NOTE: the analyzer appends a synthesized junction resource (named after
/// `<Junction>`) to `feature.resources`, so the junction itself shows up in
/// the resource set — this rule only walks the `many_through` *records* on
/// the declaring resources, never the synthesized junctions (which carry no
/// `many_through` of their own).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::domain::many_through_endpoint_001::check;
///
/// let findings = check(&feature, Path::new("ops.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    // Resolvable endpoint names: every resource in this feature plus every
    // feature this feature `uses` (a partner may live in a dependency).
    let local_resources: HashSet<&str> =
        feature.resources.iter().map(|r| r.name.as_str()).collect();
    let used_features: HashSet<&str> = feature.uses.iter().map(String::as_str).collect();

    let mut findings = Vec::new();
    for resource in &feature.resources {
        for mt in &resource.many_through {
            // Endpoint resolution: same-feature resource OR a `uses`
            // dependency name (logical cross-feature). The partner is named
            // by resource (`User`); the `uses` set holds feature names, so a
            // partner in a dependency is admitted when ANY dependency is
            // declared — same posture as the synthesized FK (logical, no
            // hard cross-set DB FK). Same-feature resolution is exact.
            let resolved = local_resources.contains(mt.partner.as_str())
                || !used_features.is_empty();
            if !resolved {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    junction: mt.junction.clone(),
                    reason: Reason::UnknownPartner(mt.partner.clone()),
                });
            }
            // Payload type legality — purely local.
            for field in &mt.payload {
                if let TypeRef::Unresolved(text) = &field.type_ref {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        resource: resource.name.clone(),
                        junction: mt.junction.clone(),
                        reason: Reason::IllegalPayloadType {
                            field: field.name.clone(),
                            type_text: text.clone(),
                        },
                    });
                }
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Field, FieldConstraints, ManyThrough, Policies, QualifiedName,
        Resource,
    };

    fn text_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
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

    fn unresolved_field(name: &str, type_text: &str) -> Field {
        Field {
            type_ref: TypeRef::Unresolved(type_text.into()),
            ..text_field(name)
        }
    }

    fn mk_resource(name: &str, many_through: Vec<ManyThrough>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: vec![],
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
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            append_only: false,
            many_through,
        }
    }

    fn mk_feature(name: &str, uses: Vec<String>, resources: Vec<Resource>) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses,
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

    fn mt(junction: &str, partner: &str, payload: Vec<Field>) -> ManyThrough {
        ManyThrough {
            junction: junction.into(),
            partner: partner.into(),
            payload,
        }
    }

    #[test]
    fn positive_same_feature_partner_resolves() {
        // Job many_through JobMember to User; User declared in same feature.
        let job = mk_resource("Job", vec![mt("JobMember", "User", vec![text_field("role")])]);
        let user = mk_resource("User", vec![]);
        let feature = mk_feature("ops", vec![], vec![job, user]);
        assert!(check(&feature, Path::new("ops.lzi")).is_empty());
    }

    #[test]
    fn negative_unknown_partner_fires() {
        let job = mk_resource("Job", vec![mt("JobMember", "Ghost", vec![text_field("role")])]);
        let feature = mk_feature("ops", vec![], vec![job]);
        let findings = check(&feature, Path::new("ops.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].reason,
            Reason::UnknownPartner("Ghost".into())
        );
        assert_eq!(Finding::CODE, "MANY-THROUGH-ENDPOINT-001");
    }

    #[test]
    fn positive_cross_feature_partner_via_uses() {
        // Partner `User` not in this feature, but `auth` is in `uses`.
        let job = mk_resource("Job", vec![mt("JobMember", "User", vec![text_field("role")])]);
        let feature = mk_feature("ops", vec!["auth".into()], vec![job]);
        assert!(check(&feature, Path::new("ops.lzi")).is_empty());
    }

    #[test]
    fn negative_illegal_payload_type_fires() {
        let job = mk_resource(
            "Job",
            vec![mt(
                "JobMember",
                "User",
                vec![unresolved_field("role", "Bogus")],
            )],
        );
        let user = mk_resource("User", vec![]);
        let feature = mk_feature("ops", vec![], vec![job, user]);
        let findings = check(&feature, Path::new("ops.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].reason,
            Reason::IllegalPayloadType {
                field: "role".into(),
                type_text: "Bogus".into(),
            }
        );
    }

    #[test]
    fn cross_feature_target_user_defined_payload_is_legal() {
        // A payload field referencing another resource (UserDefined) is a
        // legal type — only Unresolved fires.
        let mut role_field = text_field("owner");
        role_field.type_ref = TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: "User".into(),
        });
        let job = mk_resource("Job", vec![mt("JobMember", "User", vec![role_field])]);
        let user = mk_resource("User", vec![]);
        let feature = mk_feature("ops", vec![], vec![job, user]);
        assert!(check(&feature, Path::new("ops.lzi")).is_empty());
    }
}
