//! LIFECYCLE-ENUM-DUPLICATE — generated lifecycle enum collides with sibling enum.
//!
//! Fires when `lifecycle.generated_enum` matches an authored `enum <Name>` in the
//! same feature.
//!
//! Severity: `error` (strict), `error` (production).
//! Reference: docs/proposals/lifecycle-vocab.md §5

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

/// One LIFECYCLE-ENUM-DUPLICATE finding — the enum a lifecycle would
/// generate (e.g. `<Resource>Status`) collides with an authored
/// `enum <Name>` in the same feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the lifecycle is declared in.
    pub path: PathBuf,
    /// Resource owning the lifecycle.
    pub resource: String,
    /// Name of the colliding enum.
    pub enum_name: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "LIFECYCLE-ENUM-DUPLICATE";

    /// Render the "lifecycle enum collides with sibling enum" message.
    /// Names both the resource and the enum so authors can decide
    /// whether to rename the discriminator field or the existing enum.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::lifecycle::enum_duplicate::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("publishing.lzi"),
    ///     resource: "Publication".into(),
    ///     enum_name: "PublicationStatus".into(),
    /// };
    /// assert!(f.message().contains("PublicationStatus"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}` auto-emits enum `{}` but a sibling `enum {}` is already declared — rename your existing enum OR rename the lifecycle discriminator field",
            self.resource, self.enum_name, self.enum_name
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Walk `feature` and flag any resource whose lifecycle would generate
/// an enum that collides with an authored `enum <Name>` in the same
/// feature.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::lifecycle::enum_duplicate::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a lifecycle + sibling enum collision");
/// let _ = check(&feature, Path::new("publishing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let declared: HashSet<&str> = feature.enums.iter().map(|e| e.name.as_str()).collect();
    let mut findings = vec![];

    for r in &feature.resources {
        let Some(lc) = r.lifecycle.as_ref() else {
            continue;
        };
        if declared.contains(lc.generated_enum.as_str()) {
            findings.push(Finding {
                path: path.to_path_buf(),
                resource: r.name.clone(),
                enum_name: lc.generated_enum.clone(),
            });
        }
    }

    findings
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, EnumDecl, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, Resource,
    };

    fn mk_feature_with_lifecycle(resource_name: &str, lc: Lifecycle) -> Feature {
        let resource = Resource {
            name: resource_name.into(),
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
            lifecycle: Some(lc),
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        };
        Feature {
            name: "test_feat".into(),
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
            resources: vec![resource],
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

    fn mk_state(name: &str, kind: LifecycleStateKind) -> LifecycleState {
        LifecycleState {
            name: name.into(),
            kind,
            span_ref: None,
        }
    }

    fn mk_transition(name: &str, from: Vec<&str>, to: &str) -> LifecycleTransition {
        LifecycleTransition {
            name: name.into(),
            from: from.iter().map(|s| s.to_string()).collect(),
            to: to.into(),
            policy: None,
            audit: None,
            timestamps: None,
            emits: vec![],
            requires: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_lifecycle(generated_enum: &str) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: generated_enum.into(),
            states: vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            transitions: vec![mk_transition("publish", vec!["draft"], "published")],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_enum(name: &str) -> EnumDecl {
        EnumDecl {
            name: name.into(),
            public_contract: None,
            variants: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn positive_enum_collision_fires() {
        let mut feature =
            mk_feature_with_lifecycle("Publication", mk_lifecycle("PublicationStatus"));
        feature.enums = vec![mk_enum("PublicationStatus")];

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].enum_name, "PublicationStatus");
        assert_eq!(Finding::CODE, "LIFECYCLE-ENUM-DUPLICATE");
        assert!(findings[0].message().contains("PublicationStatus"));
    }

    #[test]
    fn negative_no_collision_does_not_fire() {
        let mut feature =
            mk_feature_with_lifecycle("Publication", mk_lifecycle("PublicationStatus"));
        feature.enums = vec![mk_enum("ReviewStatus")];

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert!(findings.is_empty());
    }
}
