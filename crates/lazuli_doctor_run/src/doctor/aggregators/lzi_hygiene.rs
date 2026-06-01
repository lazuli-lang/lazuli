//! `[doctor.lzi_hygiene]` aggregator — runs `LZI-*` rules over user
//! `.lzi` source files.
//!
//! Walks `<project_root>` for `.lzi` files (via
//! [`lazuli_doctor::lzi_hygiene::walker::walk_lzi_sources`]), then runs
//! the three rules and converts each finding into a
//! [`DoctorDiagnostic`]. Severity is resolved through
//! [`super::super::helpers::resolve_lzi_hygiene_severity`] which honors
//! the `[doctor.lzi_hygiene].preset` from `Lazurite.toml`.

use std::path::{Path, PathBuf};

use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor::DoctorSeverity as SharedSeverity;
use lazuli_doctor::lzi_hygiene::feature_cohesion_002::LoweredFeature;
use lazuli_doctor::lzi_hygiene::preset::LziHygienePreset;
use lazuli_doctor::lzi_hygiene::walker::walk_lzi_sources;
use lazuli_doctor::lzi_hygiene::{
    feature_cohesion_001, feature_cohesion_002, feature_naming_matches_file_001, file_size_001,
};
use lazuli_ir::Feature;
use lazuli_syntax::parse_feature_skeletons;

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

    // spec 0008 — `LZI-FILE-SIZE-001` (re-keyed) and `LZI-FEATURE-COHESION-002`
    // read the lowered IR. Lower each `.lzi`'s first feature once here; the
    // `lazuli_doctor` crate has no runtime `lazuli_analyzer` dependency, so
    // lowering lives in this aggregator. Files that don't parse / lower are
    // skipped (other rules report those).
    let lowered: Vec<(PathBuf, String, Feature)> = files
        .iter()
        .filter_map(|f| {
            let skeletons = parse_feature_skeletons(&f.source).ok()?;
            let first = skeletons.first()?;
            let feature = lower_feature_skeleton(first).ok()?;
            Some((f.relative_path.clone(), f.source.clone(), feature))
        })
        .collect();
    let lowered_features: Vec<LoweredFeature<'_>> = lowered
        .iter()
        .map(|(path, source, feature)| LoweredFeature::new(path.clone(), feature, source.as_str()))
        .collect();

    let mut diagnostics = Vec::new();

    // LZI-FILE-SIZE-001 — spec 0008 re-key: default Warning (demoted
    // from preset-escalated), triggered off distinct (resource × effect)
    // pairs, not LOC; preset still escalates under iron-hand.
    for finding in file_size_001::check(&lowered_features) {
        let severity = resolve_lzi_hygiene_severity(
            DoctorSeverity::Warning,
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

    // LZI-FEATURE-COHESION-002 — spec 0008. Default Warning; preset
    // escalates to Error under iron-hand. Non-waivable-in-spirit: the
    // `# doctor:allow` opt-out is honored mechanically inside `check`,
    // but the message tells the author the only honest fix is a split.
    for finding in feature_cohesion_002::check(&lowered_features) {
        let severity = resolve_lzi_hygiene_severity(
            DoctorSeverity::Warning,
            feature_cohesion_002::Finding::CODE,
            preset,
        );
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity,
            code: feature_cohesion_002::Finding::CODE.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // LZI-FEATURE-COHESION-002 info companions — `uses` fan-out and
    // cross-feature name similarity. Always Info (never escalated): they
    // are softer heuristics, not warnings.
    for finding in feature_cohesion_002::check_info(&lowered_features) {
        let message = finding.message();
        diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: DoctorSeverity::Info,
            code: feature_cohesion_002::InfoFinding::CODE.to_owned(),
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
    fn aggregator_fires_file_size_on_high_surface_feature() {
        // spec 0008 re-key: LZI-FILE-SIZE-001 now triggers off distinct
        // (resource × effect) pairs, not LOC. Build a feature with many
        // resources × effects so it crosses the threshold.
        let tmp = tempfile::tempdir().unwrap();
        // Declarations grouped (resources, then commands, then queries)
        // per the `.lzi` grammar.
        let mut body = String::from("feature billing\n");
        for i in 0..8 {
            body.push_str(&format!("  resource Res{i}\n    label: Text required\n"));
        }
        for i in 0..8 {
            body.push_str(&format!("  command create_res{i}\n    creates Res{i}\n"));
        }
        for i in 0..8 {
            body.push_str(&format!("  query.list list_res{i}\n"));
        }
        for i in 0..8 {
            body.push_str(&format!("  query.lookup get_res{i} by id: ID\n"));
        }
        write(tmp.path(), "features/billing/billing.lzi", &body);
        let diagnostics = lzi_hygiene_diagnostics(tmp.path(), None);
        assert!(
            diagnostics.iter().any(|d| d.code == "LZI-FILE-SIZE-001"),
            "expected LZI-FILE-SIZE-001, got {:?}",
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    #[test]
    fn aggregator_fires_feature_cohesion_002_on_disconnected() {
        // Three resources with no FK/has_many/on_delete edge → ≥2
        // components → LZI-FEATURE-COHESION-002 fires.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "features/platform/platform.lzi",
            "feature platform\n  resource LegalDoc\n    body: Text required\n  \
             resource PlatformConfig\n    key: Text required\n  \
             resource DataRequest\n    email: Text required\n",
        );
        let diagnostics = lzi_hygiene_diagnostics(tmp.path(), None);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "LZI-FEATURE-COHESION-002"),
            "got {:?}",
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
