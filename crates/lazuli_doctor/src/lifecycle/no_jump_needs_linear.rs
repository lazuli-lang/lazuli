//! LIFECYCLE-NO-JUMP-NEEDS-LINEAR — `no_jump_more_than_one` on a
//! non-linear lifecycle.
//!
//! Fires when a lifecycle declares `invariant no_jump_more_than_one`
//! BUT the transition graph is not a single linear chain — any state
//! has fan-in (≥2 incoming transitions) OR fan-out (≥2 outgoing
//! transitions) OR a transition has multiple sources (multi-`from`
//! fan-in). `no_jump_more_than_one` is only meaningful when the states
//! sit in a totally-ordered sequence; on a branching graph the
//! invariant has no operational meaning.
//!
//! Severity: error (strict), error (production). Matches the table in
//! `docs/proposals/lifecycle-vocab.md` §5.
//!
//! Opt-out: `# doctor:allow LIFECYCLE-NO-JUMP-NEEDS-LINEAR` on the
//! source `.lzi` skips the check.
//!
//! Example trigger fixture:
//!
//! ```text
//! lifecycle status
//!   state scheduled initial
//!   state publishing
//!   state published terminal
//!   state failed terminal      # <-- fan-out from `publishing`
//!   transition mark_published
//!     from publishing
//!     to published
//!   transition mark_failed
//!     from publishing
//!     to failed
//!   invariant no_jump_more_than_one   # <-- rule fires here
//! ```
//!
//! Reference: docs/proposals/lifecycle-vocab.md §3.4, §5.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Lifecycle, LifecycleInvariant};

/// One LIFECYCLE-NO-JUMP-NEEDS-LINEAR finding — `no_jump_more_than_one`
/// declared on a lifecycle whose transition graph branches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the lifecycle is declared in.
    pub path: PathBuf,
    /// Resource owning the lifecycle.
    pub resource: String,
    /// Short reason text describing the non-linearity (e.g. "state
    /// `publishing` has fan-out 2"). Surfaced so authors immediately
    /// see which edge to flatten.
    pub reason: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "LIFECYCLE-NO-JUMP-NEEDS-LINEAR";

    /// Render the "non-linear machine" message, naming the resource and
    /// the specific branching reason so authors know which edge to
    /// flatten (or drop the invariant for).
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::lifecycle::no_jump_needs_linear::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("publishing.lzi"),
    ///     resource: "Publication".into(),
    ///     reason: "state `publishing` has fan-out 2 (more than one outgoing transition)".into(),
    /// };
    /// assert!(f.message().contains("Publication"));
    /// assert!(f.message().contains("linear chain"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}`: `invariant no_jump_more_than_one` requires a single linear \
             chain of states, but {} — drop the invariant or restructure the transitions into \
             one linear chain (see docs/proposals/lifecycle-vocab.md §3.4)",
            self.resource, self.reason
        )
    }
}

/// Walk every resource lifecycle in `feature` and flag the ones whose
/// `LifecycleInvariant::NoJumpMoreThanOne` is declared on a non-linear
/// transition graph.
///
/// Honors `# doctor:allow LIFECYCLE-NO-JUMP-NEEDS-LINEAR` on the source
/// file: when present, the rule short-circuits to an empty finding list.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::lifecycle::no_jump_needs_linear::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with NoJumpMoreThanOne invariant");
/// let _ = check(&feature, Path::new("publishing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }

    let mut findings = Vec::new();

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        let has_no_jump_invariant = lifecycle
            .invariants
            .iter()
            .any(|inv| matches!(inv, LifecycleInvariant::NoJumpMoreThanOne));
        if !has_no_jump_invariant {
            continue;
        }

        if let Some(reason) = non_linear_reason(lifecycle) {
            findings.push(Finding {
                path: path.to_path_buf(),
                resource: resource.name.clone(),
                reason,
            });
        }
    }

    findings
}

