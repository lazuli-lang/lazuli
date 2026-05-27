//! `[doctor.lzi_hygiene].preset` — mirror of
//! [`crate::error_handling::preset`] for `.lzi` source-shape rules.
//!
//! Under `tdd-iron-hand`, every `LZI-*` rule fires at `Error`. The
//! escape hatch for legitimately-deviant cases is the same as every
//! other doctor category — per-rule `severity_override` with a `reason`
//! string per `DOCTOR-OVERRIDE-NEEDS-REASON-001`.
//!
//! ## Examples
//!
//! ```rust
//! use lazuli_doctor::lzi_hygiene::preset::{
//!     LziHygienePreset, preset_rule_severity,
//! };
//! use lazuli_doctor::DoctorSeverity;
//!
//! let sev = preset_rule_severity(
//!     LziHygienePreset::TddIronHand,
//!     "LZI-FILE-SIZE-001",
//! );
//! assert_eq!(sev, Some(DoctorSeverity::Error));
//! ```

use crate::DoctorSeverity;

/// `.lzi` hygiene preset, parsed from
/// `[doctor.lzi_hygiene].preset = "..."`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LziHygienePreset {
    /// Every `LZI-*` rule fires at `Error`. Workspace-wide editorial
    /// veto.
    TddIronHand,
    /// Every rule fires at `Warning`.
    TddStrict,
    /// Per-rule defaults stand.
    TddMature,
    /// Every rule fires at `Info` — report-only.
    Off,
}

impl LziHygienePreset {
    /// Parse a preset name from TOML. `None` for unknown input.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::lzi_hygiene::preset::LziHygienePreset;
    ///
    /// assert_eq!(
    ///     LziHygienePreset::parse("tdd-iron-hand"),
    ///     Some(LziHygienePreset::TddIronHand),
    /// );
    /// assert_eq!(LziHygienePreset::parse("nope"), None);
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

    /// Stable kebab-case identifier.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::lzi_hygiene::preset::LziHygienePreset;
    ///
    /// assert_eq!(LziHygienePreset::TddIronHand.as_str(), "tdd-iron-hand");
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

/// Returns the severity that the given `.lzi` hygiene rule fires at
/// under the active preset. `None` means "preset has no opinion — fall
/// back to per-rule default".
///
/// Codes outside the `LZI-*` prefix always return `None`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::preset::{
///     LziHygienePreset, preset_rule_severity,
/// };
/// use lazuli_doctor::DoctorSeverity;
///
/// assert_eq!(
///     preset_rule_severity(LziHygienePreset::TddIronHand, "LZI-FILE-SIZE-001"),
///     Some(DoctorSeverity::Error),
/// );
/// assert_eq!(
///     preset_rule_severity(LziHygienePreset::TddIronHand, "INTERNAL-PANIC-UNWRAP-001"),
///     None,
/// );
/// assert_eq!(
///     preset_rule_severity(LziHygienePreset::TddMature, "LZI-FEATURE-COHESION-001"),
///     None,
/// );
/// ```
pub fn preset_rule_severity(preset: LziHygienePreset, code: &str) -> Option<DoctorSeverity> {
    if !is_lzi_hygiene_code(code) {
        return None;
    }
    match preset {
        LziHygienePreset::TddIronHand => Some(DoctorSeverity::Error),
        LziHygienePreset::TddStrict => Some(DoctorSeverity::Warning),
        LziHygienePreset::TddMature => None,
        LziHygienePreset::Off => Some(DoctorSeverity::Info),
    }
}

fn is_lzi_hygiene_code(code: &str) -> bool {
    matches!(code.split('-').next(), Some("LZI"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        for variant in [
            LziHygienePreset::TddIronHand,
            LziHygienePreset::TddStrict,
            LziHygienePreset::TddMature,
            LziHygienePreset::Off,
        ] {
            assert_eq!(LziHygienePreset::parse(variant.as_str()), Some(variant));
        }
    }

    #[test]
    fn iron_hand_escalates_all_lzi_codes() {
        for code in [
            "LZI-FILE-SIZE-001",
            "LZI-FEATURE-NAMING-MATCHES-FILE-001",
            "LZI-FEATURE-COHESION-001",
        ] {
            assert_eq!(
                preset_rule_severity(LziHygienePreset::TddIronHand, code),
                Some(DoctorSeverity::Error),
                "iron-hand should escalate {code}"
            );
        }
    }

    #[test]
    fn non_lzi_codes_unaffected() {
        for code in [
            "INTERNAL-PANIC-UNWRAP-001",
            "INTERNAL-FILE-SIZE-001",
            "ERR-VOCAB-001",
            "HANDLER-NO-PANIC-001",
        ] {
            assert_eq!(
                preset_rule_severity(LziHygienePreset::TddIronHand, code),
                None,
                "preset should not touch {code}"
            );
        }
    }

    #[test]
    fn mature_defers_to_per_rule_default() {
        assert_eq!(
            preset_rule_severity(LziHygienePreset::TddMature, "LZI-FILE-SIZE-001"),
            None,
        );
    }

    #[test]
    fn strict_warns_all_lzi() {
        assert_eq!(
            preset_rule_severity(LziHygienePreset::TddStrict, "LZI-FILE-SIZE-001"),
            Some(DoctorSeverity::Warning),
        );
    }
}
