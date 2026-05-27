//! LIFECYCLE-INVARIANT-CATALOG-MISMATCH — invariant catalog entry is
//! malformed at the IR level.
//!
//! The closed-catalog enforcement of `invariant <form>` happens at
//! parse time inside `lazuli_syntax::parser::lzi::helpers::parse_invariant_form`:
//! an unknown head identifier becomes a parse error before lowering.
//! This post-parse rule is the defense-in-depth check that catches the
//! residual mismatches a hand-constructed IR (or a corrupted JSON
//! snapshot) can still smuggle in:
//!
//! - `LifecycleInvariant::SingleStatePerScope` with an empty `state` or
//!   empty `scope_field` (parser rejects, but a manually-deserialized
//!   IR can carry empty strings).
//!
//! Fires when any `LifecycleInvariant` is structurally malformed in
//! one of the above ways.
//!
//! Severity: error (strict), error (production). Matches the table in
//! `docs/proposals/lifecycle-vocab.md` §5.
//!
//! Coordination with parse-time enforcement (Cell B): the parser is
//! the primary line of defense and produces actionable line-anchored
//! errors. This IR-level rule complements it for the cases the parser
//! can't see (programmatically-constructed `Feature`, deserialized
//! snapshots, IR mutations during lowering passes).
//!
//! Opt-out: `# doctor:allow LIFECYCLE-INVARIANT-CATALOG-MISMATCH` on
//! the source `.lzi` skips the check.
//!
//! Example trigger fixture (programmatically constructed IR):
//!
//! ```ignore
//! Lifecycle {
//!     invariants: vec![LifecycleInvariant::SingleStatePerScope {
//!         state: "".into(),         // <-- empty state
//!         scope_field: "org".into(),
//!     }],
//!     // ... other fields
//! }
//! ```
//!
//! Reference: docs/proposals/lifecycle-vocab.md §3.4, §5; closed-set
//! parser helper at
//! `crates/lazuli_syntax/src/parser/lzi/helpers.rs::parse_invariant_form`.

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, LifecycleInvariant};

/// One LIFECYCLE-INVARIANT-CATALOG-MISMATCH finding — an invariant
/// catalog entry is structurally malformed at the IR level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the lifecycle is declared in.
    pub path: PathBuf,
    /// Resource owning the lifecycle.
    pub resource: String,
    /// Short, structured description of why the invariant is malformed
    /// (e.g. "`single <state> per <scope_field>` has empty state name").
    pub reason: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "LIFECYCLE-INVARIANT-CATALOG-MISMATCH";

    /// Render the "catalog mismatch" message, steering the author back
    /// to the closed catalog spec.
    pub fn message(&self) -> String {
        format!(
            "lifecycle on `{}`: invariant entry is malformed — {} (closed catalog: \
             `terminal_immutable`, `single <state> per <scope_field>`, \
             `no_jump_more_than_one` — see docs/proposals/lifecycle-vocab.md §3.4)",
            self.resource, self.reason
        )
    }
}

/// Walk every resource lifecycle in `feature` and flag every invariant
/// that's structurally malformed (empty required slots).
///
/// Honors `# doctor:allow LIFECYCLE-INVARIANT-CATALOG-MISMATCH` on the
/// source file: when present, the rule short-circuits to an empty
/// finding list.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if crate::allow_comment::file_contains_doctor_allow(path, Finding::CODE) {
        return Vec::new();
    }

    let mut findings = Vec::new();

    for resource in &feature.resources {
        let Some(lifecycle) = resource.lifecycle.as_ref() else {
            continue;
        };

        for invariant in &lifecycle.invariants {
            if let Some(reason) = malformed_reason(invariant) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    resource: resource.name.clone(),
                    reason,
                });
            }
        }
    }

    findings
}

/// Returns `Some(reason)` when an invariant entry is malformed at the
/// IR level, `None` when it's well-formed.
///
/// Closed-catalog variants without payload (`TerminalImmutable`,
/// `NoJumpMoreThanOne`) are always well-formed by enum construction;
/// the only payload-bearing variant is `SingleStatePerScope`, which
/// must carry non-empty `state` and `scope_field` strings.
fn malformed_reason(invariant: &LifecycleInvariant) -> Option<String> {
    match invariant {
        LifecycleInvariant::TerminalImmutable | LifecycleInvariant::NoJumpMoreThanOne => None,
        LifecycleInvariant::SingleStatePerScope { state, scope_field } => {
            if state.trim().is_empty() {
                return Some(
                    "`single <state> per <scope_field>` has empty state name".to_owned(),
                );
            }
            if scope_field.trim().is_empty() {
                return Some(
                    "`single <state> per <scope_field>` has empty scope field name".to_owned(),
                );
            }
            None
        }
    }
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

    fn mk_lifecycle(invariants: Vec<LifecycleInvariant>) -> Lifecycle {
        Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: "PublicationStatus".into(),
            states: vec![
                mk_state("draft", LifecycleStateKind::Initial),
                mk_state("published", LifecycleStateKind::Terminal),
            ],
            transitions: vec![mk_transition("publish", vec!["draft"], "published")],
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
    fn negative_well_formed_invariants_do_not_fire() {
        let lifecycle = mk_lifecycle(vec![
            LifecycleInvariant::TerminalImmutable,
            LifecycleInvariant::NoJumpMoreThanOne,
            LifecycleInvariant::SingleStatePerScope {
                state: "gold".into(),
                scope_field: "item_id".into(),
            },
        ]);
        let feature = mk_feature("Publication", lifecycle);

        assert!(check(&feature, Path::new("publishing.lzi")).is_empty());
    }

    #[test]
    fn positive_empty_state_fires() {
        let lifecycle = mk_lifecycle(vec![LifecycleInvariant::SingleStatePerScope {
            state: "".into(),
            scope_field: "org".into(),
        }]);
        let feature = mk_feature("Publication", lifecycle);

        let findings = check(&feature, Path::new("publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Publication");
        assert!(findings[0].reason.contains("empty state name"));
        assert_eq!(Finding::CODE, "LIFECYCLE-INVARIANT-CATALOG-MISMATCH");
        assert!(findings[0].message().contains("Publication"));
        assert!(findings[0].message().contains("closed catalog"));
    }

    #[test]
    fn positive_empty_scope_field_fires() {
        let lifecycle = mk_lifecycle(vec![LifecycleInvariant::SingleStatePerScope {
            state: "gold".into(),
            scope_field: "  ".into(),
        }]);
        let feature = mk_feature("Publication", lifecycle);

        let findings = check(&feature, Path::new("publishing.lzi"));

        assert_eq!(findings.len(), 1);
        assert!(findings[0].reason.contains("empty scope field"));
    }

    #[test]
    fn opt_out_doctor_allow_comment_suppresses_finding() {
        let lifecycle = mk_lifecycle(vec![LifecycleInvariant::SingleStatePerScope {
            state: "".into(),
            scope_field: "org".into(),
        }]);
        let feature = mk_feature("Publication", lifecycle);

        let tmp_dir = std::env::temp_dir();
        let tmp_path = tmp_dir.join("lifecycle_invariant_catalog_mismatch_opt_out.lzi");
        std::fs::write(
            &tmp_path,
            "# doctor:allow LIFECYCLE-INVARIANT-CATALOG-MISMATCH\nfeature x\n",
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
