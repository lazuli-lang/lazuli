//! LIFECYCLE-STATE-DUPLICATE — duplicate state names in one lifecycle block.
//!
//! Fires when a `lifecycle` block declares two or more `state` entries with
//! the same name.
//!
//! Severity: `error` (strict), `error` (production).
//! Reference: docs/proposals/lifecycle-vocab.md §5

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

/// One LIFECYCLE-STATE-DUPLICATE finding: a repeated state name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the resource that owns the offending lifecycle block.
    pub resource: String,
    /// Repeated state name.
    pub state_name: String,
    /// Number of times the state name appears in the lifecycle block.
    pub occurrences: usize,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-STATE-DUPLICATE";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on resource `{}` declares state `{}` {} times — state names must be unique",
            self.resource, self.state_name, self.occurrences
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run LIFECYCLE-STATE-DUPLICATE for all resource lifecycles in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here. The caller (doctor walker) maps each `Finding` into a
/// `DoctorDiagnostic`.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        let mut counts: HashMap<&str, usize> = HashMap::new();
        for state in &lifecycle.states {
            *counts.entry(state.name.as_str()).or_insert(0) += 1;
        }

        for (state_name, occurrences) in counts {
            if occurrences >= 2 {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    resource: resource.name.clone(),
                    state_name: state_name.to_owned(),
                    occurrences,
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
        Defaults, Feature, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, Resource,
    };

    fn mk_feature_with_lifecycle(resource_name: &str, lc: Lifecycle) -> Feature {
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
            lifecycle: Some(lc),
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
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_lifecycle(states: Vec<LifecycleState>) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states,
            transitions: vec![],
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

    #[allow(dead_code)]
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

    #[test]
    fn positive_duplicate_state_fires() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            mk_lifecycle(vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("scheduled", LifecycleStateKind::Terminal),
            ]),
        );

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].state_name, "scheduled");
        assert_eq!(findings[0].occurrences, 2);
        assert_eq!(Finding::CODE, "LIFECYCLE-STATE-DUPLICATE");
        assert!(
            findings[0].message().contains("Publication"),
            "message should name the resource"
        );
    }

    #[test]
    fn negative_unique_states_do_not_fire() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            mk_lifecycle(vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("scheduled", LifecycleStateKind::Intermediate),
                mk_state("published", LifecycleStateKind::Terminal),
            ]),
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn positive_three_duplicates_fires_with_correct_count() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            mk_lifecycle(vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("scheduled", LifecycleStateKind::Intermediate),
                mk_state("scheduled", LifecycleStateKind::Terminal),
            ]),
        );

        let findings = check(&feature, Path::new("f.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].state_name, "scheduled");
        assert_eq!(findings[0].occurrences, 3);
    }
}
