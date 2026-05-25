//! Leaf utilities shared across the doctor dispatch pipeline.
//!
//! Three clusters live here, none of which carry behavior:
//!
//! 1. **Severity bridging** — `From<lazuli_doctor::DoctorSeverity> for
//!    DoctorSeverity` plus the two preset-aware resolvers
//!    (`resolve_test_discipline_severity`,
//!    `resolve_internal_hygiene_severity`) that translate
//!    `[doctor.<category>].preset` overrides into the CLI's private
//!    severity enum. Centralized so every dispatch site escalates
//!    uniformly (e.g. `tdd-iron-hand` promotes everything to `Error`
//!    in one place).
//! 2. **TOML override parsing** — `parse_doctor_severity` accepts the
//!    `"error" | "warning" | "warn" | "info" | "hint"` vocabulary
//!    authored in `Lazurite.toml` `[doctor.*.severity_override]`
//!    tables. Returning `Option` lets callers fall back to the
//!    per-category default when an override is malformed.
//! 3. **CLI flag parsing** — `parse_doctor_format` and
//!    `parse_fail_on_specs` translate the wire-level `--format` /
//!    `--fail-on` strings into the typed shapes defined by
//!    `crate::doctor_report`.
//! 4. **Project-root resolution** — `doctor_project_root` and
//!    `project_has_lazurite_manifest` answer "where does this project
//!    live?" for any `lazuli doctor <input>` invocation. Single-file
//!    inputs resolve to the file's parent directory so manifest
//!    lookups still work; directory inputs pass through.
//!
//! Everything here is `pub(super)` — the doctor module owns its
//! private severity vocabulary and these helpers never escape past
//! `crate::doctor::*`.

use std::path::{Path, PathBuf};

use super::DoctorSeverity;

impl From<lazuli_doctor::DoctorSeverity> for DoctorSeverity {
    /// W1.5 — bridge from the shared `lazuli_doctor::DoctorSeverity`
    /// returned by preset machinery into the CLI's private severity
    /// enum used by `DoctorDiagnostic`. 1:1 mapping; variants line up.
    fn from(severity: lazuli_doctor::DoctorSeverity) -> Self {
        match severity {
            lazuli_doctor::DoctorSeverity::Error => Self::Error,
            lazuli_doctor::DoctorSeverity::Warning => Self::Warning,
            lazuli_doctor::DoctorSeverity::Info => Self::Info,
            lazuli_doctor::DoctorSeverity::Hint => Self::Hint,
        }
    }
}

/// W1.5 — resolve effective severity for a test-discipline rule under
/// the active `[doctor.test_discipline].preset`. Returns the preset's
/// opinion when one exists, otherwise the caller-supplied default
/// (typically the per-rule severity calibrated by the rule's authored
/// intent).
///
/// Centralized helper so every test-discipline dispatch site
/// (`test_discipline_diagnostics`, `.lzx`-loop view rules, future
/// per-file dispatchers) escalates uniformly under `tdd-iron-hand`.
pub(super) fn resolve_test_discipline_severity(
    default: DoctorSeverity,
    code: &str,
    preset: Option<lazuli_doctor::test_discipline::preset::TestDisciplinePreset>,
) -> DoctorSeverity {
    if let Some(preset) = preset {
        if let Some(override_sev) =
            lazuli_doctor::test_discipline::preset::preset_rule_severity(preset, code)
        {
            return override_sev.into();
        }
    }
    default
}

/// W3 — mirror of `resolve_test_discipline_severity` for the
/// internal-hygiene category. Under `tdd-iron-hand`, every `INTERNAL-*`
/// rule escalates to `Error`; otherwise the per-rule default carried
/// by the dispatcher stands.
pub(super) fn resolve_internal_hygiene_severity(
    default: DoctorSeverity,
    code: &str,
    preset: Option<lazuli_doctor::internal_hygiene::preset::InternalHygienePreset>,
) -> DoctorSeverity {
    if let Some(preset) = preset {
        if let Some(override_sev) =
            lazuli_doctor::internal_hygiene::preset::preset_rule_severity(preset, code)
        {
            return override_sev.into();
        }
    }
    default
}

/// Parse a TOML override string (`"warning"`, `"error"`, …) into a
/// `DoctorSeverity`. Returns `None` for unrecognized strings; callers
/// fall back to the category default in that case.
pub(super) fn parse_doctor_severity(s: &str) -> Option<DoctorSeverity> {
    match s.to_ascii_lowercase().as_str() {
        "error" => Some(DoctorSeverity::Error),
        "warning" | "warn" => Some(DoctorSeverity::Warning),
        "info" => Some(DoctorSeverity::Info),
        "hint" => Some(DoctorSeverity::Hint),
        _ => None,
    }
}

/// Translate the wire-level `--format` string into a typed
/// `DoctorFormat`. Unknown values fall back to `Text` (the historical
/// default) so a typo at the CLI never short-circuits the pipeline.
pub(super) fn parse_doctor_format(input: Option<&str>) -> crate::doctor_report::DoctorFormat {
    use crate::doctor_report::DoctorFormat;
    match input.unwrap_or("text").to_ascii_lowercase().as_str() {
        "text" => DoctorFormat::Text,
        "json" => DoctorFormat::Json,
        "ndjson" => DoctorFormat::Ndjson,
        "auto" => DoctorFormat::Auto,
        _ => DoctorFormat::Text,
    }
}

/// Parse every `--fail-on <spec>` flag into a `FailOnSpec`. Errors
/// surface as `String` so callers can wrap them with their own
/// `anyhow::anyhow!("--fail-on: {e}")` context.
pub(super) fn parse_fail_on_specs(
    inputs: &[String],
) -> Result<Vec<crate::doctor_report::FailOnSpec>, String> {
    inputs
        .iter()
        .map(|s| crate::doctor_report::FailOnSpec::parse(s))
        .collect()
}

/// Resolve the project-root directory for a `lazuli doctor <input>`
/// invocation. Directory inputs pass through unchanged; file inputs
/// resolve to the parent directory so `Lazurite.toml` / `app.lzi`
/// lookups still find the project. Inputs without a parent fall back
/// to `.` (single-file invocation from the repo root).
pub(super) fn doctor_project_root(input: &Path) -> PathBuf {
    if input.is_dir() {
        return input.to_path_buf();
    }

    input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

/// `true` when the resolved project root carries a `Lazurite.toml`.
/// Used by manifest-aware diagnostics to short-circuit on single-file
/// or scratch-directory invocations that have no manifest to honor.
pub(crate) fn project_has_lazurite_manifest(project_root: &Path) -> bool {
    project_root.join("Lazurite.toml").is_file()
}
