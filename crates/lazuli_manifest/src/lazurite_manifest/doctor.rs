//! `[doctor]` block schema.
//!
//! W1 (lsp-doctor single-source) — these config structs were relocated
//! to `lazuli_doctor_config` so the shared doctor severity resolver (and,
//! later, the LSP) can deserialize them without depending on the CLI.
//! This module re-exports them verbatim so every existing CLI call site
//! (`Manifest.doctor: Option<Doctor>`,
//! `doctor.test_discipline.severity_override`, …) keeps compiling
//! unchanged.
//!
//! Each sub-block stays optional so most pilots author only the
//! sections relevant to their CI posture. The `DOCTOR-OVERRIDE-NEEDS-
//! REASON-001` analyzer enforces `reason = "..."` on every entry.

pub use lazuli_doctor_config::{
    CoverageSection, Doctor, InternalHygieneDoctor, LayerThresholdConfig, SeverityOverride,
    TestDisciplineDoctor,
};

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
