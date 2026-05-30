//! Wave 1.5 — `[doctor.test_discipline]` preset extension.
//!
//! Mirrors the [`crate::coverage::CoveragePreset`] mechanism for the
//! test_discipline category. A single workspace-root config line —
//! `[doctor.test_discipline] preset = "tdd-iron-hand"` — escalates every
//! `TEST-*` and `DOCTOR-*` rule to `error` severity regardless of the
//! active `SecurityProfile`. No-escape gate; the editorial veto
//! mechanism for Lazuli framework's own CI (analogous to Rails'
//! `.rubocop.yml` + `.rspec` consolidated config).
//!
//! ## Preset shapes
//!
//! - [`TestDisciplinePreset::TddIronHand`] — every rule fires `error`.
//!   Workspace contract: "if any test-discipline rule fires, merge is
//!   blocked." Use for production framework self-application.
//! - [`TestDisciplinePreset::TddStrict`] — every rule fires `warning`.
//!   Surfaces in CI but does not block. Use for pilots ramping up.
//! - [`TestDisciplinePreset::TddMature`] — per-rule defaults stand
//!   (calibrated severity per the rule's authored intent). Use when
//!   pilots have full TDD authorship and the per-rule calibration is
//!   the right level.
//! - [`TestDisciplinePreset::Off`] — every rule fires `info`. Report-
//!   only; never gate. For prototypes that want the diagnostic visibility
//!   without any CI failure mode.
//!
//! ## Precedence
//!
//! `[doctor.test_discipline].severity_override.<RULE>` (per-rule, requires
//! `reason` per `DOCTOR-OVERRIDE-NEEDS-REASON-001`) wins over preset, which
//! wins over `SecurityProfile`-derived defaults. See
//! `crate::test_discipline::preset::resolve_test_discipline_severity`.
//!
//! ## Examples
//!
//! Parsing a preset string from `Lazurite.toml`:
//!
//! ```rust
//! use lazuli_doctor::test_discipline::preset::TestDisciplinePreset;
//!
//! assert_eq!(
//!     TestDisciplinePreset::parse("tdd-iron-hand"),
//!     Some(TestDisciplinePreset::TddIronHand)
//! );
//! assert_eq!(TestDisciplinePreset::parse("unknown"), None);
//! ```
//!
//! Looking up the severity a rule fires at under a given preset:
//!
//! ```rust
//! use lazuli_doctor::test_discipline::preset::{
//!     TestDisciplinePreset, preset_rule_severity,
//! };
//! use lazuli_doctor::DoctorSeverity;
//!
//! let sev = preset_rule_severity(
//!     TestDisciplinePreset::TddIronHand,
//!     "TEST-MISSING-AUTHORED-001",
//! );
//! assert_eq!(sev, Some(DoctorSeverity::Error));
//! ```
//!
//! ## See also
//!
//! - [`crate::coverage::CoveragePreset`] — the original preset mechanism
//!   this module mirrors, applied to coverage thresholds.
//! - [`crate::test_discipline::override_needs_reason_001`] — enforces
//!   `reason` on every entry under
//!   `[doctor.test_discipline].severity_override`.
//! - `docs/proposals/tdd-bdd-first-2026-05-23.md` Wave 1.5 — design
//!   intent.

use crate::DoctorSeverity;

/// Test-discipline preset, parsed from
/// `[doctor.test_discipline].preset = "..."`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestDisciplinePreset {
    /// Every `TEST-*` / `DOCTOR-*` rule fires at `Error`. No escape
    /// without an explicit `severity_override` entry carrying a
    /// non-empty `reason`. Editorial veto posture.
    TddIronHand,
    /// Every rule fires at `Warning`. Surfaces in CI but does not
    /// block merge unless paired with `--fail-on category:test_discipline`.
    TddStrict,
    /// Per-rule defaults stand. Calibrated severity per the rule's
    /// authored intent (e.g. `TEST-FIXTURE-LITERAL-001` is error;
    /// `TEST-PREDICATE-UNCOVERED-001` is info).
    TddMature,
    /// Every rule fires at `Info`. Report-only; never blocks anything.
    Off,
}

