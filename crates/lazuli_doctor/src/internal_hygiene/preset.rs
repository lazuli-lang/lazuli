//! `[doctor.internal_hygiene].preset` — mirror of
//! [`crate::test_discipline::preset`] for internal-hygiene rules.
//!
//! Under `tdd-iron-hand`, every `INTERNAL-*` rule fires at `Error`
//! regardless of profile. Editorial veto for the framework's CI:
//! a single TOML line at workspace root blocks merge on file-size
//! violations, missing rustdoc, absent `## Examples`, unpaired tests.
//!
//! ## Examples
//!
//! ```rust
//! use lazuli_doctor::internal_hygiene::preset::{
//!     InternalHygienePreset, preset_rule_severity,
//! };
//! use lazuli_doctor::DoctorSeverity;
//!
//! let sev = preset_rule_severity(
//!     InternalHygienePreset::TddIronHand,
//!     "INTERNAL-UNDOC-PUB-001",
//! );
//! assert_eq!(sev, Some(DoctorSeverity::Error));
//! ```
//!
//! ## See also
//!
//! - [`crate::test_discipline::preset`] — the test_discipline preset
//!   this module mirrors.
//! - [`crate::internal_hygiene`] module doc for catalog of rules
//!   governed by this preset.

use crate::DoctorSeverity;

/// Internal-hygiene preset, parsed from
/// `[doctor.internal_hygiene].preset = "..."`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalHygienePreset {
    /// Every `INTERNAL-*` rule fires at `Error`. Editorial veto for
    /// the framework's own CI.
    TddIronHand,
    /// Every rule fires at `Warning`. Visible in CI but does not
    /// block by default.
    TddStrict,
    /// Per-rule defaults stand.
    TddMature,
    /// Every rule fires at `Info`. Report-only.
    Off,
}

impl InternalHygienePreset {
    /// Parse a preset name. Returns `None` for unknown input. The
    /// caller surfaces unknowns as `Lazurite.toml` schema errors rather
    /// than silently picking a default.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::internal_hygiene::preset::InternalHygienePreset;
    ///
    /// assert_eq!(
    ///     InternalHygienePreset::parse("tdd-iron-hand"),
    ///     Some(InternalHygienePreset::TddIronHand),
    /// );
    /// assert_eq!(InternalHygienePreset::parse("loose"), None);
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

    /// Stable kebab-case identifier — the name authors write in
    /// `Lazurite.toml`. Round-trips with [`parse`](Self::parse).
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::internal_hygiene::preset::InternalHygienePreset;
    ///
    /// assert_eq!(InternalHygienePreset::TddIronHand.as_str(), "tdd-iron-hand");
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

/// Returns the severity that the given `INTERNAL-*` rule fires at
/// under the active preset. `None` means "preset has no opinion —
/// fall back to per-rule default".
///
/// Non-`INTERNAL-*` codes always return `None` regardless of preset
/// (this preset only governs the internal-hygiene category).
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::internal_hygiene::preset::{
///     InternalHygienePreset, preset_rule_severity,
/// };
/// use lazuli_doctor::DoctorSeverity;
///
/// assert_eq!(
///     preset_rule_severity(InternalHygienePreset::TddIronHand, "INTERNAL-FILE-SIZE-001"),
///     Some(DoctorSeverity::Error),
/// );
///
/// // Other categories pass through unaffected.
/// assert_eq!(
///     preset_rule_severity(InternalHygienePreset::TddIronHand, "VOCAB-TESTS-MISSING-001"),
///     None,
/// );
///
/// // tdd-mature defers to per-rule default.
/// assert_eq!(
///     preset_rule_severity(InternalHygienePreset::TddMature, "INTERNAL-UNDOC-PUB-001"),
///     None,
/// );
/// ```
pub fn preset_rule_severity(preset: InternalHygienePreset, code: &str) -> Option<DoctorSeverity> {
    if !is_internal_hygiene_code(code) {
        return None;
    }
    match preset {
        InternalHygienePreset::TddIronHand => Some(DoctorSeverity::Error),
        InternalHygienePreset::TddStrict => Some(DoctorSeverity::Warning),
        InternalHygienePreset::TddMature => None,
        InternalHygienePreset::Off => Some(DoctorSeverity::Info),
    }
}

fn is_internal_hygiene_code(code: &str) -> bool {
    matches!(code.split('-').next(), Some("INTERNAL"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        for variant in [
            InternalHygienePreset::TddIronHand,
            InternalHygienePreset::TddStrict,
            InternalHygienePreset::TddMature,
            InternalHygienePreset::Off,
        ] {
            assert_eq!(
                InternalHygienePreset::parse(variant.as_str()),
                Some(variant)
            );
        }
    }

    #[test]
    fn iron_hand_escalates_all_internal_codes() {
        let codes = [
            "INTERNAL-FILE-SIZE-001",
            "INTERNAL-UNDOC-PUB-001",
            "INTERNAL-NO-EXAMPLE-001",
            "INTERNAL-TEST-PAIRING-001",
        ];
        for code in codes {
            assert_eq!(
                preset_rule_severity(InternalHygienePreset::TddIronHand, code),
                Some(DoctorSeverity::Error),
            );
        }
    }

    #[test]
    fn other_category_codes_unaffected() {
        assert_eq!(
            preset_rule_severity(
                InternalHygienePreset::TddIronHand,
                "TEST-MISSING-AUTHORED-001",
            ),
            None,
        );
    }

    #[test]
    fn mature_defers_to_per_rule_default() {
        assert_eq!(
            preset_rule_severity(InternalHygienePreset::TddMature, "INTERNAL-FILE-SIZE-001"),
            None,
        );
    }
}
