//! DOCTOR-OVERRIDE-NEEDS-REASON-001 — `[doctor.*]` severity overrides
//! must carry a non-empty `reason` field.
//!
//! Fires when a project's `Lazurite.toml` declares any
//! `[doctor.<category>].severity_override.<RULE-CODE>` entry without an
//! adjacent `reason = "..."` justification.
//!
//! Rationale: profile-level overrides are escape hatches. They are
//! sometimes the right call (legacy code, multi-step migration) but they
//! suppress framework signal. Requiring a written reason keeps the
//! escape hatch auditable; without it, overrides accumulate as
//! tribal-knowledge dark matter and the framework loses the loop that
//! Rails-style enforcement depends on.
//!
//! Severity: `error` at production, `warning` at strict, `info` at
//! prototype — handled in the caller via
//! `RuleCategory::TestDiscipline` mapping (see
//! `doctor_severity_for`). The rule itself emits a category-agnostic
//! `Finding`; the dispatcher applies severity.
//!
//! Reference: docs/proposals/tdd-bdd-first-2026-05-23.md §Wave 0.5.

use std::path::{Path, PathBuf};

/// One DOCTOR-OVERRIDE-NEEDS-REASON-001 finding: a rule override missing
/// its `reason` justification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `Lazurite.toml` file the override was authored in.
    pub path: PathBuf,
    /// Doctor category whose `[doctor.<category>]` section holds the
    /// offending override (e.g. `"test_discipline"`).
    pub category: String,
    /// The rule code missing a justification (e.g. `TEST-MISSING-AUTHORED-001`).
    pub rule_code: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "DOCTOR-OVERRIDE-NEEDS-REASON-001";

    /// Render the user-facing diagnostic body — names the override
    /// site and suggests a `reason = "…"` line to add.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::override_needs_reason_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("Lazurite.toml"),
    ///     category: "test_discipline".into(),
    ///     rule_code: "TEST-MISSING-AUTHORED-001".into(),
    /// };
    /// assert!(f.message().contains("TEST-MISSING-AUTHORED-001"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "[doctor.{}].severity_override for `{}` requires a non-empty `reason` field — \
             explain why the framework default is being suppressed so the override stays \
             auditable. Add e.g. `reason = \"legacy feature scheduled for refactor in Q3\"`.",
            self.category, self.rule_code
        )
    }
}

/// View of an authored `[doctor.<category>]` override block. The
/// `lazuli_doctor` crate intentionally avoids depending on
/// `lazuli_cli::lazurite_manifest` to keep the dependency direction
/// pointing one way (cli → doctor). The CLI lifts its manifest into this
/// portable shape before calling `check`.
#[derive(Debug, Clone)]
pub struct OverrideEntry {
    /// Doctor category name as authored in TOML (`test_discipline`,
    /// `vocabulary`, …).
    pub category: String,
    /// The rule code being overridden.
    pub rule_code: String,
    /// The author-supplied severity (`warning`, `error`, `info`, `hint`).
    pub severity: String,
    /// Optional human justification.
    pub reason: Option<String>,
}

/// Run DOCTOR-OVERRIDE-NEEDS-REASON-001 over a flat list of authored
/// override entries.
///
/// `path` is the source `Lazurite.toml` file used to anchor findings.
/// No I/O is performed here. A finding is emitted for every override
/// whose `reason` is missing OR whose `reason` is present but blank.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::override_needs_reason_001::{check, OverrideEntry};
///
/// let entries = vec![OverrideEntry {
///     category: "test_discipline".into(),
///     rule_code: "TEST-MISSING-AUTHORED-001".into(),
///     severity: "warning".into(),
///     reason: None,
/// }];
/// let findings = check(&entries, Path::new("Lazurite.toml"));
/// assert_eq!(findings.len(), 1);
/// ```
pub fn check(overrides: &[OverrideEntry], path: &Path) -> Vec<Finding> {
    overrides
        .iter()
        .filter(|entry| match entry.reason.as_deref() {
            None => true,
            Some(reason) => reason.trim().is_empty(),
        })
        .map(|entry| Finding {
            path: path.to_path_buf(),
            category: entry.category.clone(),
            rule_code: entry.rule_code.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(code: &str, reason: Option<&str>) -> OverrideEntry {
        OverrideEntry {
            category: "test_discipline".to_owned(),
            rule_code: code.to_owned(),
            severity: "warning".to_owned(),
            reason: reason.map(|r| r.to_owned()),
        }
    }

    #[test]
    fn override_without_reason_fires() {
        let findings = check(
            &[entry("TEST-MISSING-AUTHORED-001", None)],
            Path::new("Lazurite.toml"),
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_code, "TEST-MISSING-AUTHORED-001");
        assert_eq!(Finding::CODE, "DOCTOR-OVERRIDE-NEEDS-REASON-001");
        assert!(
            findings[0].message().contains("reason"),
            "message should mention the missing reason"
        );
    }

    #[test]
    fn override_with_blank_reason_fires() {
        let findings = check(
            &[entry("TEST-MISSING-AUTHORED-001", Some("   "))],
            Path::new("Lazurite.toml"),
        );
        assert_eq!(findings.len(), 1, "blank/whitespace reason is not a reason");
    }

    #[test]
    fn override_with_real_reason_does_not_fire() {
        let findings = check(
            &[entry(
                "TEST-MISSING-AUTHORED-001",
                Some("legacy feature scheduled for refactor in Q3"),
            )],
            Path::new("Lazurite.toml"),
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn multiple_overrides_emit_per_rule_findings() {
        let findings = check(
            &[
                entry("TEST-MISSING-AUTHORED-001", None),
                entry("TEST-PREDICATE-UNCOVERED-001", Some("")),
                entry("TEST-RESTATES-EFFECT-001", Some("documented rationale")),
            ],
            Path::new("Lazurite.toml"),
        );
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn empty_overrides_emit_nothing() {
        let findings = check(&[], Path::new("Lazurite.toml"));
        assert!(findings.is_empty());
    }
}
