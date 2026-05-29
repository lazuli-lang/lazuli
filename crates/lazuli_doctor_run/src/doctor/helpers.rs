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
//! 2. **TOML override parsing** — severity-string parsing now lives in
//!    `lazuli_doctor_config::parse_severity` (the shared resolver crate),
//!    consumed transitively through `effective_severity` /
//!    `category_preset_severity`. The CLI no longer carries its own
//!    `parse_doctor_severity` copy.
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

use lazuli_doctor::RuleCategory;
use lazuli_doctor_config::{ResolvedDoctorConfig, category_preset_severity};

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
    // W1 — share the category-preset escalation (precedence level 3) via
    // `lazuli_doctor_config`. This site applies ONLY that level over the
    // per-rule `default` (no profile default, no manifest override), so
    // it calls `category_preset_severity` rather than the full
    // `effective_severity`. Identical answer: for TestDiscipline with
    // this preset, the shared fn returns
    // `preset_rule_severity(preset, code)`.
    let config = ResolvedDoctorConfig {
        test_discipline_preset: preset,
        ..ResolvedDoctorConfig::default()
    };
    category_preset_severity(code, RuleCategory::TestDiscipline, &config)
        .map(DoctorSeverity::from)
        .unwrap_or(default)
}

/// W3 — mirror of `resolve_test_discipline_severity` for the
/// internal-hygiene category. Under `tdd-iron-hand`, every `INTERNAL-*`
/// rule escalates to `Error`; otherwise the per-rule default carried
/// by the dispatcher stands.
pub fn resolve_internal_hygiene_severity(
    default: DoctorSeverity,
    code: &str,
    preset: Option<lazuli_doctor::internal_hygiene::preset::InternalHygienePreset>,
) -> DoctorSeverity {
    // W1 — see `resolve_test_discipline_severity`: category-preset
    // escalation only, shared via `lazuli_doctor_config`.
    let config = ResolvedDoctorConfig {
        internal_hygiene_preset: preset,
        ..ResolvedDoctorConfig::default()
    };
    category_preset_severity(code, RuleCategory::InternalHygiene, &config)
        .map(DoctorSeverity::from)
        .unwrap_or(default)
}

/// `.lzi` hygiene severity resolver — mirror of
/// `resolve_internal_hygiene_severity` for `LZI-*` rules. Under
/// `tdd-iron-hand`, every `LZI-*` rule escalates to `Error`; otherwise
/// the per-rule default stands.
pub(super) fn resolve_lzi_hygiene_severity(
    default: DoctorSeverity,
    code: &str,
    preset: Option<lazuli_doctor::lzi_hygiene::preset::LziHygienePreset>,
) -> DoctorSeverity {
    // W1 — see `resolve_test_discipline_severity`: category-preset
    // escalation only, shared via `lazuli_doctor_config`.
    let config = ResolvedDoctorConfig {
        lzi_hygiene_preset: preset,
        ..ResolvedDoctorConfig::default()
    };
    category_preset_severity(code, RuleCategory::LziHygiene, &config)
        .map(DoctorSeverity::from)
        .unwrap_or(default)
}

/// Iron-hand 4th-dimension severity resolver — mirror of
/// `resolve_internal_hygiene_severity` for error-handling rules
/// (`INTERNAL-PANIC-*`, `INTERNAL-ERROR-*`, `ERROR-*`, `HANDLER-*`).
/// Under `tdd-iron-hand`, every error-handling code escalates to
/// `Error`; otherwise the per-rule default stands.
pub fn resolve_error_handling_severity(
    default: DoctorSeverity,
    code: &str,
    preset: Option<lazuli_doctor::error_handling::preset::ErrorHandlingPreset>,
) -> DoctorSeverity {
    // W1 — see `resolve_test_discipline_severity`: category-preset
    // escalation only, shared via `lazuli_doctor_config`.
    let config = ResolvedDoctorConfig {
        error_handling_preset: preset,
        ..ResolvedDoctorConfig::default()
    };
    category_preset_severity(code, RuleCategory::ErrorHandling, &config)
        .map(DoctorSeverity::from)
        .unwrap_or(default)
}

/// Resolve the project-root directory for a `lazuli doctor <input>`
/// invocation. Directory inputs pass through unchanged; file inputs
/// resolve to the parent directory so `Lazurite.toml` / `app.lzi`
/// lookups still find the project. Inputs without a parent fall back
/// to `.` (single-file invocation from the repo root).
pub fn doctor_project_root(input: &Path) -> PathBuf {
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

/// Walk `source` and translate a byte offset into a `(line, column)`
/// pair, both 1-based. Used by every diagnostic builder that anchors
/// a finding to a span: the parser hands back byte offsets, and the
/// LSP / text report want line/column. Shared so the command-routing,
/// plugin-reference, and report-storage aggregators agree on the same
/// counting (lines split on `\n`, columns count chars).
pub(crate) fn line_col_for_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;

    for (index, ch) in source.char_indices() {
        if index >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

/// File-anchored variant of [`line_col_for_offset`]: reads the file at
/// `path` and resolves the offset against its on-disk contents. Falls
/// back to `(1, 1)` on a read failure so the caller still gets a valid
/// anchor (the diagnostic stays attached to the file, just at the top).
pub(crate) fn line_col_for_offset_in_file(path: &Path, offset: usize) -> (usize, usize) {
    let Ok(source) = std::fs::read_to_string(path) else {
        return (1, 1);
    };
    line_col_for_offset(&source, offset)
}
