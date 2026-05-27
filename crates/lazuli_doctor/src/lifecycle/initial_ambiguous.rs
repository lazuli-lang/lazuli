//! LIFECYCLE-INITIAL-AMBIGUOUS — implicit initial state on a 3+-state
//! lifecycle.
//!
//! Fires when a lifecycle has ≥3 states and no state is marked
//! `initial`. The §3.2 grammar lets the first declared state become
//! implicit initial when omitted, but on a 3-state-or-more machine the
//! "first state wins" rule is non-obvious to cold-readers; doctor warns
//! the author to mark it explicitly. Companion to
//! `LIFECYCLE-NO-INITIAL-STATE`: that rule handles the hard-error case
//! when the implicit-initial fallback isn't safe (e.g. when the order
//! of state declarations isn't unambiguous), this rule handles the
//! warning case for cold-reader clarity even when an implicit initial
//! exists.
//!
//! Severity: warning (strict), error (production). Matches the table in
//! `docs/proposals/lifecycle-vocab.md` §5.
//!
//! Opt-out: `# doctor:allow LIFECYCLE-INITIAL-AMBIGUOUS` on the source
//! `.lzi` skips the check.
//!
//! Example trigger fixture:
//!
//! ```text
//! resource Publication
//!   lifecycle status
//!     state scheduled       # <-- no `initial`, no other state marks it
//!     state publishing
//!     state published terminal
//! ```
//!
//! Reference: docs/proposals/lifecycle-vocab.md §3.2, §5.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, LifecycleStateKind};

/// Threshold above which the implicit-initial fallback becomes
/// ambiguous to cold-readers and the warning fires.
const AMBIGUOUS_STATE_THRESHOLD: usize = 3;

/// One LIFECYCLE-INITIAL-AMBIGUOUS finding — a lifecycle with ≥3
/// states and no explicit `initial` mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the lifecycle is declared in.
    pub path: PathBuf,
    /// Resource owning the lifecycle.
    pub resource: String,
    /// Number of states declared on the lifecycle (≥3 when the rule
    /// fires).
    pub state_count: usize,
    /// Name of the state the implicit-initial fallback would pick (the
    /// first-declared state). Empty string when the lifecycle declares
    /// no states at all — guarded against but never expected.
    pub implicit_first: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "LIFECYCLE-INITIAL-AMBIGUOUS";

    /// Render the "ambiguous initial" message, naming the resource,
    /// state count, and the state the implicit-initial fallback would
    /// pick so the author can either confirm with an explicit mark or
    /// reorder.
    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}` has {} states with no explicit `initial` mark — \
             cold-readers can't see which state new rows start in (implicit fallback \
             would pick `{}`); mark one state with `initial` explicitly",
            self.resource, self.state_count, self.implicit_first
        )
    }
}

/// Walk every resource lifecycle in `feature` and flag the ones with
/// ≥3 states and no `Initial` state kind.
///
/// Honors `# doctor:allow LIFECYCLE-INITIAL-AMBIGUOUS` on the source
/// file: when present, the rule short-circuits to an empty finding list.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }

    let mut findings = Vec::new();

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        if lifecycle.states.len() < AMBIGUOUS_STATE_THRESHOLD {
            continue;
        }
        let has_initial = lifecycle
            .states
            .iter()
            .any(|s| matches!(s.kind, LifecycleStateKind::Initial));
        if has_initial {
            continue;
        }

        let implicit_first = lifecycle
            .states
            .first()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        findings.push(Finding {
            path: path.to_path_buf(),
            resource: resource.name.clone(),
            state_count: lifecycle.states.len(),
            implicit_first,
        });
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

    fn mk_lifecycle(states: Vec<LifecycleState>) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states,
            transitions: vec![mk_transition("step", vec!["a"], "b")],
            invariants: vec![],
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

    #[test]
    fn negative_two_states_no_initial_does_not_fire() {
        // Below the ambiguity threshold — the implicit "first wins"
        // rule is obvious for 2 states.
        let lifecycle = mk_lifecycle(vec![
            mk_state("draft", LifecycleStateKind::Intermediate),
            mk_state("published", LifecycleStateKind::Terminal),
        ]);
        let feature = mk_feature("Publication", lifecycle);

        assert!(check(&feature, Path::new("publishing.lzi")).is_empty());
    }

    #[test]
    fn negative_three_states_with_explicit_initial_does_not_fire() {
        let lifecycle = mk_lifecycle(vec![
            mk_state("scheduled", LifecycleStateKind::Initial),
            mk_state("publishing", LifecycleStateKind::Intermediate),
            mk_state("published", LifecycleStateKind::Terminal),
        ]);
        let feature = mk_feature("Publication", lifecycle);

        assert!(check(&feature, Path::new("publishing.lzi")).is_empty());
    }

    #[test]
    fn positive_three_states_no_initial_fires() {
        let lifecycle = mk_lifecycle(vec![
            mk_state("scheduled", LifecycleStateKind::Intermediate),
            mk_state("publishing", LifecycleStateKind::Intermediate),
            mk_state("published", LifecycleStateKind::Terminal),
        ]);
        let feature = mk_feature("Publication", lifecycle);

        let findings = check(&feature, Path::new("publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert_eq!(findings[0].state_count, 3);
        assert_eq!(findings[0].implicit_first, "scheduled");
        assert_eq!(Finding::CODE, "LIFECYCLE-INITIAL-AMBIGUOUS");
        assert!(findings[0].message().contains("Publication"));
        assert!(findings[0].message().contains("scheduled"));
        assert!(findings[0].message().contains("initial"));
    }

    #[test]
    fn positive_many_states_no_initial_fires() {
        let lifecycle = mk_lifecycle(vec![
            mk_state("a", LifecycleStateKind::Intermediate),
            mk_state("b", LifecycleStateKind::Intermediate),
            mk_state("c", LifecycleStateKind::Intermediate),
            mk_state("d", LifecycleStateKind::Terminal),
        ]);
        let feature = mk_feature("Thing", lifecycle);

        let findings = check(&feature, Path::new("f.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].state_count, 4);
        assert_eq!(findings[0].implicit_first, "a");
    }

    #[test]
    fn opt_out_doctor_allow_comment_suppresses_finding() {
        let lifecycle = mk_lifecycle(vec![
            mk_state("scheduled", LifecycleStateKind::Intermediate),
            mk_state("publishing", LifecycleStateKind::Intermediate),
            mk_state("published", LifecycleStateKind::Terminal),
        ]);
        let feature = mk_feature("Publication", lifecycle);

        let tmp_dir = std::env::temp_dir();
        let tmp_path = tmp_dir.join("lifecycle_initial_ambiguous_opt_out.lzi");
        std::fs::write(
            &tmp_path,
            "# doctor:allow LIFECYCLE-INITIAL-AMBIGUOUS\nfeature x\n",
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
