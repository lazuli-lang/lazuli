//! LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION - terminal state used as a transition source.
//!
//! Fires when a state marked `terminal` appears in any lifecycle transition's
//! `from` list.
//!
//! Severity: `error` (strict), `error` (production).
//! Reference: docs/proposals/lifecycle-vocab.md §5

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, LifecycleStateKind};

// output

/// One finding for a terminal state that still has an outgoing transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the resource that owns the lifecycle.
    pub resource: String,
    /// Terminal state used as a transition source.
    pub terminal_state: String,
    /// Transition that declares the terminal state in `from`.
    pub transition: String,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}`: state `{}` is `terminal` but transition `{}` uses it as `from`. \
             Terminal states must have no outgoing transitions.",
            self.resource, self.terminal_state, self.transition
        )
    }
}

// detection

/// Run LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION for all resource lifecycles
/// in one feature.
///
/// `path` is the source `.lzi` file used to anchor findings; no I/O is
/// performed here.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        let terminals: HashSet<&str> = lifecycle
            .states
            .iter()
            .filter(|state| matches!(state.kind, LifecycleStateKind::Terminal))
            .map(|state| state.name.as_str())
            .collect();

        for transition in &lifecycle.transitions {
            for from in &transition.from {
                if terminals.contains(from.as_str()) {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        resource: resource.name.clone(),
                        terminal_state: from.clone(),
                        transition: transition.name.clone(),
                    });
                }
            }
        }
    }

    findings
}

// tests

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
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
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
            invariants: vec![LifecycleInvariant::TerminalImmutable],
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
    fn positive_terminal_has_outgoing_fires() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("unpublish", vec!["published"], "draft")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0],
            Finding {
                path: PathBuf::from("features/publishing/publishing.lzi"),
                resource: "Publication".into(),
                terminal_state: "published".into(),
                transition: "unpublish".into(),
            }
        );
        assert_eq!(Finding::CODE, "LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION");
        assert!(findings[0].message().contains("published"));
        assert!(findings[0].message().contains("unpublish"));
    }

    #[test]
    fn negative_terminal_with_no_outgoing_does_not_fire() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("publish", vec!["draft"], "published")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        assert!(check(&feature, Path::new("features/publishing/publishing.lzi")).is_empty());
    }

    #[test]
    fn negative_intermediate_with_outgoing_does_not_fire() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("review", LifecycleStateKind::Intermediate),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("publish", vec!["review"], "published")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        assert!(check(&feature, Path::new("features/publishing/publishing.lzi")).is_empty());
    }
}
