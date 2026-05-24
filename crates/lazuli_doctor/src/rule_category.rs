//! Doctor rule categories — taxonomy used by severity overrides, JSON
//! grouping (`summary.by_category`), and per-profile escalation.
//!
//! Wave 0.5 introduces this enum so the framework can express
//! "TestDiscipline rules behave differently at `strict` than other
//! categories". Before this enum, `doctor_rule_severity()` was a single
//! global function that applied one mapping to every rule. See
//! `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5.
//!
//! Variants mirror the existing module layout under `lazuli_doctor/src/`
//! (`vocab/`, `correctness/`, `domain/`, etc.) plus the new
//! `test_discipline/` module introduced in this wave.

use serde::{Deserialize, Serialize};

/// Coarse taxonomy of doctor rules. Used for severity overrides and JSON
/// rollups; every diagnostic carries one (possibly defaulted via
/// `from_code_prefix`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleCategory {
    Vocabulary,
    Correctness,
    Security,
    TestDiscipline,
    Design,
    Encryption,
    Lifecycle,
    Domain,
    CrossFeature,
    ErrorVocab,
    Poller,
    Report,
}

impl RuleCategory {
    /// Map a rule code prefix to its canonical category.
    ///
    /// Used by diagnostic construction sites that have not yet been
    /// migrated to set `category` explicitly. The Wave 0.5 risk note
    /// (see proposal) calls out that the rewrite is mechanical but
    /// error-prone across ~300 sites; this helper lets the migration
    /// proceed site-by-site over follow-up cells without blocking
    /// Wave 1.
    ///
    /// The fallback is `Vocabulary` — the largest existing module — so
    /// unmigrated codes land in the broadest bucket rather than
    /// accidentally claiming a narrower category like `Security`.
    pub fn from_code_prefix(code: &str) -> Self {
        match code.split('-').next() {
            Some("TEST") | Some("DOCTOR") => Self::TestDiscipline,
            Some("VOCAB") | Some("MONEY") => Self::Vocabulary,
            Some("SECURITY") | Some("FIELD") | Some("WEBHOOK") | Some("AUTH") => Self::Security,
            Some("HOOK") | Some("DUPLICATE") | Some("ROUTE") | Some("UPDATES")
            | Some("MUTATION") | Some("MISSING") | Some("MANUAL") | Some("IMPORT")
            | Some("CAP") | Some("SCHEMA") => Self::Correctness,
            Some("LIFECYCLE") => Self::Lifecycle,
            Some("DOMAIN") => Self::Domain,
            Some("CROSS") => Self::CrossFeature,
            Some("ERROR") => Self::ErrorVocab,
            Some("POLLER") => Self::Poller,
            Some("REPORT") => Self::Report,
            Some("DESIGN") => Self::Design,
            Some("ENCRYPTION") | Some("ENCRYPT") => Self::Encryption,
            _ => Self::Vocabulary, // safe fallback; auditor flags
        }
    }

    /// Stable snake_case identifier for JSON serialization and TOML
    /// override keys.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vocabulary => "vocabulary",
            Self::Correctness => "correctness",
            Self::Security => "security",
            Self::TestDiscipline => "test_discipline",
            Self::Design => "design",
            Self::Encryption => "encryption",
            Self::Lifecycle => "lifecycle",
            Self::Domain => "domain",
            Self::CrossFeature => "cross_feature",
            Self::ErrorVocab => "error_vocab",
            Self::Poller => "poller",
            Self::Report => "report",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_routes_to_test_discipline() {
        assert_eq!(
            RuleCategory::from_code_prefix("TEST-MISSING-AUTHORED-001"),
            RuleCategory::TestDiscipline
        );
        assert_eq!(
            RuleCategory::from_code_prefix("DOCTOR-OVERRIDE-NEEDS-REASON-001"),
            RuleCategory::TestDiscipline
        );
    }

    #[test]
    fn vocab_prefix_routes_to_vocabulary() {
        assert_eq!(
            RuleCategory::from_code_prefix("VOCAB-TESTS-MISSING-001"),
            RuleCategory::Vocabulary
        );
        assert_eq!(
            RuleCategory::from_code_prefix("MONEY-COMPARE-001"),
            RuleCategory::Vocabulary
        );
    }

    #[test]
    fn unknown_prefix_falls_back_to_vocabulary() {
        assert_eq!(
            RuleCategory::from_code_prefix("unknown-code"),
            RuleCategory::Vocabulary
        );
        assert_eq!(RuleCategory::from_code_prefix(""), RuleCategory::Vocabulary);
    }

    #[test]
    fn as_str_round_trips_via_serde() {
        let cats = [
            RuleCategory::Vocabulary,
            RuleCategory::TestDiscipline,
            RuleCategory::Security,
        ];
        for cat in cats {
            let json = serde_json::to_string(&cat).unwrap();
            assert!(
                json.contains(cat.as_str()),
                "{json} should contain {}",
                cat.as_str()
            );
        }
    }
}
