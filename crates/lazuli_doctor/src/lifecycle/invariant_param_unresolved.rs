//! LIFECYCLE-INVARIANT-PARAM-UNRESOLVED - unresolved lifecycle invariant names.
//!
//! Fires when `invariant single <state> per <scope_field>` references a state
//! that is not declared in the lifecycle, or a scope field that is not declared
//! on the parent resource.
//!
//! Severity: `error` (strict), `error` (production).
//! Reference: docs/proposals/lifecycle-vocab.md §3.4, §5

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, LifecycleInvariant};

/// One LIFECYCLE-INVARIANT-PARAM-UNRESOLVED finding — either the
/// state or the scope-field an invariant references is not declared on
/// the parent lifecycle / resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the lifecycle is declared in.
    pub path: PathBuf,
    /// Resource owning the lifecycle.
    pub resource: String,
    /// What's unresolved — state name vs scope-field name.
    pub kind: UnresolvedKind,
    /// The dangling identifier.
    pub name: String,
}

/// Whether the unresolved reference is a lifecycle state name or a
/// resource scope-field name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedKind {
    /// `invariant single <state>` referenced a state that isn't in
    /// the lifecycle's `states` block.
    State,
    /// `invariant single <state> per <scope_field>` referenced a
    /// field that isn't declared on the parent resource.
    ScopeField,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "LIFECYCLE-INVARIANT-PARAM-UNRESOLVED";

    /// Render the "unresolved invariant parameter" message. Branches
    /// on [`UnresolvedKind`] to differentiate state vs scope-field
    /// references so authors immediately know which catalog is
    /// missing the name.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::lifecycle::invariant_param_unresolved::{
    ///     Finding, UnresolvedKind,
    /// };
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("publishing.lzi"),
    ///     resource: "Publication".into(),
    ///     kind: UnresolvedKind::State,
    ///     name: "ghost".into(),
    /// };
    /// assert!(f.message().contains("unknown state"));
    /// ```
    pub fn message(&self) -> String {
        match self.kind {
            UnresolvedKind::State => format!(
                "lifecycle on `{}`: invariant references unknown state `{}` \
                 (not declared in this lifecycle)",
                self.resource, self.name
            ),
            UnresolvedKind::ScopeField => format!(
                "lifecycle on `{}`: invariant references unknown scope field `{}` \
                 (not declared on this resource)",
                self.resource, self.name
            ),
        }
    }
}

/// Walk every resource lifecycle in `feature` and emit a finding for
/// every `SingleStatePerScope` invariant whose `state` or `scope_field`
/// is dangling.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::lifecycle::invariant_param_unresolved::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with a dangling invariant ref");
/// let _ = check(&feature, Path::new("publishing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        let states: HashSet<&str> = lifecycle
            .states
            .iter()
            .map(|state| state.name.as_str())
            .collect();
        let fields: HashSet<&str> = resource
            .fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();

        for invariant in &lifecycle.invariants {
            let LifecycleInvariant::SingleStatePerScope { state, scope_field } = invariant else {
                continue;
            };

            if !states.contains(state.as_str()) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    resource: resource.name.clone(),
                    kind: UnresolvedKind::State,
                    name: state.clone(),
                });
            }

            if !fields.contains(scope_field.as_str()) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    resource: resource.name.clone(),
                    kind: UnresolvedKind::ScopeField,
                    name: scope_field.clone(),
                });
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Field, FieldConstraints, Lifecycle, LifecycleState,
        LifecycleStateKind, Policies, Resource, TypeRef,
    };

    fn mk_feature_with_lifecycle(
        resource_name: &str,
        fields: Vec<Field>,
        lifecycle: Lifecycle,
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
            lifecycle: Some(lifecycle),
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        };

        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
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

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
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

    fn mk_lifecycle(state_names: &[&str], state: &str, scope_field: &str) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states: state_names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let kind = if index == 0 {
                        LifecycleStateKind::Initial
                    } else {
                        LifecycleStateKind::Intermediate
                    };
                    mk_state(name, kind)
                })
                .collect(),
            transitions: vec![],
            invariants: vec![LifecycleInvariant::SingleStatePerScope {
                state: state.into(),
                scope_field: scope_field.into(),
            }],
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn positive_unknown_state_fires() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            vec![mk_field("item_id")],
            mk_lifecycle(&["gold"], "ghost", "item_id"),
        );

        let findings = check(&feature, Path::new("features/pub/pub.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].kind, UnresolvedKind::State);
        assert_eq!(findings[0].name, "ghost");
        assert_eq!(Finding::CODE, "LIFECYCLE-INVARIANT-PARAM-UNRESOLVED");
        assert!(findings[0].message().contains("unknown state `ghost`"));
    }

    #[test]
    fn positive_unknown_scope_field_fires() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            vec![mk_field("item_id")],
            mk_lifecycle(&["gold"], "gold", "nonexistent_fk"),
        );

        let findings = check(&feature, Path::new("features/pub/pub.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].kind, UnresolvedKind::ScopeField);
        assert_eq!(findings[0].name, "nonexistent_fk");
        assert!(
            findings[0]
                .message()
                .contains("unknown scope field `nonexistent_fk`")
        );
    }

    #[test]
    fn positive_both_unknown_fires_twice() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            vec![mk_field("item_id")],
            mk_lifecycle(&["gold"], "ghost", "nonexistent_fk"),
        );

        let findings = check(&feature, Path::new("features/pub/pub.lzi"));

        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].kind, UnresolvedKind::State);
        assert_eq!(findings[0].name, "ghost");
        assert_eq!(findings[1].kind, UnresolvedKind::ScopeField);
        assert_eq!(findings[1].name, "nonexistent_fk");
    }

    #[test]
    fn negative_both_resolved_does_not_fire() {
        let feature = mk_feature_with_lifecycle(
            "Publication",
            vec![mk_field("item_id")],
            mk_lifecycle(&["gold"], "gold", "item_id"),
        );

        assert!(check(&feature, Path::new("features/pub/pub.lzi")).is_empty());
    }
}
