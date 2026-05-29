//! VOCAB-CONTEXT-PURPOSE-001 — feature missing a `purpose` line.
//!
//! Fires when a Feature's `purpose` is absent OR the quoted string is
//! empty / whitespace-only.  The `tdd-iron-hand` coverage preset
//! escalates this rule from warn to error (see
//! `crates/lazuli_doctor/src/coverage/mod.rs` —
//! `preset_severity_overrides`), gating CI on every feature having a
//! one-sentence statement of intent.  Other presets emit it as a
//! `warning`; `off` suppresses the rule entirely.
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

/// One VOCAB-CONTEXT-PURPOSE-001 finding: a feature missing intent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Name of the offending feature.
    pub feature: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-CONTEXT-PURPOSE-001";

    /// Render the "no purpose line" message asking the author to add a
    /// one-sentence intent statement.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_context_purpose_001::Finding;
    ///
    /// let f = Finding { path: PathBuf::from("f.lzi"), feature: "billing".into() };
    /// assert!(f.message().contains("purpose"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "feature `{}` has no `purpose` line (or the line is empty) — add a one-sentence \
             quoted statement of intent at indent 2, e.g. \
             `purpose \"Discover and book lodging via host properties + services\"`. \
             The `tdd-iron-hand` preset gates CI on this; other presets surface it as a \
             warning. See docs/canonical-semantics.md#feature-context-vocabulary.",
            self.feature
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-CONTEXT-PURPOSE-001 for one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O
/// is performed here.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_context_purpose_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower any feature");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let missing = match feature.purpose.as_deref() {
        None => true,
        Some(text) => text.trim().is_empty(),
    };
    if !missing {
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
    use lazuli_ir::{Defaults, Feature, Policies};

    fn mk_feature(name: &str, purpose: Option<&str>) -> Feature {
        Feature {
            name: name.into(),
            purpose: purpose.map(|s| s.to_owned()),
            non_goals: vec![],
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

    #[test]
    fn missing_purpose_fires() {
        let feature = mk_feature("catalog", None);
        let findings = check(&feature, Path::new("features/catalog/catalog.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].feature, "catalog");
        assert_eq!(Finding::CODE, "VOCAB-CONTEXT-PURPOSE-001");
        assert!(findings[0].message().contains("catalog"));
        assert!(findings[0].message().contains("iron-hand"));
    }

    #[test]
    fn empty_purpose_fires() {
        let feature = mk_feature("catalog", Some(""));
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1, "empty quoted string still fires");
    }

    #[test]
    fn whitespace_only_purpose_fires() {
        let feature = mk_feature("catalog", Some("   \t  "));
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1, "whitespace-only string still fires");
    }

    #[test]
    fn nonempty_purpose_does_not_fire() {
        let feature = mk_feature("catalog", Some("Discover and book lodging"));
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    /// Tabled coverage — testify-style: one input per row, one expectation per row.
    #[test]
    fn tabled_cases() {
        let cases: &[(&str, Option<&str>, usize)] = &[
            ("missing", None, 1),
            ("empty", Some(""), 1),
            ("whitespace", Some("  "), 1),
            ("one_char", Some("x"), 0),
            ("sentence", Some("Discover and book lodging."), 0),
        ];
        for (label, purpose, expected) in cases {
            let feature = mk_feature("f", *purpose);
            let got = check(&feature, Path::new("f.lzi")).len();
            assert_eq!(
                got, *expected,
                "case `{label}`: expected {expected}, got {got}"
            );
        }
    }
}
