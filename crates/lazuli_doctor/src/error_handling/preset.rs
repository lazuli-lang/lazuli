//! `[doctor.error_handling].preset` — mirror of
//! [`crate::internal_hygiene::preset`] for error-handling rules.
//!
//! Under `tdd-iron-hand`, every error-handling rule fires at `Error`
//! regardless of profile. Editorial veto via one TOML line at workspace
//! root.
//!
//! ## Code prefixes governed
//!
//! - `INTERNAL-PANIC-*` (framework Rust)
//! - `INTERNAL-ERROR-*` (framework Rust)
//! - `ERROR-*` (`.lzi` / `.lzx`)
//! - `HANDLER-*` (Go handlers)
//! - `JOB-*` (`.lzi` jobs — declarative bodies the runtime can't execute)
//!
//! All other prefixes pass through with `None` (the preset has no
//! opinion; per-rule default stands).
//!
//! ## Examples
//!
//! ```rust
//! use lazuli_doctor::error_handling::preset::{
//!     ErrorHandlingPreset, preset_rule_severity,
//! };
//! use lazuli_doctor::DoctorSeverity;
//!
//! let sev = preset_rule_severity(
//!     ErrorHandlingPreset::TddIronHand,
//!     "INTERNAL-PANIC-UNWRAP-001",
//! );
//! assert_eq!(sev, Some(DoctorSeverity::Error));
//! ```

use crate::DoctorSeverity;

/// Error-handling preset, parsed from
/// `[doctor.error_handling].preset = "..."`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorHandlingPreset {
    /// Every error-handling rule fires at `Error`.
    TddIronHand,
    /// Every rule fires at `Warning`.
    TddStrict,
    /// Per-rule defaults stand.
    TddMature,
    /// Every rule fires at `Info` — report-only.
    Off,
}

