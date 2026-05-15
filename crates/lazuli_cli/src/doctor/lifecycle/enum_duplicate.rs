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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub resource: String,
    pub enum_name: String,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-ENUM-DUPLICATE";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}` auto-emits enum `{}` but a sibling `enum {}` is already declared — rename your existing enum OR rename the lifecycle discriminator field",
            self.resource, self.enum_name, self.enum_name
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

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
            reports: vec![],
            channels: vec![],
            aggregates: vec![],
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
