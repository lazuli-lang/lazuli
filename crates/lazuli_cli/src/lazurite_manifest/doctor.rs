//! `[doctor]` block schema — Wave 0.5 (severity overrides), Wave 1.5
//! (test_discipline preset), Wave 3 (internal_hygiene), Wave 6
//! (coverage thresholds + Frente 1 preset shortcut).
//!
//! Each sub-block stays optional so most pilots author only the
//! sections relevant to their CI posture. The `DOCTOR-OVERRIDE-NEEDS-
//! REASON-001` analyzer enforces `reason = "..."` on every entry.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Wave 0.5 + Wave 6 + Wave 3 (rails-style) — `[doctor]` block in `Lazurite.toml`.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Doctor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_discipline: Option<TestDisciplineDoctor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageSection>,
    /// W3 (rails-style-refactor) — `[doctor.internal_hygiene]` block.
    /// Governs `INTERNAL-*` rules that audit the framework's own Rust
    /// source under `lazuli doctor --self`. Mirrors test_discipline shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_hygiene: Option<InternalHygieneDoctor>,
    /// Iron-hand 4th dimension — `[doctor.error_handling]` block.
    /// Governs `INTERNAL-PANIC-*` / `INTERNAL-ERROR-*` / `ERROR-*` /
    /// `HANDLER-*` rules. Under `preset = "tdd-iron-hand"` every rule
    /// fires at `Error` regardless of profile — editorial veto for the
    /// framework's own CI plus user-app error contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_handling: Option<ErrorHandlingDoctor>,
    /// `[doctor.lzi_hygiene]` block — user-side `.lzi` source-shape
    /// hygiene. Governs `LZI-FILE-SIZE-001`,
    /// `LZI-FEATURE-NAMING-MATCHES-FILE-001`,
    /// `LZI-FEATURE-COHESION-001`. Mirrors `[doctor.internal_hygiene]`
    /// shape; under `preset = "tdd-iron-hand"` every `LZI-*` rule
    /// fires at `Error`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lzi_hygiene: Option<LziHygieneDoctor>,
}

/// `[doctor.lzi_hygiene]` block — user-side `.lzi` shape rules.
///
/// Mirrors [`InternalHygieneDoctor`] / [`ErrorHandlingDoctor`]: an
/// optional preset name and per-rule severity overrides keyed by
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
/// (`HANDLER-*`). Mirrors the shape of [`InternalHygieneDoctor`] and
/// [`TestDisciplineDoctor`].
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

/// W3 — `[doctor.internal_hygiene]` block.
///
/// Configures the four `INTERNAL-*` rules that audit the framework's
/// Rust source. Under `preset = "tdd-iron-hand"`, every rule fires at
/// `Error` regardless of profile — editorial veto for the framework's
/// own CI. Per-rule overrides via `severity_override` must carry
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

/// Wave 0.5 + Wave 1.5 — `[doctor.test_discipline]` block.
///
/// Wave 1.5 (rails-style-refactor) adds the optional `preset` shortcut.
/// Mirrors `[doctor.coverage].preset` mechanism: a single line sets the
/// severity posture for every TEST-* / DOCTOR-* / MIGRATION-* / RUNTIME-*
/// rule. Values: `tdd-iron-hand` (all error), `tdd-strict` (all warning),
/// `tdd-mature` (per-rule defaults), `off` (all info). Per-rule overrides
/// in `severity_override` still win — preset is the baseline.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct TestDisciplineDoctor {
    /// Wave 1.5 — preset name. Parsed by
    /// `lazuli_doctor::test_discipline::preset::TestDisciplinePreset::parse`.
    /// `None` means "no preset; defer to profile-derived defaults +
    /// per-rule overrides only".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub severity_override: BTreeMap<String, SeverityOverride>,
}

/// Wave 0.5 — `[doctor.<category>].severity_override.<RULE-CODE>` entry.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SeverityOverride {
    pub severity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Wave 6 — `[doctor.coverage]` schema.
///
/// Frente 1 (2026-05-24) adds the optional `preset` shortcut so
/// pilots can opt into the `tdd-strict` / `tdd-mature` / `off`
/// opinionated layer-threshold sets without authoring all six
/// `[doctor.coverage.<layer>]` sub-blocks. Per-layer sub-blocks
/// still override the preset; see
/// `docs/canonical-semantics.md#coverage-presets`.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CoverageSection {
    /// Coverage preset name. One of `tdd-strict`, `tdd-mature`,
    /// `off`. Unknown values surface as a doctor error so unknown
    /// presets don't silently degrade into vacuous-pass behavior.
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
