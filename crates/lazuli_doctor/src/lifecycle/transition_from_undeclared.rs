//! LIFECYCLE-TRANSITION-FROM-UNDECLARED — transition source state not declared.
//!
//! Fires when a lifecycle transition's `from` references a state name that is
//! not declared in the same lifecycle's `states`.
//!
//! Severity: error (strict), error (production).
//! Reference: docs/proposals/lifecycle-vocab.md §5

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub resource: String,
    pub transition: String,
    pub unresolved_state: String,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-TRANSITION-FROM-UNDECLARED";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}`: transition `{}` references undeclared `from` state `{}`",
            self.resource, self.transition, self.unresolved_state
        )
    }
}

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
            for from in &transition.from {
                if !declared.contains(from.as_str()) {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        resource: resource.name.clone(),
                        transition: transition.name.clone(),
                        unresolved_state: from.clone(),
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
        Defaults, Feature, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, Resource,
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
            from: from.into_iter().map(str::to_owned).collect(),
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
    fn positive_unknown_from_state_fires() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("publish", vec!["scheduledd"], "published")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);
        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0],
            Finding {
                path: PathBuf::from("features/publishing/publishing.lzi"),
                resource: "Publication".into(),
                transition: "publish".into(),
                unresolved_state: "scheduledd".into(),
            }
        );
        assert_eq!(Finding::CODE, "LIFECYCLE-TRANSITION-FROM-UNDECLARED");
        assert!(findings[0].message().contains("scheduledd"));
    }

    #[test]
    fn negative_resolved_from_does_not_fire() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("publish", vec!["scheduled"], "published")],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn positive_multi_from_one_unresolved_fires_once() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition(
                "publish",
                vec!["scheduled", "draftt"],
                "published",
            )],
        );
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);
        let findings = check(&feature, Path::new("f.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].transition, "publish");
        assert_eq!(findings[0].unresolved_state, "draftt");
    }
}