impl TestDisciplinePreset {
    /// Parse a preset name from `Lazurite.toml`. Returns `None` for
    /// any unknown name; callers should surface that as a config
    /// error so typos don't silently degrade enforcement.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::test_discipline::preset::TestDisciplinePreset;
    ///
    /// assert_eq!(
    ///     TestDisciplinePreset::parse("tdd-mature"),
    ///     Some(TestDisciplinePreset::TddMature)
    /// );
    /// assert_eq!(TestDisciplinePreset::parse(" off "), Some(TestDisciplinePreset::Off));
    /// assert_eq!(TestDisciplinePreset::parse(""), None);
    /// ```
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim() {
            "tdd-iron-hand" => Some(Self::TddIronHand),
            "tdd-strict" => Some(Self::TddStrict),
            "tdd-mature" => Some(Self::TddMature),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    /// Stable kebab-case identifier for round-trip serialization and
    /// JSON disclosure in `DoctorReport.coverage.preset`-style fields.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::test_discipline::preset::TestDisciplinePreset;
    ///
    /// assert_eq!(TestDisciplinePreset::TddIronHand.as_str(), "tdd-iron-hand");
    /// assert_eq!(TestDisciplinePreset::Off.as_str(), "off");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TddIronHand => "tdd-iron-hand",
            Self::TddStrict => "tdd-strict",
            Self::TddMature => "tdd-mature",
            Self::Off => "off",
        }
    }
}

/// Returns the severity that the given rule fires at under the active
/// preset. `None` means "preset has no opinion — fall back to per-rule
/// default or `SecurityProfile`-derived value".
///
/// Today the contract is uniform per preset (iron-hand → Error for all
/// codes in the catalog; strict → Warning; mature → None (defer to
/// caller); off → Info). The signature is shaped to allow per-rule
/// granularity in the future without breaking call sites — e.g. a
/// hypothetical `tdd-strict-but-fixture-literal-still-error` preset
/// would only return overrides for matching codes.
///
/// ## Catalog
///
/// The rules currently in scope (all under
/// [`crate::test_discipline`]):
///
/// - `TEST-MISSING-AUTHORED-001` / `TEST-PREDICATE-UNCOVERED-001` /
///   `TEST-RESTATES-EFFECT-001` / `TEST-RESTATES-POLICY-001` /
///   `TEST-FIXTURE-LITERAL-001` — Wave 1 TDD/BDD catalog.
/// - `MIGRATION-DSL-UNIQUE-001` / `RUNTIME-UPDATE-BUILDER-JSONB-001` —
///   bug-driven rules from the canonical pilot §7.1.
/// - `DOCTOR-OVERRIDE-NEEDS-REASON-001` — Wave 0.5 meta-rule.
/// - `TEST-STUB-001` — Wave 3 scaffold marker rule.
/// - `TEST-VIEW-E2E-MISSING-001` / `TEST-VIEW-EXTENSIBILITY-001` /
///   `TEST-VIEW-DRIFT-001` — Wave 3.5 + 4 view rules.
/// - `TEST-COMMAND-ASSERTION-DRIFT-001` — Wave 4 §7.1 widening.
/// - `TEST-HANDLER-MISSING-001` — Wave 5 twin rule.
///
/// Unknown rule codes (not `TEST-*` / `DOCTOR-*` / `MIGRATION-*` /
/// `RUNTIME-*` prefixed) return `None` regardless of preset — the preset
/// only governs the test-discipline category.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::test_discipline::preset::{
///     TestDisciplinePreset, preset_rule_severity,
/// };
/// use lazuli_doctor::DoctorSeverity;
///
/// // Iron-hand escalates every test-discipline rule to error.
/// assert_eq!(
///     preset_rule_severity(TestDisciplinePreset::TddIronHand, "TEST-FIXTURE-LITERAL-001"),
///     Some(DoctorSeverity::Error),
/// );
///
/// // `off` reports without gating.
/// assert_eq!(
///     preset_rule_severity(TestDisciplinePreset::Off, "TEST-MISSING-AUTHORED-001"),
///     Some(DoctorSeverity::Info),
/// );
///
/// // `tdd-mature` defers to per-rule defaults.
/// assert_eq!(
///     preset_rule_severity(TestDisciplinePreset::TddMature, "TEST-MISSING-AUTHORED-001"),
///     None,
/// );
///
/// // Codes outside the category are unaffected.
/// assert_eq!(
///     preset_rule_severity(TestDisciplinePreset::TddIronHand, "VOCAB-TESTS-MISSING-001"),
///     None,
/// );
/// ```
pub fn preset_rule_severity(preset: TestDisciplinePreset, code: &str) -> Option<DoctorSeverity> {
    if !is_test_discipline_code(code) {
        return None;
    }
    match preset {
        TestDisciplinePreset::TddIronHand => Some(DoctorSeverity::Error),
        TestDisciplinePreset::TddStrict => Some(DoctorSeverity::Warning),
        TestDisciplinePreset::TddMature => None,
        TestDisciplinePreset::Off => Some(DoctorSeverity::Info),
    }
}

