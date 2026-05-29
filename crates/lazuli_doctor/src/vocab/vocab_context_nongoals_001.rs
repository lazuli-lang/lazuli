//! VOCAB-CONTEXT-NONGOALS-001 — feature missing or empty `non_goals` block.
//!
//! Fires when a Feature's `non_goals` list is empty (zero entries),
//! regardless of which authored surface form it came from (flat list or
//! `delegated_to` / `out_of_scope` partition).  The `tdd-iron-hand`
//! coverage preset escalates this rule from warn to error, forcing
//! every feature to document at least one explicit boundary.  Other
//! presets emit it as a `warning`; `off` suppresses the rule entirely.
//!
//! Severity (per preset):
//!   off            — suppressed
//!   tdd-strict     — warning (informational)
//!   tdd-mature     — warning (informational)
//!   tdd-iron-hand  — error   (gates CI)
//!
//! Reference: docs/canonical-semantics.md#feature-context-vocabulary

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-CONTEXT-NONGOALS-001 finding: a feature with no boundary list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the offending feature.
    pub feature: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-CONTEXT-NONGOALS-001";

    /// Render the "declare at least one boundary" message, naming the
    /// feature and pointing at the canonical-semantics doc.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_context_nongoals_001::Finding;
    ///
    /// let f = Finding { path: PathBuf::from("f.lzi"), feature: "billing".into() };
    /// assert!(f.message().contains("non_goals"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "feature `{}` has no `non_goals` entries — declare at least one boundary so the \
             feature's scope is visible to cold readers, e.g. \
             `non_goals\\n  \"Real-time chat (use messaging feature)\"`. \
             The `tdd-iron-hand` preset gates CI on this; other presets surface it as a \
             warning. See docs/canonical-semantics.md#feature-context-vocabulary.",
            self.feature
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-CONTEXT-NONGOALS-001 for one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O
/// is performed here.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_context_nongoals_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower any feature");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    if !feature.non_goals.is_empty() {
        return Vec::new();
    }
    vec![Finding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
    }]
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, Feature, NonGoal, Policies};

    fn mk_feature(name: &str, non_goals: Vec<NonGoal>) -> Feature {
        Feature {
            name: name.into(),
            purpose: None,
            non_goals,
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: vec![],
            uses_versions: vec![],
            requirements: vec![],
            enums: vec![],
            resources: vec![],
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

    fn mk_ng(desc: &str) -> NonGoal {
        NonGoal {
            key: String::new(),
            description: desc.into(),
        }
    }

    #[test]
    fn empty_non_goals_fires() {
        let feature = mk_feature("catalog", vec![]);
        let findings = check(&feature, Path::new("features/catalog/catalog.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "catalog");
        assert_eq!(Finding::CODE, "VOCAB-CONTEXT-NONGOALS-001");
    }

    #[test]
    fn one_entry_does_not_fire() {
        let feature = mk_feature("catalog", vec![mk_ng("Real-time chat")]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn many_entries_does_not_fire() {
        let feature = mk_feature("catalog", vec![mk_ng("A"), mk_ng("B"), mk_ng("C")]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    /// Partitioned `delegated_to` entries lower into the same flat
    /// list, so once an entry exists the lint must not fire.
    #[test]
    fn partitioned_entries_satisfy_lint() {
        let feature = mk_feature(
            "customer",
            vec![NonGoal {
                key: "delegated_to.user".into(),
                description: "staff authentication".into(),
            }],
        );
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    /// Tabled coverage — testify-style.
    #[test]
    fn tabled_cases() {
        let cases: &[(&str, usize, usize)] = &[
            ("zero_entries", 0, 1),
            ("one_entry", 1, 0),
            ("five_entries", 5, 0),
        ];
        for (label, count, expected) in cases {
            let entries = (0..*count).map(|i| mk_ng(&format!("ng_{i}"))).collect();
            let feature = mk_feature("f", entries);
            let got = check(&feature, Path::new("f.lzi")).len();
            assert_eq!(
                got, *expected,
                "case `{label}`: expected {expected}, got {got}"
            );
        }
    }
}