/// Returns `Some(reason)` when the lifecycle's transition graph is NOT
/// a single linear chain, and `None` when it is linear.
///
/// Linearity rules:
/// - every transition has exactly one `from` (no multi-source fan-in);
/// - every state has at most one outgoing transition (no fan-out);
/// - every state has at most one incoming transition (no fan-in).
fn non_linear_reason(lifecycle: &Lifecycle) -> Option<String> {
    // Multi-source `from` clause = fan-in, fail fast with a precise
    // pointer.
    if let Some(transition) = lifecycle.transitions.iter().find(|t| t.from.len() > 1) {
        return Some(format!(
            "transition `{}` has {} source states (multi-`from` fan-in)",
            transition.name,
            transition.from.len()
        ));
    }

    let mut outgoing: HashMap<&str, usize> = HashMap::new();
    let mut incoming: HashMap<&str, usize> = HashMap::new();
    for transition in &lifecycle.transitions {
        for from_state in &transition.from {
            *outgoing.entry(from_state.as_str()).or_default() += 1;
        }
        *incoming.entry(transition.to.as_str()).or_default() += 1;
    }

    for state in &lifecycle.states {
        let out = outgoing.get(state.name.as_str()).copied().unwrap_or(0);
        if out > 1 {
            return Some(format!(
                "state `{}` has fan-out {} (more than one outgoing transition)",
                state.name, out
            ));
        }
        let inn = incoming.get(state.name.as_str()).copied().unwrap_or(0);
        if inn > 1 {
            return Some(format!(
                "state `{}` has fan-in {} (more than one incoming transition)",
                state.name, inn
            ));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Defaults, Feature, Lifecycle, LifecycleState, LifecycleStateKind, LifecycleTransition,
        Policies, Resource,
    };

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

    fn mk_lifecycle(
        states: Vec<LifecycleState>,
        transitions: Vec<LifecycleTransition>,
        invariants: Vec<LifecycleInvariant>,
    ) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states,
            transitions,
            invariants,
            invariant_handlers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(resource_name: &str, lifecycle: Lifecycle) -> Feature {
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

    #[test]
    fn negative_linear_chain_with_invariant_does_not_fire() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("locked", LifecycleStateKind::Initial),
                mk_state("available", LifecycleStateKind::Intermediate),
                mk_state("studying", LifecycleStateKind::Intermediate),
                mk_state("completed", LifecycleStateKind::Terminal),
            ],
            vec![
                mk_transition("unlock", vec!["locked"], "available"),
                mk_transition("start", vec!["available"], "studying"),
                mk_transition("complete", vec!["studying"], "completed"),
            ],
            vec![LifecycleInvariant::NoJumpMoreThanOne],
        );
        let feature = mk_feature("LearningNode", lifecycle);

        assert!(check(&feature, Path::new("erudito.lzi")).is_empty());
    }

    #[test]
    fn negative_no_invariant_skips_rule_even_when_non_linear() {
        // No `NoJumpMoreThanOne` invariant declared — rule must not fire
        // regardless of branching.
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("publishing", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
                mk_state("failed", LifecycleStateKind::Terminal),
            ],
            vec![
                mk_transition("ok", vec!["publishing"], "published"),
                mk_transition("err", vec!["publishing"], "failed"),
            ],
            vec![],
        );
        let feature = mk_feature("Publication", lifecycle);

        assert!(check(&feature, Path::new("publishing.lzi")).is_empty());
    }

    #[test]
    fn positive_fan_out_fires() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("publishing", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
                mk_state("failed", LifecycleStateKind::Terminal),
            ],
            vec![
                mk_transition("ok", vec!["publishing"], "published"),
                mk_transition("err", vec!["publishing"], "failed"),
            ],
            vec![LifecycleInvariant::NoJumpMoreThanOne],
        );
        let feature = mk_feature("Publication", lifecycle);

        let findings = check(&feature, Path::new("publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert!(findings[0].reason.contains("publishing"));
        assert!(findings[0].reason.contains("fan-out"));
        assert_eq!(Finding::CODE, "LIFECYCLE-NO-JUMP-NEEDS-LINEAR");
        assert!(findings[0].message().contains("Publication"));
        assert!(findings[0].message().contains("linear chain"));
    }

    #[test]
    fn positive_multi_from_fan_in_fires() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("scheduled", LifecycleStateKind::Initial),
                mk_state("publishing", LifecycleStateKind::Intermediate),
                mk_state("cancelled", LifecycleStateKind::Terminal),
            ],
            vec![
                mk_transition("begin", vec!["scheduled"], "publishing"),
                // multi-`from` cancel = fan-in
                mk_transition("cancel", vec!["scheduled", "publishing"], "cancelled"),
            ],
            vec![LifecycleInvariant::NoJumpMoreThanOne],
        );
        let feature = mk_feature("Publication", lifecycle);

        let findings = check(&feature, Path::new("publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert!(findings[0].reason.contains("cancel"));
        assert!(findings[0].reason.contains("fan-in"));
    }

    #[test]
    fn opt_out_doctor_allow_comment_suppresses_finding() {
        let lifecycle = mk_lifecycle(
            vec![
                mk_state("publishing", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
                mk_state("failed", LifecycleStateKind::Terminal),
            ],
            vec![
                mk_transition("ok", vec!["publishing"], "published"),
                mk_transition("err", vec!["publishing"], "failed"),
            ],
            vec![LifecycleInvariant::NoJumpMoreThanOne],
        );
        let feature = mk_feature("Publication", lifecycle);

        let tmp_dir = std::env::temp_dir();
        let tmp_path = tmp_dir.join("lifecycle_no_jump_needs_linear_opt_out.lzi");
        std::fs::write(
            &tmp_path,
            "# doctor:allow LIFECYCLE-NO-JUMP-NEEDS-LINEAR\nfeature x\n",
        )
        .expect("write temp opt-out file");

        let findings = check(&feature, &tmp_path);

        let _ = std::fs::remove_file(&tmp_path);
        assert!(
            findings.is_empty(),
            "doctor:allow comment must suppress the finding"
        );
    }
}
