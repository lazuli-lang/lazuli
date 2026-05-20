//! LIFECYCLE-TIMESTAMP-TYPE — transition timestamp field is not DateTime.
//!
//! Fires when `transition.timestamps <field>` names an existing resource field
//! whose type is not `DateTime`. Missing fields are allowed here because
//! lowering auto-emits them on the parent resource.
//!
//! Severity: error (strict), error (production).
//! Reference: docs/proposals/lifecycle-vocab.md §3.3, §5

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, TypeRef};

// ── output ────────────────────────────────────────────────────────────────────

/// One LIFECYCLE-TIMESTAMP-TYPE finding: a timestamp field with the wrong type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource declaring the lifecycle.
    pub resource: String,
    /// Transition carrying the `timestamps` child.
    pub transition: String,
    /// Field named by `timestamps`.
    pub field: String,
    /// Debug label for the field's actual IR type.
    pub actual_type: String,
}

impl Finding {
    pub const CODE: &'static str = "LIFECYCLE-TIMESTAMP-TYPE";

    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}`: transition `{}` stamps `{}` (declared as `{}`) — \
             `timestamps` requires a `DateTime` field",
            self.resource, self.transition, self.field, self.actual_type
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run LIFECYCLE-TIMESTAMP-TYPE for all lifecycle transitions in one feature.
///
/// `path` is the source `.lzi` file used to anchor findings. No I/O is performed
/// here. If the named timestamp field is absent, this rule intentionally stays
/// silent because lifecycle lowering auto-emits that DateTime field.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        for transition in &lifecycle.transitions {
            let Some(field_name) = transition.timestamps.as_ref() else {
                continue;
            };
            let Some(field) = resource
                .fields
                .iter()
                .find(|field| &field.name == field_name)
            else {
                continue;
            };

            if !matches!(&field.type_ref, TypeRef::Builtin(BuiltinType::DateTime)) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    resource: resource.name.clone(),
                    transition: transition.name.clone(),
                    field: field_name.clone(),
                    actual_type: format!("{:?}", field.type_ref),
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
        BuiltinType, Defaults, Feature, Field, FieldConstraints, Lifecycle, LifecycleState,
        LifecycleStateKind, LifecycleTransition, Policies, Resource, TypeRef,
    };

    fn mk_feature_with_lifecycle(
        resource_name: &str,
        fields: Vec<Field>,
        lc: Lifecycle,
    ) -> Feature {
        let resource = Resource {
            name: resource_name.into(),
            public_contract: None,
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

    fn mk_lifecycle_with_timestamp(field_name: &str) -> Lifecycle {
        let mut transition = mk_transition("publish", vec!["draft"], "published");
        transition.timestamps = Some(field_name.to_owned());

        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states: vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            transitions: vec![transition],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            span_ref: None,
        }
    }

    #[test]
    fn positive_text_field_fires() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            vec![mk_field(
                "published_at",
                TypeRef::Builtin(BuiltinType::Text),
            )],
            mk_lifecycle_with_timestamp("published_at"),
        );

        let findings = check(&feature, Path::new("features/publication/publication.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].transition, "publish");
        assert_eq!(findings[0].field, "published_at");
        assert_eq!(findings[0].actual_type, "Builtin(Text)");
        assert_eq!(Finding::CODE, "LIFECYCLE-TIMESTAMP-TYPE");
        assert!(
            findings[0]
                .message()
                .contains("requires a `DateTime` field"),
            "message should explain the required timestamp type"
        );
    }

    #[test]
    fn negative_datetime_field_does_not_fire() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            vec![mk_field(
                "published_at",
                TypeRef::Builtin(BuiltinType::DateTime),
            )],
            mk_lifecycle_with_timestamp("published_at"),
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_field_missing_does_not_fire() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            vec![],
            mk_lifecycle_with_timestamp("published_at"),
        );

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