/// Returns `true` if the rule code belongs to the test_discipline
/// category. Prefix-based; matches the dispatcher convention.
fn is_test_discipline_code(code: &str) -> bool {
    matches!(
        code.split('-').next(),
        Some("TEST") | Some("DOCTOR") | Some("MIGRATION") | Some("RUNTIME"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_iron_hand_round_trip() {
        let p = TestDisciplinePreset::parse("tdd-iron-hand").unwrap();
        assert_eq!(p, TestDisciplinePreset::TddIronHand);
        assert_eq!(p.as_str(), "tdd-iron-hand");
    }

    #[test]
    fn parse_strips_whitespace() {
        assert_eq!(
            TestDisciplinePreset::parse("  tdd-strict  "),
            Some(TestDisciplinePreset::TddStrict),
        );
    }

    #[test]
    fn parse_rejects_unknown() {
        assert_eq!(TestDisciplinePreset::parse("tdd-loose"), None);
        assert_eq!(TestDisciplinePreset::parse(""), None);
    }

    #[test]
    fn iron_hand_escalates_all_test_discipline_codes() {
        let codes = [
            "TEST-MISSING-AUTHORED-001",
            "TEST-FIXTURE-LITERAL-001",
            "TEST-STUB-001",
            "TEST-VIEW-E2E-MISSING-001",
            "TEST-HANDLER-MISSING-001",
            "TEST-COMMAND-ASSERTION-DRIFT-001",
            "DOCTOR-OVERRIDE-NEEDS-REASON-001",
            "MIGRATION-DSL-UNIQUE-001",
            "RUNTIME-UPDATE-BUILDER-JSONB-001",
        ];
        for code in codes {
            assert_eq!(
                preset_rule_severity(TestDisciplinePreset::TddIronHand, code),
                Some(DoctorSeverity::Error),
                "code {} should escalate to Error under tdd-iron-hand",
                code,
            );
        }
    }

    #[test]
    fn strict_returns_warning_uniformly() {
        assert_eq!(
            preset_rule_severity(TestDisciplinePreset::TddStrict, "TEST-MISSING-AUTHORED-001",),
            Some(DoctorSeverity::Warning),
        );
    }

    #[test]
    fn mature_defers_to_per_rule_default() {
        assert_eq!(
            preset_rule_severity(TestDisciplinePreset::TddMature, "TEST-FIXTURE-LITERAL-001",),
            None,
        );
    }

    #[test]
    fn off_returns_info_uniformly() {
        assert_eq!(
            preset_rule_severity(TestDisciplinePreset::Off, "TEST-STUB-001"),
            Some(DoctorSeverity::Info),
        );
    }

    #[test]
    fn other_categories_return_none_under_any_preset() {
        for preset in [
            TestDisciplinePreset::TddIronHand,
            TestDisciplinePreset::TddStrict,
            TestDisciplinePreset::TddMature,
            TestDisciplinePreset::Off,
        ] {
            assert_eq!(
                preset_rule_severity(preset, "VOCAB-TESTS-MISSING-001"),
                None,
                "vocab codes should be unaffected by test_discipline preset {:?}",
                preset,
            );
            assert_eq!(
                preset_rule_severity(preset, "ERR-VOCAB-001"),
                None,
                "error_vocab codes should be unaffected by preset {:?}",
                preset,
            );
        }
    }
}
