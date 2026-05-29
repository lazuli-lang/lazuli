//! `[doctor]` block schema — relocated here from
//! `lazuli_cli::lazurite_manifest::doctor` so the resolver crate (and,
//! later, the LSP) can deserialize the doctor config without depending
//! on the CLI. `lazuli_cli` re-exports these types so its existing call
//! sites (`Manifest.doctor: Option<Doctor>`,
//! `doctor.test_discipline.severity_override`, …) keep compiling
//! unchanged.
//!
//! Each sub-block stays optional so most pilots author only the
//! sections relevant to their CI posture. The `DOCTOR-OVERRIDE-NEEDS-
//! REASON-001` analyzer (in the CLI) enforces `reason = "..."` on every
//! entry — that meta-finding is orthogonal to severity resolution and is
//! NOT folded into this crate.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `[doctor]` block in `Lazurite.toml`.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Doctor {
    /// `[doctor] profile = "..."`. Parsed but, today, not used to drive
    /// the active profile (the CLI takes profile from
    /// `--security-profile`). Kept on the struct so deserialization is
    /// lossless and a later wave can wire it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_discipline: Option<TestDisciplineDoctor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageSection>,
    /// `[doctor.internal_hygiene]` block. Governs `INTERNAL-*` rules that
    /// audit the framework's own Rust source under `lazuli doctor
    /// --self`. Mirrors test_discipline shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_hygiene: Option<InternalHygieneDoctor>,
    /// `[doctor.error_handling]` block. Governs `INTERNAL-PANIC-*` /
    /// `INTERNAL-ERROR-*` / `ERROR-*` / `HANDLER-*` rules. Under `preset
    /// = "tdd-iron-hand"` every rule fires at `Error` regardless of
    /// profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_handling: Option<ErrorHandlingDoctor>,
    /// `[doctor.lzi_hygiene]` block — user-side `.lzi` source-shape
    /// hygiene. Governs `LZI-FILE-SIZE-001`,
    /// `LZI-FEATURE-NAMING-MATCHES-FILE-001`, `LZI-FEATURE-COHESION-001`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lzi_hygiene: Option<LziHygieneDoctor>,
}

/// `[doctor.lzi_hygiene]` block — user-side `.lzi` shape rules.
///
/// An optional preset name and per-rule severity overrides keyed by
/// canonical code.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct LziHygieneDoctor {
    /// Preset name. Parsed by
    /// `lazuli_doctor::lzi_hygiene::preset::LziHygienePreset::parse`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Per-rule severity overrides keyed by canonical code.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severity_override: BTreeMap<String, SeverityOverride>,
}

/// `[doctor.error_handling]` block — iron-hand 4th dimension.
///
/// Covers framework Rust source (`INTERNAL-PANIC-*`, `INTERNAL-ERROR-*`),
/// user `.lzi`/`.lzx` contract (`ERROR-*`), and user Go handlers
/// (`HANDLER-*`).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ErrorHandlingDoctor {
    /// Preset name. Parsed by
    /// `lazuli_doctor::error_handling::preset::ErrorHandlingPreset::parse`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Per-rule severity overrides keyed by canonical code.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severity_override: BTreeMap<String, SeverityOverride>,
}

/// `[doctor.internal_hygiene]` block.
///
/// Configures the `INTERNAL-*` rules that audit the framework's Rust
/// source. Per-rule overrides via `severity_override` must carry
/// `reason` per `DOCTOR-OVERRIDE-NEEDS-REASON-001`.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct InternalHygieneDoctor {
    /// Preset name. Parsed by
    /// `lazuli_doctor::internal_hygiene::preset::InternalHygienePreset::parse`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// Per-rule severity overrides keyed by canonical code.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severity_override: BTreeMap<String, SeverityOverride>,
}

/// `[doctor.test_discipline]` block.
///
/// The optional `preset` shortcut mirrors `[doctor.coverage].preset`: a
/// single line sets the severity posture for every TEST-* / DOCTOR-* /
/// MIGRATION-* / RUNTIME-* rule. Per-rule overrides in
/// `severity_override` still win — preset is the baseline.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TestDisciplineDoctor {
    /// Preset name. Parsed by
    /// `lazuli_doctor::test_discipline::preset::TestDisciplinePreset::parse`.
    /// `None` means "no preset; defer to profile-derived defaults +
    /// per-rule overrides only".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severity_override: BTreeMap<String, SeverityOverride>,
}

/// `[doctor.<category>].severity_override.<RULE-CODE>` entry.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SeverityOverride {
    /// Author-supplied severity (`warning`, `error`, `info`, `hint`).
    pub severity: String,
    /// Optional human justification. Required (non-blank) per
    /// `DOCTOR-OVERRIDE-NEEDS-REASON-001` (enforced in the CLI, not here).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `[doctor.coverage]` schema.
///
/// The optional `preset` shortcut lets pilots opt into the
/// `tdd-strict` / `tdd-mature` / `tdd-iron-hand` / `off` opinionated
/// layer-threshold sets without authoring all six `[doctor.coverage.<layer>]`
/// sub-blocks. Per-layer sub-blocks still override the preset.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CoverageSection {
    /// Coverage preset name. One of `tdd-strict`, `tdd-mature`,
    /// `tdd-iron-hand`, `off`. Unknown values surface as a doctor error
    /// (in the CLI) so unknown presets don't silently degrade.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(flatten)]
    pub per_layer: BTreeMap<String, LayerThresholdConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregate_method: Option<String>,
}

/// One `[doctor.coverage.<layer>]` block — the hard threshold and the
/// warn-band threshold for a single coverage layer.
#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct LayerThresholdConfig {
    /// Block the run (non-zero exit) when coverage drops below this
    /// percentage.
    pub block_under: u32,
    /// Surface a warning (but do not block) when coverage drops below
    /// this percentage.
    pub warn_under: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_default_is_empty() {
        let doctor = Doctor::default();
        assert!(doctor.profile.is_none());
        assert!(doctor.test_discipline.is_none());
        assert!(doctor.coverage.is_none());
        assert!(doctor.internal_hygiene.is_none());
        assert!(doctor.error_handling.is_none());
    }

    #[test]
    fn error_handling_block_deserializes_from_toml() {
        let toml_input = r#"
[error_handling]
preset = "tdd-iron-hand"

[error_handling.severity_override.INTERNAL-PANIC-UNWRAP-001]
severity = "warning"
reason = "transition period — escalate after pilot adoption"
"#;
        let doctor: Doctor = toml::from_str(toml_input).expect("deserialize");
        let eh = doctor.error_handling.expect("error_handling block");
        assert_eq!(eh.preset.as_deref(), Some("tdd-iron-hand"));
        let ov = eh
            .severity_override
            .get("INTERNAL-PANIC-UNWRAP-001")
            .expect("override");
        assert_eq!(ov.severity, "warning");
        assert!(ov.reason.is_some());
    }
}
