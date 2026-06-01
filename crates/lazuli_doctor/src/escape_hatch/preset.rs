//! Preset -> severity resolution for the `escape_hatch` category.
//!
//! `[doctor.escape_hatch].preset` in `Lazurite.toml` selects one of four
//! presets; each maps a rule's natural severity to an effective
//! [`DoctorSeverity`]. The dispatcher calls [`preset_rule_severity`] to
//! resolve the severity for each finding.
//!
//! Note on `ESC-RAWSQL-IN-HANDLER-001`: the preset can escalate/relax its
//! *display* severity like any rule, but its waivable-to-convert (not
//! waivable-to-silence) contract is enforced in the rule body, independent
//! of the preset.

use crate::severity::DoctorSeverity;

/// The four severity presets for escape-hatch rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeHatchPreset {
    /// Every rule fires at `Error`.
    TddIronHand,
    /// Every rule fires at `Warning`.
    TddStrict,
    /// Per-rule defaults.
    TddMature,
    /// Every rule fires at `Info`.
    Off,
}

impl EscapeHatchPreset {
    /// Parse a preset name from config; `None` for unknown.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "tdd-iron-hand" => Some(Self::TddIronHand),
            "tdd-strict" => Some(Self::TddStrict),
            "tdd-mature" => Some(Self::TddMature),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

/// Resolve the effective severity for a rule under a preset.
///
/// `default` is the rule's natural severity (used by `TddMature`).
pub fn preset_rule_severity(preset: EscapeHatchPreset, default: DoctorSeverity) -> DoctorSeverity {
    match preset {
        EscapeHatchPreset::TddIronHand => DoctorSeverity::Error,
        EscapeHatchPreset::TddStrict => DoctorSeverity::Warning,
        EscapeHatchPreset::TddMature => default,
        EscapeHatchPreset::Off => DoctorSeverity::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trips_known_presets() {
        assert_eq!(
            EscapeHatchPreset::parse("tdd-iron-hand"),
            Some(EscapeHatchPreset::TddIronHand)
        );
        assert_eq!(EscapeHatchPreset::parse("off"), Some(EscapeHatchPreset::Off));
        assert_eq!(EscapeHatchPreset::parse("nope"), None);
    }

    #[test]
    fn iron_hand_escalates_to_error() {
        assert_eq!(
            preset_rule_severity(EscapeHatchPreset::TddIronHand, DoctorSeverity::Warning),
            DoctorSeverity::Error
        );
    }

    #[test]
    fn mature_keeps_default() {
        assert_eq!(
            preset_rule_severity(EscapeHatchPreset::TddMature, DoctorSeverity::Warning),
            DoctorSeverity::Warning
        );
    }
}
