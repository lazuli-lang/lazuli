//! Shared severity enum for doctor rules.
//!
//! Historically `lazuli_cli::doctor::DoctorSeverity` was private to
//! the CLI crate because rules emitted `Finding` values and the
//! dispatcher classified severity downstream. Wave 1.5 surfaces a
//! shared `DoctorSeverity` in `lazuli_doctor` so the preset machinery
//! ([`crate::test_discipline::preset`], later
//! `crate::internal_hygiene::preset`) can return severities directly —
//! the dispatcher then maps `DoctorSeverity` → `DoctorDiagnostic.severity`
//! 1:1.
//!
//! ## Variants
//!
//! - [`DoctorSeverity::Error`] — blocks merge / fails CI / exit code 1.
//! - [`DoctorSeverity::Warning`] — visible in text output and JSON,
//!   does NOT block by default (only with `--fail-on category:X`).
//! - [`DoctorSeverity::Info`] — report-only; never affects exit code.
//! - [`DoctorSeverity::Hint`] — IDE-only soft suggestion; rendered as
//!   underline-only in rust-analyzer, not as a squiggle.
//!
//! ## Examples
//!
//! Round-trip via the kebab-case identifier:
//!
//! ```rust
//! use lazuli_doctor::DoctorSeverity;
//!
//! assert_eq!(DoctorSeverity::Error.as_str(), "error");
//! assert_eq!(DoctorSeverity::parse("warning"), Some(DoctorSeverity::Warning));
//! ```
//!
//! ## See also
//!
//! - [`crate::test_discipline::preset::preset_rule_severity`] — returns
//!   `Option<DoctorSeverity>` to indicate "preset has an opinion" vs
//!   "defer to per-rule default".
//! - `lazuli_cli::doctor::DoctorDiagnostic.severity` — the canonical
//!   field this enum maps to in the dispatcher.

use serde::{Deserialize, Serialize};

/// Doctor diagnostic severity. Kept in lock-step with the
/// `lazuli_cli::doctor::DoctorSeverity` enum used by the dispatcher;
/// rules in `lazuli_doctor` and `lazuli_lsp` use this shared variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorSeverity {
    /// Blocks merge / fails CI / exit code 1.
    Error,
    /// Visible but non-blocking by default.
    Warning,
    /// Report-only; never affects exit code.
    Info,
    /// IDE-only soft suggestion.
    Hint,
}

impl DoctorSeverity {
    /// Stable kebab-case identifier (matches the
    /// `severity = "..."` TOML field and JSON output `severity` field).
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::DoctorSeverity;
    ///
    /// assert_eq!(DoctorSeverity::Error.as_str(), "error");
    /// assert_eq!(DoctorSeverity::Warning.as_str(), "warning");
    /// assert_eq!(DoctorSeverity::Hint.as_str(), "hint");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Hint => "hint",
        }
    }

    /// Parse the kebab-case identifier. Returns `None` on unknown input.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::DoctorSeverity;
    ///
    /// assert_eq!(DoctorSeverity::parse("warning"), Some(DoctorSeverity::Warning));
    /// assert_eq!(DoctorSeverity::parse("  hint  "), Some(DoctorSeverity::Hint));
    /// assert_eq!(DoctorSeverity::parse("blocker"), None);
    /// ```
    pub fn parse(input: &str) -> Option<Self> {
        match input.trim() {
            "error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "info" => Some(Self::Info),
            "hint" => Some(Self::Hint),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_round_trips_through_parse() {
        for sev in [
            DoctorSeverity::Error,
            DoctorSeverity::Warning,
            DoctorSeverity::Info,
            DoctorSeverity::Hint,
        ] {
            assert_eq!(DoctorSeverity::parse(sev.as_str()), Some(sev));
        }
    }

    #[test]
    fn parse_strips_whitespace() {
        assert_eq!(
            DoctorSeverity::parse("  error  "),
            Some(DoctorSeverity::Error),
        );
    }

    #[test]
    fn parse_rejects_unknown() {
        assert_eq!(DoctorSeverity::parse("blocker"), None);
        assert_eq!(DoctorSeverity::parse(""), None);
    }
}
