use serde_json::{json, Value};

use lazuli_ir::{AuditSpec, Feature, HandlerRef, LifecycleStateKind, PolicyRef};

pub fn project_lifecycle(features: &[Feature]) -> Value {
    let payload: Vec<Value> = features
        .iter()
        .flat_map(|feature| {
            feature.resources.iter().filter_map(|resource| {
                resource.lifecycle.as_ref().map(|lifecycle| {
                    json!({
                        "feature": feature.name,
                        "resource": resource.name,
                        "discriminator_field": lifecycle.discriminator_field,
                        "generated_enum": lifecycle.generated_enum,
                        "states": lifecycle.states.iter().map(|state| {
                            json!({
                                "name": state.name,
                                "kind": match state.kind {
                                    LifecycleStateKind::Initial => "initial",
                                    LifecycleStateKind::Intermediate => "intermediate",
                                    LifecycleStateKind::Terminal => "terminal",
                                },
                            })
                        }).collect::<Vec<_>>(),
                        "transitions": lifecycle.transitions.iter().map(|transition| {
                            json!({
                                "name": transition.name,
                                "from": transition.from,
                                "to": transition.to,
                                "policy": transition.policy.as_ref().map(format_policy_ref),
                                "audit": transition.audit.as_ref().map(format_audit_spec),
                                "timestamps": transition.timestamps,
                                "emits": transition.emits,
                                "requires": transition.requires.as_ref().map(format_policy_ref),
                            })
                        }).collect::<Vec<_>>(),
                        "invariants": lifecycle.invariants.iter().map(|invariant| {
                            serde_json::to_value(invariant).expect("LifecycleInvariant serializes")
                        }).collect::<Vec<_>>(),
                        "invariant_handlers": lifecycle.invariant_handlers.iter()
                            .map(format_handler_ref)
                            .collect::<Vec<_>>(),
                    })
                })
            })
        })
        .collect();

    json!({ "lifecycle": payload })
}

fn format_policy_ref(policy: &PolicyRef) -> String {
    crate::policy_ref_to_string(policy)
}

fn format_audit_spec(audit: &AuditSpec) -> String {
    match &audit.emit_to {
        Some(emit_to) if audit.subjects.is_empty() => format!("emit_to {emit_to}"),
        Some(emit_to) => format!("{} emit_to {emit_to}", audit.subjects.join(", ")),
        None => audit.subjects.join(", "),
    }
}

fn format_handler_ref(handler: &HandlerRef) -> String {
    format!("@{}.{}", handler.namespace, handler.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Lifecycle, LifecycleInvariant, LifecycleState, LifecycleTransition, Policies,
        Resource,
    };

    fn mk_feature_with_lifecycle(resource_name: &str, lifecycle: Lifecycle) -> Feature {
        Feature {
            name: "publishing".to_owned(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources: vec![Resource {
                name: resource_name.to_owned(),
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
            }],
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
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature_without_lifecycle(resource_name: &str) -> Feature {
        let mut feature = mk_feature_with_lifecycle(resource_name, mk_lifecycle());
        feature.resources[0].lifecycle = None;
        feature
    }

    fn mk_lifecycle() -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".to_owned(),
            generated_enum: "PublicationStatus".to_owned(),
            states: vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("publishing", LifecycleStateKind::Intermediate),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            transitions: vec![mk_transition(
                "mark_published",
                vec!["publishing"],
                "published",
            )],
            invariants: vec![],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_state(name: &str, kind: LifecycleStateKind) -> LifecycleState {
        LifecycleState {
            name: name.to_owned(),
            kind,
            span_ref: None,
        }
    }

    fn mk_transition(name: &str, from: Vec<&str>, to: &str) -> LifecycleTransition {
        LifecycleTransition {
            name: name.to_owned(),
            from: from.into_iter().map(str::to_owned).collect(),
            to: to.to_owned(),
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
    fn project_lifecycle_emits_resource_with_states_and_transitions() {
        let output = project_lifecycle(&[mk_feature_with_lifecycle("Publication", mk_lifecycle())]);
        let entry = &output["lifecycle"][0];

        assert_eq!(entry["feature"], "publishing");
        assert_eq!(entry["resource"], "Publication");
        assert_eq!(entry["discriminator_field"], "status");
        assert_eq!(entry["generated_enum"], "PublicationStatus");
        assert_eq!(entry["states"].as_array().unwrap().len(), 3);
        assert_eq!(
            entry["states"][0],
            json!({"name": "scheduled", "kind": "initial"})
        );
        assert_eq!(
            entry["states"][2],
            json!({"name": "published", "kind": "terminal"})
        );
        assert_eq!(entry["transitions"].as_array().unwrap().len(), 1);
        assert_eq!(entry["transitions"][0]["name"], "mark_published");
        assert_eq!(entry["transitions"][0]["from"], json!(["publishing"]));
        assert_eq!(entry["transitions"][0]["to"], "published");
    }

    #[test]
    fn project_lifecycle_skips_resources_without_lifecycle() {
        let output = project_lifecycle(&[mk_feature_without_lifecycle("Publication")]);

        assert_eq!(output, json!({ "lifecycle": [] }));
    }

    #[test]
    fn project_lifecycle_serializes_invariants_with_tag_kind() {
        let mut lifecycle = mk_lifecycle();
        lifecycle.invariants = vec![LifecycleInvariant::TerminalImmutable];

        let output = project_lifecycle(&[mk_feature_with_lifecycle("Publication", lifecycle)]);

        assert_eq!(
            output["lifecycle"][0]["invariants"][0],
            json!({ "kind": "terminal_immutable" })
        );
    }
}
