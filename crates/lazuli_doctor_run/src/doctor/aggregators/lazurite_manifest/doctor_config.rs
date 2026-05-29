//! Doctor configuration hygiene checks for `Lazurite.toml`.
//!
//! - `DOCTOR-OVERRIDE-NEEDS-REASON-001` — every
//!   `[doctor.<cat>].severity_override.<rule>` must carry a non-blank
//!   `reason` so the override is auditable in code review.
//! - `COVERAGE-PRESET-UNKNOWN-001` — a `[doctor.coverage] preset =
//!   "<x>"` value that `CoveragePreset::parse` doesn't recognize
//!   would silently fall back to defaults; surfacing the typo as an
//!   error keeps coverage gates honest.
//! - `CONFIG-NOISE-001` — when a config file's comment ratio exceeds
//!   1:1, the framework is leaking intent into the user's
//!   configuration. The fix path: push the explanation into
//!   canonical docs.

use lazuli_manifest::lazurite_manifest::Manifest;

use crate::doctor::{
    DoctorDiagnostic, DoctorPackage, DoctorSeverity, RuleCategory, doctor_severity_for,
};

/// Wave 0.5 — `DOCTOR-OVERRIDE-NEEDS-REASON-001` dispatcher.
///
/// Lifts the `[doctor.test_discipline].severity_override` table from
/// the parsed manifest into the rule's portable `OverrideEntry` shape,
/// invokes the rule, and maps findings to `DoctorDiagnostic`. Anchors
/// the diagnostic at `Lazurite.toml` line 1 (the rule's structural
/// payload doesn't yet carry exact TOML line spans; that refinement
/// lands post-Wave-0.5 once the toml crate exposes spans cleanly).
pub(super) fn check_doctor_override_needs_reason(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::test_discipline::override_needs_reason_001::{self, OverrideEntry};
    use lazuli_manifest::lazurite_manifest as lzr;

    let Some(doctor) = manifest.doctor.as_ref() else {
        return Vec::new();
    };
    let mut entries: Vec<OverrideEntry> = Vec::new();
    if let Some(td) = doctor.test_discipline.as_ref() {
        for (code, ov) in td.severity_override.iter() {
            let _: &lzr::SeverityOverride = ov; // keep the type pinned
            entries.push(OverrideEntry {
                category: RuleCategory::TestDiscipline.as_str().to_owned(),
                rule_code: code.clone(),
                severity: ov.severity.clone(),
                reason: ov.reason.clone(),
            });
        }
    }

    let manifest_path = package.project_root.join(lzr::MANIFEST_FILENAME);
    let findings = override_needs_reason_001::check(&entries, &manifest_path);
    findings
        .into_iter()
        .map(|finding| {
            let message = finding.message();
            let severity = doctor_severity_for(
                override_needs_reason_001::Finding::CODE,
                RuleCategory::TestDiscipline,
                package.security_profile,
                &std::collections::BTreeMap::new(),
            );
            DoctorDiagnostic {
                path: finding.path,
                line: 1,
                column: 1,
                severity,
                code: override_needs_reason_001::Finding::CODE.to_owned(),
                message,
                category: Some(RuleCategory::TestDiscipline),
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }
        })
        .collect()
}

/// Frente 1 — `COVERAGE-PRESET-UNKNOWN-001`. Fires when
/// `[doctor.coverage] preset = "<name>"` names a preset that
/// `CoveragePreset::parse` does not recognize. Listing the recognized
/// preset names in the message keeps the diagnostic self-explanatory
/// to an LLM authoring `Lazurite.toml` cold.
pub(super) fn check_coverage_preset_unknown(
    manifest: &Manifest,
    package: &DoctorPackage,
) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::coverage::CoveragePreset;

    let Some(preset_name) = manifest
        .doctor
        .as_ref()
        .and_then(|d| d.coverage.as_ref())
        .and_then(|c| c.preset.as_deref())
    else {
        return Vec::new();
    };
    if CoveragePreset::parse(preset_name).is_some() {
        return Vec::new();
    }
    vec![DoctorDiagnostic {
        path: package
            .project_root
            .join(lazuli_manifest::lazurite_manifest::MANIFEST_FILENAME),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "COVERAGE-PRESET-UNKNOWN-001".to_owned(),
        message: format!(
            "[doctor.coverage] preset = \"{preset_name}\" is not a recognized preset. \
             Allowed values: tdd-strict, tdd-mature, off. \
             Omit the field to fall back to the security-profile defaults."
        ),
        category: Some(RuleCategory::Vocabulary),
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }]
}

/// Frente 1 — `CONFIG-NOISE-001`. Warns when the comment ratio of a
/// top-level config file exceeds 1:1 (more comment lines than semantic
/// lines). The signal: when the user's config file is mostly inline
/// commentary, the framework is hiding intent behind explanation. The
/// fix: push the explanation into framework defaults / canonical docs.
///
/// Severity is Warning by design — the rule is informational and
/// never gates. Scope: `Lazurite.toml` (and the legacy lowercase
/// `lazurite.toml`). `.lzi` / `.lzx` follow in a future cycle once
/// the comment-vs-statement counter understands Lazuli syntax.
///
/// Heuristic logic + 6 unit tests live in
/// `lazuli_doctor::config_noise`; this function only stitches the
/// metrics to a `DoctorDiagnostic`.
pub(super) fn check_config_noise(package: &DoctorPackage) -> Vec<DoctorDiagnostic> {
    use lazuli_doctor::config_noise::config_noise_metrics;
    let mut diagnostics = Vec::new();
    // Prefer the canonical capitalized name; fall back to legacy only
    // if canonical is absent (mirrors `lazurite_manifest::load`). On
    // case-insensitive filesystems both `exists()` calls would
    // otherwise report the same file twice and double-fire.
    let canonical = package
        .project_root
        .join(lazuli_manifest::lazurite_manifest::MANIFEST_FILENAME);
    let legacy = package
        .project_root
        .join(lazuli_manifest::lazurite_manifest::LEGACY_MANIFEST_FILENAME);
    let (path, filename) = if canonical.exists() {
        (
            canonical,
            lazuli_manifest::lazurite_manifest::MANIFEST_FILENAME,
        )
    } else if legacy.exists() {
        (
            legacy,
            lazuli_manifest::lazurite_manifest::LEGACY_MANIFEST_FILENAME,
        )
    } else {
        return diagnostics;
    };
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return diagnostics;
    };
    let metrics = config_noise_metrics(&contents);
    if metrics.fires() {
        diagnostics.push(DoctorDiagnostic {
            path,
            line: 1,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "CONFIG-NOISE-001".to_owned(),
            message: format!(
                "{filename} has {} comment line(s) vs {} semantic line(s) (ratio {:.2}:1). \
                 When commentary exceeds config, the framework is leaking \
                 intent into the user's file — push the knowledge into framework \
                 defaults or canonical docs. See docs/canonical-semantics.md#config-hygiene.",
                metrics.comment_lines,
                metrics.semantic_lines,
                metrics.ratio()
            ),
            category: Some(RuleCategory::Vocabulary),
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    diagnostics
}
