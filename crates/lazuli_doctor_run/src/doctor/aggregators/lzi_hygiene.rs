//! `[doctor.lzi_hygiene]` aggregator — runs `LZI-*` rules over user
//! `.lzi` source files.
//!
//! Walks `<project_root>` for `.lzi` files (via
//! [`lazuli_doctor::lzi_hygiene::walker::walk_lzi_sources`]), then runs
//! the three rules and converts each finding into a
//! [`DoctorDiagnostic`]. Severity is resolved through
//! [`super::super::helpers::resolve_lzi_hygiene_severity`] which honors
//! the `[doctor.lzi_hygiene].preset` from `Lazurite.toml`.

use std::path::Path;

use lazuli_doctor::DoctorSeverity as SharedSeverity;
use lazuli_doctor::lzi_hygiene::preset::LziHygienePreset;
use lazuli_doctor::lzi_hygiene::walker::walk_lzi_sources;
use lazuli_doctor::lzi_hygiene::{
    feature_cohesion_001, feature_naming_matches_file_001, file_size_001,
};

use crate::doctor::helpers::resolve_lzi_hygiene_severity;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity};

/// Run every `LZI-*` rule against `project_root` and return the
/// collected diagnostics. `preset` is the active `[doctor.lzi_hygiene]`
/// preset, resolved by the caller off the severity `ResolvedDoctorConfig`.
///
/// v2 — the preset arrives pre-resolved from the caller's severity config
/// (CLI: disk; LSP: unsaved `Lazurite.toml` buffer) instead of being
/// re-read off an on-disk manifest here, so in-editor severity tracks
/// unsaved `[doctor.lzi_hygiene] preset` edits.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// let diagnostics = lzi_hygiene_diagnostics(
///     Path::new("/path/to/user-app"),
///     None,
/// );
/// for d in &diagnostics {
///     println!("{}", d.code);
/// }
/// ```
pub(crate) fn lzi_hygiene_diagnostics(
    project_root: &Path,
    preset: Option<LziHygienePreset>,
) -> Vec<DoctorDiagnostic> {
    let files = walk_lzi_sources(project_root);
    if files.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();

    // LZI-FILE-SIZE-001 — default Info; preset escalates.
    for finding in file_size_001::check(&files) {
        let severity = resolve_lzi_hygiene_severity(
            DoctorSeverity::Info,
            file_size_001::Finding::CODE,
            preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity,
            code: file_size_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // LZI-FEATURE-NAMING-MATCHES-FILE-001 — default Warning; preset
    // escalates to Error under iron-hand.
    for finding in feature_naming_matches_file_001::check(&files) {
        let severity = resolve_lzi_hygiene_severity(
            DoctorSeverity::Warning,
            feature_naming_matches_file_001::Finding::CODE,
            preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity,
            code: feature_naming_matches_file_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // LZI-FEATURE-COHESION-001 — default Warning; preset escalates to
    // Error under iron-hand.
    for finding in feature_cohesion_001::check(&files) {
        let severity = resolve_lzi_hygiene_severity(
            DoctorSeverity::Warning,
            feature_cohesion_001::Finding::CODE,
            preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity,
            code: feature_cohesion_001::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // Sanity-check: bridge enum maps cleanly. This is a compile-time
    // assertion via use; runtime work happened above.
    let _: SharedSeverity = SharedSeverity::Warning;

    diagnostics
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::*;

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::File::create(&p)
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
    }

    #[test]
    fn aggregator_returns_empty_when_root_has_no_lzi() {
        let tmp = tempfile::tempdir().unwrap();
        let diagnostics = lzi_hygiene_diagnostics(tmp.path(), None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn aggregator_fires_file_size_on_large_file() {
        let tmp = tempfile::tempdir().unwrap();
        let body: String = "feature billing\n".repeat(700);
        write(tmp.path(), "features/billing/billing.lzi", &body);
        let diagnostics = lzi_hygiene_diagnostics(tmp.path(), None);
        assert!(
            diagnostics.iter().any(|d| d.code == "LZI-FILE-SIZE-001"),
            "expected LZI-FILE-SIZE-001, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn aggregator_fires_cohesion_on_arbitrary_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/mixed/mixed.lzi",
            "feature billing\nfeature invoice\nfeature subscription\n",
        );
        let diagnostics = lzi_hygiene_diagnostics(tmp.path(), None);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "LZI-FEATURE-COHESION-001")
        );
    }

    #[test]
    fn aggregator_silent_on_cohesive_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/customer/customer.lzi",
            "feature customer\nfeature customer_auth\nfeature customer_tags\n",
        );
        let diagnostics = lzi_hygiene_diagnostics(tmp.path(), None);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn aggregator_fires_naming_when_stem_mismatches() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/billing/payments.lzi",
            "feature subscription\n",
        );
        let diagnostics = lzi_hygiene_diagnostics(tmp.path(), None);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "LZI-FEATURE-NAMING-MATCHES-FILE-001")
        );
    }
}
