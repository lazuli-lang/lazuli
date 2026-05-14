//! LIFECYCLE-UNREACHABLE-STATE — non-initial lifecycle state with no incoming transition.
//!
//! Fires when a lifecycle state is not marked `initial` and no transition targets it.
//!
//! Severity: warning (strict), error (production).
//! Reference: docs/proposals/lifecycle-vocab.md §5

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Lifecycle, LifecycleStateKind, Resource};

// ── output ────────────────────────────────────────────────────────────────────

/// One LIFECYCLE-UNREACHABLE-STATE finding: an orphan non-initial state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource that owns the lifecycle block.
    pub resource: String,
    /// State with no incoming transition.
    pub state_name: String,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-UNREACHABLE-STATE";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}`: non-initial state `{}` has no incoming transitions — \
             declare it `initial` or add a transition with `to {}`",
            self.resource, self.state_name, self.state_name
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run LIFECYCLE-UNREACHABLE-STATE for all resource lifecycles in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here. The caller maps each `Finding` into a `DoctorDiagnostic`.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .filter_map(|resource| resource.lifecycle.as_ref().map(|lc| (resource, lc)))
        .flat_map(|(resource, lc)| check_lifecycle(resource, lc, path))
        .collect()
}

fn check_lifecycle(resource: &Resource, lifecycle: &Lifecycle, path: &Path) -> Vec<Finding> {
    let reached: HashSet<&str> = lifecycle
        .transitions
        .iter()
        .map(|transition| transition.to.as_str())
        .collect();

    lifecycle
        .states
        .iter()
        .filter(|state| !matches!(state.kind, LifecycleStateKind::Initial))
        .filter(|state| !reached.contains(state.name.as_str()))
        .map(|state| Finding {
            path: path.to_path_buf(),
            resource: resource.name.clone(),
            state_name: state.name.clone(),
        })
        .collect()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, Resource,
    };

    fn mk_feature_with_lifecycle(resource_name: &str, lifecycle: Lifecycle) -> Feature {
        let resource = Resource {
            name: resource_name.into(),
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
        };

        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![resource],
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
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],            previous_names: vec![],
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
            invariants: vec![],
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
    fn positive_orphan_state_fires() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
                mk_state("lost_state", LifecycleStateKind::Intermediate),
            ],
            vec![mk_transition("publish", vec!["draft"], "published")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].state_name, "lost_state");
        assert_eq!(Finding::CODE, "LIFECYCLE-UNREACHABLE-STATE");
        assert!(
            findings[0].message().contains("to lost_state"),
            "message should name the missing transition target"
        );
    }

    #[test]
    fn negative_initial_state_does_not_fire() {
        let lifecycle = mk_lifecycle(vec![mk_state("draft", LifecycleStateKind::Initial)], vec![]);
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_reached_state_does_not_fire() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("publish", vec!["draft"], "published")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
