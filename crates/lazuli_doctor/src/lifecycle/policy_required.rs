//! LIFECYCLE-POLICY-REQUIRED — lifecycle transition with no resolvable policy.
//!
//! Fires when a `lifecycle <field>` transition declares no `policy`
//! directive AND the parent feature has no `defaults.policy` to fall
//! back on. Every lifecycle-lowered command writes to the resource, so
//! an unguarded transition would expose a public write with no authn
//! gate — the rule blocks that drift at audit time.
//!
//! Severity: error (strict), error (production). Matches the table in
//! `docs/proposals/lifecycle-vocab.md` §5.
//!
//! Opt-out: `# doctor:allow LIFECYCLE-POLICY-REQUIRED` on the source
//! `.lzi` skips the check (intended only for synthetic fixtures where
//! the parent feature is constructed without a defaults block).
//!
//! Example trigger fixture:
//!
//! ```text
//! resource Publication
//!   lifecycle status
//!     state draft initial
//!     state published terminal
//!     transition publish     # <-- no `policy` AND no feature default
//!       from draft
//!       to published
//! ```
//!
//! Reference: docs/proposals/lifecycle-vocab.md §3.3, §5.

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

/// One LIFECYCLE-POLICY-REQUIRED finding — a transition with no policy
/// chain that resolves to an authn/authz gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the lifecycle is declared in.
    pub path: PathBuf,
    /// Resource owning the lifecycle.
    pub resource: String,
    /// Transition that lacks a resolvable policy.
    pub transition: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "LIFECYCLE-POLICY-REQUIRED";

    /// Render the "no policy" message, steering the author either to a
    /// per-transition `policy @policy.<name>` clause or a feature-level
    /// `defaults` block.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::lifecycle::policy_required::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("publishing.lzi"),
    ///     resource: "Publication".into(),
    ///     transition: "publish".into(),
    /// };
    /// assert!(f.message().contains("publish"));
    /// assert!(f.message().contains("defaults.policy"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}`: transition `{}` declares no `policy` and the feature has no \
             `defaults.policy` to inherit — every lifecycle-lowered command must have a \
             resolvable policy (add `policy @policy.<name>` to the transition or a \
             `defaults` block with `policy @policy.<name>` to the feature)",
            self.resource, self.transition
        )
    }
}

/// Walk every resource lifecycle in `feature` and flag every transition
/// whose `policy` slot is `None` AND the parent feature has no
/// `defaults.policy` fallback.
///
/// Honors `# doctor:allow LIFECYCLE-POLICY-REQUIRED` on the source file:
/// when present, the rule short-circuits to an empty finding list.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::lifecycle::policy_required::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with lifecycle transitions");
/// let _ = check(&feature, Path::new("publishing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }

    let feature_has_default_policy = feature
        .defaults
        .policy
        .as_ref()
        .is_some_and(|p| !p.is_none());
    let mut findings = Vec::new();

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        for transition in &lifecycle.transitions {
            let transition_has_policy = transition.policy.as_ref().is_some_and(|p| !p.is_none());
            if !transition_has_policy && !feature_has_default_policy {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    resource: resource.name.clone(),
                    transition: transition.name.clone(),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, PolicyRef, Resource,
    };

    fn mk_state(name: &str, kind: LifecycleStateKind) -> LifecycleState {
        LifecycleState {
            name: name.into(),
            kind,
            span_ref: None,
        }
    }

    fn mk_transition(name: &str, policy: Option<PolicyRef>) -> LifecycleTransition {
        LifecycleTransition {
            name: name.into(),
            from: vec!["draft".into()],
            to: "published".into(),
            policy,
            audit: None,
            timestamps: None,
            emits: vec![],
            requires: None,
            tests: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_lifecycle(transitions: Vec<LifecycleTransition>) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states: vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            transitions,
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(resource_name: &str, lifecycle: Lifecycle, defaults: Defaults) -> Feature {
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
            lifecycle: Some(lifecycle),
            invariants: vec![],
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
            append_only: false,
        };
        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults,
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

    fn mk_policy_ref(name: &str) -> PolicyRef {
        PolicyRef::Atom(format!("@policy.{}", name))
    }

    #[test]
    fn negative_transition_with_explicit_policy_does_not_fire() {
        let lifecycle = mk_lifecycle(vec![mk_transition(
            "publish",
            Some(mk_policy_ref("publisher")),
        )]);
        let feature = mk_feature("Publication", lifecycle, Defaults::default());

        assert!(check(&feature, Path::new("publishing.lzi")).is_empty());
    }

    #[test]
    fn negative_transition_inherits_feature_default_policy() {
        let lifecycle = mk_lifecycle(vec![mk_transition("publish", None)]);
        let defaults = Defaults {
            policy: Some(mk_policy_ref("admin")),
            ..Default::default()
        };
        let feature = mk_feature("Publication", lifecycle, defaults);

        assert!(check(&feature, Path::new("publishing.lzi")).is_empty());
    }

    #[test]
    fn positive_no_policy_and_no_default_fires() {
        let lifecycle = mk_lifecycle(vec![mk_transition("publish", None)]);
        let feature = mk_feature("Publication", lifecycle, Defaults::default());

        let findings = check(&feature, Path::new("publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].transition, "publish");
        assert_eq!(Finding::CODE, "LIFECYCLE-POLICY-REQUIRED");
        assert!(findings[0].message().contains("publish"));
        assert!(findings[0].message().contains("Publication"));
        assert!(findings[0].message().contains("defaults.policy"));
    }

    #[test]
    fn positive_multiple_transitions_each_fires() {
        let lifecycle = mk_lifecycle(vec![
            mk_transition("publish", None),
            mk_transition("archive", None),
        ]);
        let feature = mk_feature("Publication", lifecycle, Defaults::default());

        let findings = check(&feature, Path::new("publishing.lzi"));

        assert_eq!(findings.len(), 2);
        let names: Vec<_> = findings.iter().map(|f| f.transition.as_str()).collect();
        assert!(names.contains(&"publish"));
        assert!(names.contains(&"archive"));
    }

    #[test]
    fn positive_policy_ref_none_treated_as_no_policy() {
        // Some(PolicyRef::None) is the explicit "feature-default applies"
        // marker — with no feature default, it's equivalent to "no policy".
        let lifecycle = mk_lifecycle(vec![mk_transition("publish", Some(PolicyRef::None))]);
        let feature = mk_feature("Publication", lifecycle, Defaults::default());

        let findings = check(&feature, Path::new("publishing.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].transition, "publish");
    }

    #[test]
    fn opt_out_doctor_allow_comment_suppresses_finding() {
        let lifecycle = mk_lifecycle(vec![mk_transition("publish", None)]);
        let feature = mk_feature("Publication", lifecycle, Defaults::default());

        let tmp_dir = std::env::temp_dir();
        let tmp_path = tmp_dir.join("lifecycle_policy_required_opt_out.lzi");
        std::fs::write(
            &tmp_path,
            "# doctor:allow LIFECYCLE-POLICY-REQUIRED\nfeature x\n",
        )
        .expect("write temp opt-out file");

        let findings = check(&feature, &tmp_path);

        let _ = std::fs::remove_file(&tmp_path);
        assert!(
            findings.is_empty(),
            "doctor:allow comment must suppress the finding"
        );
    }
}
