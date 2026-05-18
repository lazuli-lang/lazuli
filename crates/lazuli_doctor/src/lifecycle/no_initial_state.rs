//! LIFECYCLE-NO-INITIAL-STATE — lifecycle without an `initial` state.
//!
//! Fires when a resource lifecycle declares states but none are marked
//! `initial`.
//!
//! Severity: error (strict), error (production).
//! Reference: docs/proposals/lifecycle-vocab.md §5

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, LifecycleStateKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub resource: String,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-NO-INITIAL-STATE";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}` declares no `initial` state — exactly one state must be marked `initial` (e.g. `state draft initial`)",
            self.resource
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];
    for r in &feature.resources {
        let Some(lc) = r.lifecycle.as_ref() else {
            continue;
        };
        let has_initial = lc
            .states
            .iter()
            .any(|s| matches!(s.kind, LifecycleStateKind::Initial));
        if !has_initial {
            findings.push(Finding {
                path: path.to_path_buf(),
                resource: r.name.clone(),
            });
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

    fn mk_lifecycle(states: Vec<LifecycleState>) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states,
            transitions: vec![mk_transition("publish", vec!["draft"], "published")],
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
    fn positive_no_initial_fires() {
        let lifecycle = mk_lifecycle(vec![
            mk_state("draft", LifecycleStateKind::Intermediate),
            mk_state("published", LifecycleStateKind::Terminal),
        ]);
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(Finding::CODE, "LIFECYCLE-NO-INITIAL-STATE");
        assert!(
            findings[0].message().contains("Publication"),
            "message should name the resource"
        );
    }

    #[test]
    fn negative_with_initial_does_not_fire() {
        let lifecycle = mk_lifecycle(vec![
            mk_state("draft", LifecycleStateKind::Initial),
            mk_state("published", LifecycleStateKind::Terminal),
        ]);
        let feature = mk_feature_with_lifecycle("Publication", lifecycle);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
