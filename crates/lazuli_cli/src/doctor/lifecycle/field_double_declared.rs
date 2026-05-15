//! LIFECYCLE-FIELD-DOUBLE-DECLARED — lifecycle discriminator also declared as a field.
//!
//! Fires when `lifecycle <field>` names a discriminator field that also appears
//! in the resource's explicit `fields` list.
//!
//! Severity: `error` (strict), `error` (production).
//! Reference: docs/proposals/lifecycle-vocab.md §3.2, §5

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

/// One LIFECYCLE-FIELD-DOUBLE-DECLARED finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the resource that owns the lifecycle.
    pub resource: String,
    /// Lifecycle discriminator field declared twice.
    pub discriminator_field: String,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-FIELD-DOUBLE-DECLARED";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}` declares discriminator `{}` but the resource's `fields` also \
             declares it — the lifecycle owns this field, remove it from the explicit fields block",
            self.resource, self.discriminator_field
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run LIFECYCLE-FIELD-DOUBLE-DECLARED for all resources in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];

    for r in &feature.resources {
        let Some(lc) = r.lifecycle.as_ref() else {
            continue;
        };

        if r.fields.iter().any(|f| f.name == lc.discriminator_field) {
            findings.push(Finding {
                path: path.to_path_buf(),
                resource: r.name.clone(),
                discriminator_field: lc.discriminator_field.clone(),
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
        BuiltinType, Defaults, Feature, Field, FieldConstraints, Lifecycle, LifecycleState,
        LifecycleStateKind, LifecycleTransition, Policies, QualifiedName, Resource, TypeRef,
    };

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_feature_with_resource(resource: Resource) -> Feature {
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
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_resource(resource_name: &str, fields: Vec<Field>, lc: Lifecycle) -> Resource {
        Resource {
            name: resource_name.into(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
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
        }
    }

    fn mk_lifecycle(discriminator_field: &str) -> Lifecycle {
        Lifecycle {
            discriminator_field: discriminator_field.into(),
            generated_enum: "PublicationStatus".into(),
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

    fn enum_field(name: &str, type_name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::UserDefined(qn(type_name)),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn text_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn positive_field_double_declared_fires() {
        let resource = mk_resource(
            "Publication",
            vec![enum_field("status", "PublicationStatus")],
            mk_lifecycle("status"),
        );
        let feature = mk_feature_with_resource(resource);

        let findings = check(&feature, Path::new("features/publishing/publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].discriminator_field, "status");
        assert_eq!(Finding::CODE, "LIFECYCLE-FIELD-DOUBLE-DECLARED");
        assert!(
            findings[0].message().contains("Publication"),
            "message should name the resource"
        );
        assert!(
            findings[0].message().contains("status"),
            "message should name the discriminator field"
        );
    }

    #[test]
    fn negative_no_overlap_does_not_fire() {
        let resource = mk_resource(
            "Publication",
            vec![text_field("title")],
            mk_lifecycle("status"),
        );
        let feature = mk_feature_with_resource(resource);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
