/// Parse a TOML override / preset-map severity string into a
/// [`DoctorSeverity`]. Mirrors the CLI's `parse_doctor_severity`:
/// `"error" | "warning" | "warn" | "info" | "hint"`, case-insensitive;
/// `None` for anything else so callers fall back to the next precedence
/// level.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor_config::{parse_severity, DoctorSeverity};
///
/// assert_eq!(parse_severity("Error"), Some(DoctorSeverity::Error));
/// assert_eq!(parse_severity("warn"), Some(DoctorSeverity::Warning));
/// assert_eq!(parse_severity("bogus"), None);
/// ```
pub fn parse_severity(s: &str) -> Option<DoctorSeverity> {
    match s.to_ascii_lowercase().as_str() {
        "error" => Some(DoctorSeverity::Error),
        "warning" | "warn" => Some(DoctorSeverity::Warning),
        "info" => Some(DoctorSeverity::Info),
        "hint" => Some(DoctorSeverity::Hint),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_strict() -> ResolvedDoctorConfig {
        ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap()
    }

    #[test]
    fn profile_round_trips() {
        for p in [
            DoctorProfile::Prototype,
            DoctorProfile::Strict,
            DoctorProfile::Production,
        ] {
            assert_eq!(DoctorProfile::parse(p.as_str()), Some(p));
        }
    }

    #[test]
    fn category_default_matches_doctor_severity_for() {
        // Vocabulary: strict -> warning, production -> error.
        let strict = empty_strict();
        assert_eq!(
            effective_severity(
                "VOCAB-X-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &strict
            ),
            Some(DoctorSeverity::Warning),
        );
        let prod = ResolvedDoctorConfig::resolve(None, DoctorProfile::Production).unwrap();
        assert_eq!(
            effective_severity(
                "VOCAB-X-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &prod
            ),
            Some(DoctorSeverity::Error),
        );
        // TestDiscipline carries its own per-profile posture.
        let proto = ResolvedDoctorConfig::resolve(None, DoctorProfile::Prototype).unwrap();
        assert_eq!(
            effective_severity(
                "TEST-X-001",
                DoctorSeverity::Warning,
                RuleCategory::TestDiscipline,
                &proto
            ),
            Some(DoctorSeverity::Info),
        );
    }

    #[test]
    fn manifest_override_wins() {
        let toml = r#"
[doctor.test_discipline.severity_override."VOCAB-CONTEXT-CTXMD-001"]
severity = "warning"
reason = "backfill scheduled"

[doctor.coverage]
preset = "tdd-iron-hand"
"#;
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        // Coverage iron-hand would escalate to error, but the manifest
        // override downgrades it back to warning.
        assert_eq!(
            effective_severity(
                "VOCAB-CONTEXT-CTXMD-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &cfg
            ),
            Some(DoctorSeverity::Warning),
        );
    }

    #[test]
    fn coverage_preset_escalates_vocab_context() {
        let toml = "[doctor.coverage]\npreset = \"tdd-iron-hand\"\n";
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        for code in [
            "VOCAB-CONTEXT-PURPOSE-001",
            "VOCAB-CONTEXT-NONGOALS-001",
            "VOCAB-CONTEXT-CTXMD-001",
        ] {
            assert_eq!(
                effective_severity(
                    code,
                    DoctorSeverity::Warning,
                    RuleCategory::Vocabulary,
                    &cfg
                ),
                Some(DoctorSeverity::Error),
                "{code} should escalate under iron-hand coverage preset",
            );
        }
    }

    #[test]
    fn off_coverage_preset_suppresses_vocab_context() {
        let toml = "[doctor.coverage]\npreset = \"off\"\n";
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        assert_eq!(
            effective_severity(
                "VOCAB-CONTEXT-PURPOSE-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &cfg
            ),
            None,
        );
        // Non-governed codes still resolve normally under `off`.
        assert_eq!(
            effective_severity(
                "VOCAB-TESTS-MISSING-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &cfg
            ),
            Some(DoctorSeverity::Warning),
        );
    }

    #[test]
    fn category_preset_escalation() {
        // test_discipline iron-hand -> Error.
        let toml = "[doctor.test_discipline]\npreset = \"tdd-iron-hand\"\n";
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        assert_eq!(
            effective_severity(
                "TEST-MISSING-AUTHORED-001",
                DoctorSeverity::Warning,
                RuleCategory::TestDiscipline,
                &cfg
            ),
            Some(DoctorSeverity::Error),
        );
        // error_handling off -> Info.
        let toml = "[doctor.error_handling]\npreset = \"off\"\n";
        let cfg = ResolvedDoctorConfig::resolve(Some(toml), DoctorProfile::Strict).unwrap();
        assert_eq!(
            effective_severity(
                "HANDLER-NO-PANIC-001",
                DoctorSeverity::Error,
                RuleCategory::ErrorHandling,
                &cfg
            ),
            Some(DoctorSeverity::Info),
        );
    }

    /// The one-knob contract: `[doctor] profile = "iron-hand"` with NO
    /// family blocks must resolve the FULL iron-hand stance — every
    /// discipline family defaulted to its `tdd-iron-hand` preset, plus the
    /// production-level severity escalation (VOCAB-CONTEXT-* → error, every
    /// test-discipline rule → error). This is the exact posture the
    /// six-block form (`profile = "production"` + five
    /// `[doctor.<family>] preset = "tdd-iron-hand"` blocks) produced.
    #[test]
    fn iron_hand_profile_meta_bundle_fans_out_all_families() {
        // No manifest at all — the profile alone must do everything.
        let cfg = ResolvedDoctorConfig::from_doctor(None, DoctorProfile::IronHand);

        // (1) all five families default to their tdd-iron-hand preset, exactly
        // as if each had authored `[doctor.<family>] preset = "tdd-iron-hand"`.
        assert_eq!(cfg.coverage_preset, CoveragePreset::parse("tdd-iron-hand"));
        assert_eq!(
            cfg.test_discipline_preset,
            TestDisciplinePreset::parse("tdd-iron-hand")
        );
        assert_eq!(
            cfg.internal_hygiene_preset,
            InternalHygienePreset::parse("tdd-iron-hand")
        );
        assert_eq!(
            cfg.error_handling_preset,
            ErrorHandlingPreset::parse("tdd-iron-hand")
        );
        assert_eq!(
            cfg.lzi_hygiene_preset,
            LziHygienePreset::parse("tdd-iron-hand")
        );
        assert!(cfg.coverage_preset.is_some(), "iron-hand must set a preset");

        // (2a) production-level severity escalation: VOCAB-CONTEXT-* → error
        // (driven by the defaulted tdd-iron-hand coverage preset).
        for code in [
            "VOCAB-CONTEXT-PURPOSE-001",
            "VOCAB-CONTEXT-NONGOALS-001",
            "VOCAB-CONTEXT-CTXMD-001",
        ] {
            assert_eq!(
                effective_severity(
                    code,
                    DoctorSeverity::Warning,
                    RuleCategory::Vocabulary,
                    &cfg
                ),
                Some(DoctorSeverity::Error),
                "{code} must escalate to error under iron-hand profile",
            );
        }

        // (2b) test-discipline rules fire at Error (the iron-hand preset +
        // production-level category default both agree on Error).
        assert_eq!(
            effective_severity(
                "TEST-MISSING-AUTHORED-001",
                DoctorSeverity::Warning,
                RuleCategory::TestDiscipline,
                &cfg
            ),
            Some(DoctorSeverity::Error),
        );
    }

    /// Iron-hand resolved via the toml-honoring entry point
    /// (`resolve_reading_profile`, the LSP / no-flag path) matches the
    /// explicit-profile path: one knob, both entrypoints, same stance.
    #[test]
    fn iron_hand_via_reading_profile_matches_explicit() {
        let toml = "[doctor]\nprofile = \"iron-hand\"\n";
        let cfg = ResolvedDoctorConfig::resolve_reading_profile(Some(toml)).unwrap();
        assert_eq!(cfg.profile.0, DoctorProfile::IronHand);
        assert_eq!(cfg.coverage_preset, CoveragePreset::parse("tdd-iron-hand"));
        assert_eq!(
            effective_severity(
                "VOCAB-CONTEXT-PURPOSE-001",
                DoctorSeverity::Warning,
                RuleCategory::Vocabulary,
                &cfg
            ),
            Some(DoctorSeverity::Error),
        );
    }

    /// iron-hand `parse`/`as_str` round-trip.
    #[test]
    fn iron_hand_profile_round_trips() {
        assert_eq!(
            DoctorProfile::parse("iron-hand"),
            Some(DoctorProfile::IronHand)
        );
        assert_eq!(DoctorProfile::IronHand.as_str(), "iron-hand");
    }
}
