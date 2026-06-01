//! LIFECYCLE-STATE-SET-UNDECLARED-001 — lifecycle machine with no closed state set.
//!
//! Fires when a resource declares a lifecycle/transition machine (≥1
//! `transition`) but never declares the closed, named `state` set those
//! transitions bind to — the "enum-by-command" shape, where the status
//! lattice lives only in prose comments and is *implied* by which command
//! ran rather than declared as a typed closed set. Without the declared
//! set there is nothing for `LIFECYCLE-TRANSITION-{FROM,TO}-UNDECLARED` to
//! membership-check against, so a transition's `from`/`to` can name any
//! identifier.
//!
//! Example (fires): a `lifecycle status` block carrying `transition`s but
//! whose `state` list is empty — the closed set was documented in a
//! comment ("pending | in_progress | completed") instead of declared.
//! Silent when the closed `state` set is present (every well-formed
//! `lifecycle` block declares ≥2 states, so authored lifecycles never
//! trip this; it guards the degenerate command-implied machine).
//!
//! Severity: `error` (strict), `error` (production).
//! Reference: .specs/changes/0017-state-enum-transition/techspec.md §Contracts

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

/// One LIFECYCLE-STATE-SET-UNDECLARED-001 finding — a lifecycle machine
/// that carries transitions but declares no closed `state` set for them
/// to bind to (the "enum-by-command" shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the lifecycle is declared in.
    pub path: PathBuf,
    /// Resource owning the machine.
    pub resource: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "LIFECYCLE-STATE-SET-UNDECLARED-001";

    /// Render the "no closed state set" message, steering the author to
    /// declare the lifecycle's states as a closed `state {}` set rather
    /// than leaving the lattice command-implied / comment-only.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::lifecycle::state_set_undeclared_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("attachments.lzi"),
    ///     resource: "Attachment".into(),
    /// };
    /// assert!(f.message().contains("Attachment"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}` declares transitions but no closed `state` set — declare the \
             states as a named closed set (`state <name> initial` … `state <name> terminal`) so \
             transitions bind to a typed lattice instead of an enum-by-command shape",
            self.resource
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Walk every resource lifecycle in `feature` and flag any machine that
/// carries `transitions` but declares no `states` — the closed state set
/// is undeclared, so the transitions have no typed lattice to bind to.
///
/// `path` anchors findings; no I/O is performed here.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::lifecycle::state_set_undeclared_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature whose lifecycle has transitions but no states");
/// let _ = check(&feature, Path::new("attachments.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = vec![];

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };
        // A machine with transitions but no declared closed state set is the
        // enum-by-command shape. A machine with no transitions AND no states
        // is not a lifecycle at all — leave it to the other structural rules.
        if !lifecycle.transitions.is_empty() && lifecycle.states.is_empty() {
            findings.push(Finding {
                path: path.to_path_buf(),
                resource: resource.name.clone(),
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
        Defaults, Feature, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, Resource,
    };

    fn mk_feature_with_lifecycle(resource_name: &str, lifecycle: Lifecycle) -> Feature {
        let resource = Resource {
            name: resource_name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
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
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            restrict_on_delete: Vec::new(),
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

    fn mk_lifecycle(
        states: Vec<LifecycleState>,
        transitions: Vec<LifecycleTransition>,
    ) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "AttachmentStatus".into(),
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

    /// The "enum-by-command" shape: transitions exist but no closed state
    /// set was declared for them to bind to.
    #[test]
    fn positive_transitions_without_states_fires() {
        let lifecycle = mk_lifecycle(
            vec![],
            vec![mk_transition("mark_uploaded", vec!["pending"], "uploaded")],
        );
        let feature = mk_feature_with_lifecycle("Attachment", lifecycle);

        let findings = check(&feature, Path::new("features/attachments/attachments.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Attachment");
        assert_eq!(Finding::CODE, "LIFECYCLE-STATE-SET-UNDECLARED-001");
        assert!(
            findings[0].message().contains("Attachment"),
            "message should name the resource"
        );
        assert!(
            findings[0].message().contains("closed `state` set"),
            "message should steer toward declaring the closed state set"
        );
    }

    /// A well-formed lifecycle with a declared closed state set is silent —
    /// this is the idiomatic shape every authored `lifecycle` block lands on.
    #[test]
    fn negative_declared_state_set_does_not_fire() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("pending", LifecycleStateKind::Initial),
                mk_state("uploaded", LifecycleStateKind::Terminal),
            ],
            vec![mk_transition("mark_uploaded", vec!["pending"], "uploaded")],
        );
        let feature = mk_feature_with_lifecycle("Attachment", lifecycle);

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    /// A resource with no lifecycle at all is not in scope.
    #[test]
    fn negative_no_lifecycle_does_not_fire() {
        let mut feature = mk_feature_with_lifecycle("Attachment", mk_lifecycle(vec![], vec![]));
        feature.resources[0].lifecycle = None;

        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