impl ErrorHandlingPreset {
    /// Parse a preset name. Returns `None` for unknown input.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::error_handling::preset::ErrorHandlingPreset;
    ///
    /// assert_eq!(
    ///     ErrorHandlingPreset::parse("tdd-iron-hand"),
    ///     Some(ErrorHandlingPreset::TddIronHand),
    /// );
    /// assert_eq!(ErrorHandlingPreset::parse("nope"), None);
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
    /// use lazuli_doctor::error_handling::preset::ErrorHandlingPreset;
    ///
    /// assert_eq!(ErrorHandlingPreset::TddIronHand.as_str(), "tdd-iron-hand");
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

/// Returns the severity that the given error-handling rule fires at
/// under the active preset. `None` means "preset has no opinion —
/// fall back to per-rule default".
///
/// Codes outside the error-handling prefix set always return `None`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::error_handling::preset::{
///     ErrorHandlingPreset, preset_rule_severity,
/// };
/// use lazuli_doctor::DoctorSeverity;
///
/// // Framework Rust panic rule.
/// assert_eq!(
///     preset_rule_severity(ErrorHandlingPreset::TddIronHand, "INTERNAL-PANIC-UNWRAP-001"),
///     Some(DoctorSeverity::Error),
/// );
///
/// // .lzi contract rule.
/// assert_eq!(
///     preset_rule_severity(ErrorHandlingPreset::TddIronHand, "ERROR-DECLARED-EXHAUSTIVE-001"),
///     Some(DoctorSeverity::Error),
/// );
///
/// // Go handler rule.
/// assert_eq!(
///     preset_rule_severity(ErrorHandlingPreset::TddIronHand, "HANDLER-NO-PANIC-001"),
///     Some(DoctorSeverity::Error),
/// );
///
/// // Other categories pass through.
/// assert_eq!(
///     preset_rule_severity(ErrorHandlingPreset::TddIronHand, "VOCAB-TESTS-MISSING-001"),
///     None,
/// );
///
/// // tdd-mature defers to per-rule default.
/// assert_eq!(
///     preset_rule_severity(ErrorHandlingPreset::TddMature, "HANDLER-NO-PANIC-001"),
///     None,
/// );
/// ```
pub fn preset_rule_severity(preset: ErrorHandlingPreset, code: &str) -> Option<DoctorSeverity> {
    if !is_error_handling_code(code) {
        return None;
    }
    match preset {
        ErrorHandlingPreset::TddIronHand => Some(DoctorSeverity::Error),
        ErrorHandlingPreset::TddStrict => Some(DoctorSeverity::Warning),
        ErrorHandlingPreset::TddMature => None,
        ErrorHandlingPreset::Off => Some(DoctorSeverity::Info),
    }
}

fn is_error_handling_code(code: &str) -> bool {
    let mut parts = code.split('-');
    match (parts.next(), parts.next()) {
        // INTERNAL-PANIC-* and INTERNAL-ERROR-* are error_handling, not
        // internal_hygiene (matching the rule_category routing).
        (Some("INTERNAL"), Some("PANIC")) => true,
        (Some("INTERNAL"), Some("ERROR")) => true,
        (Some("ERROR"), _) => true,
        (Some("HANDLER"), _) => true,
        // JOB-* — runtime-execution gaps on declarative job bodies. A
        // declarative job that lowers to a no-op silently drops the work
        // the author declared, the job-level twin of a swallowed handler
        // error (matching the `rule_category` routing of `JOB` ->
        // `ErrorHandling`).
        (Some("JOB"), _) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        for variant in [
            ErrorHandlingPreset::TddIronHand,
            ErrorHandlingPreset::TddStrict,
            ErrorHandlingPreset::TddMature,
            ErrorHandlingPreset::Off,
        ] {
            assert_eq!(ErrorHandlingPreset::parse(variant.as_str()), Some(variant));
        }
    }

    #[test]
    fn iron_hand_escalates_all_error_handling_codes() {
        let codes = [
            "INTERNAL-PANIC-UNWRAP-001",
            "INTERNAL-ERROR-NAMING-001",
            "INTERNAL-ERROR-NON-EXHAUSTIVE-001",
            "INTERNAL-ERROR-VARIANT-DOC-001",
            "ERROR-DECLARED-EXHAUSTIVE-001",
            "ERROR-MESSAGE-KEY-001",
            "ERROR-HTTP-STATUS-MAP-001",
            "ERROR-RETRIABLE-CLASS-001",
            "ERROR-AUDIT-EMIT-001",
            "ERROR-VIEW-ON-ERROR-001",
            "ERROR-VIEW-EMPTY-STATE-001",
            "ERROR-FIELD-VALIDATION-001",
            "HANDLER-NO-PANIC-001",
            "HANDLER-NO-STRING-ERROR-001",
            "HANDLER-ERROR-WRAP-001",
            "JOB-DECLARATIVE-BODY-UNSUPPORTED-001",
        ];
        for code in codes {
            assert_eq!(
                preset_rule_severity(ErrorHandlingPreset::TddIronHand, code),
                Some(DoctorSeverity::Error),
                "iron-hand should escalate {code}"
            );
        }
    }

    #[test]
    fn job_prefix_warns_under_strict_errors_under_iron_hand() {
        // The exact per-profile contract this rule was added for.
        assert_eq!(
            preset_rule_severity(
                ErrorHandlingPreset::TddStrict,
                "JOB-DECLARATIVE-BODY-UNSUPPORTED-001"
            ),
            Some(DoctorSeverity::Warning),
        );
        assert_eq!(
            preset_rule_severity(
                ErrorHandlingPreset::TddIronHand,
                "JOB-DECLARATIVE-BODY-UNSUPPORTED-001"
            ),
            Some(DoctorSeverity::Error),
        );
    }

    #[test]
    fn non_error_handling_codes_unaffected() {
        for code in [
            "VOCAB-TESTS-MISSING-001",
            "INTERNAL-FILE-SIZE-001",
            "INTERNAL-UNDOC-PUB-001",
            "ERR-VOCAB-001",
            "TEST-MISSING-AUTHORED-001",
        ] {
            assert_eq!(
                preset_rule_severity(ErrorHandlingPreset::TddIronHand, code),
                None,
                "preset should not touch {code}"
            );
        }
    }

    #[test]
    fn mature_defers_to_per_rule_default() {
        assert_eq!(
            preset_rule_severity(ErrorHandlingPreset::TddMature, "INTERNAL-PANIC-UNWRAP-001"),
            None,
        );
    }
}
