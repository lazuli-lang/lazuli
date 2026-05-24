//! LIFECYCLE-TRANSITION-TO-UNDECLARED — transition target state is undeclared.
//!
//! Fires when a lifecycle transition's `to` references a state not declared
//! in the same lifecycle.
//!
//! Severity: `error` (strict), `error` (production).
//! Reference: docs/proposals/lifecycle-vocab.md §5

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

/// One LIFECYCLE-TRANSITION-TO-UNDECLARED finding: a lifecycle transition
/// with a `to` state outside the lifecycle's declared state set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource carrying the lifecycle.
    pub resource: String,
    /// Transition with the unresolved `to` target.
    pub transition: String,
    /// State name referenced by `transition.to` but not declared.
    pub unresolved_state: String,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-TRANSITION-TO-UNDECLARED";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}`: transition `{}` targets undeclared state `{}` via `to`",
            self.resource, self.transition, self.unresolved_state
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run LIFECYCLE-TRANSITION-TO-UNDECLARED over one feature's resources.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        let declared: HashSet<&str> = lifecycle
            .states
            .iter()
            .map(|state| state.name.as_str())
            .collect();

        for transition in &lifecycle.transitions {
            if !declared.contains(transition.to.as_str()) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    resource: resource.name.clone(),
                    transition: transition.name.clone(),
                    unresolved_state: transition.to.clone(),
                });
            }
        }
    }

    findings
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, Lifecycle, LifecycleInvariant, LifecycleState, LifecycleStateKind,
        LifecycleTransition, Policies, Resource,
    };

    fn mk_feature_with_lifecycle(resource_name: &str, lifecycle: Lifecycle) -> Feature {
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
        };

        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
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

    fn mk_lifecycle(
        states: Vec<LifecycleState>,
        transitions: Vec<LifecycleTransition>,
    ) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states,
            transitions,
            invariants: Vec::<LifecycleInvariant>::new(),
            invariant_handlers: vec![],
            previous_names: vec![],
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
            from: from.iter().map(|state| state.to_string()).collect(),
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

    #[test]
    fn positive_unknown_to_state_fires() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("publish", vec!["draft"], "publisheddd")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].transition, "publish");
        assert_eq!(findings[0].unresolved_state, "publisheddd");
        assert_eq!(Finding::CODE, "LIFECYCLE-TRANSITION-TO-UNDECLARED");
        assert_eq!(
            findings[0].message(),
            "lifecycle on `Publication`: transition `publish` targets undeclared state `publisheddd` via `to`"
        );
    }

    #[test]
    fn negative_resolved_to_does_not_fire() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("publish", vec!["draft"], "published")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert!(findings.is_empty());
    }
}
