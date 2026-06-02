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
use lazuli_doctor::lzi_hygiene::comment_prose_001;
use lazuli_doctor::lzi_hygiene::feature_cohesion_002::LoweredFeature;
use lazuli_doctor::lzi_hygiene::preset::LziHygienePreset;
use lazuli_doctor::lzi_hygiene::walker::{is_exempt_path, walk_lzi_sources};
use lazuli_doctor::lzi_hygiene::{
    feature_cohesion_001, feature_cohesion_002, feature_naming_matches_file_001, file_size_001,
};
use lazuli_ir::Feature;
use lazuli_syntax::parse_feature_skeletons;

use crate::doctor::helpers::resolve_lzi_hygiene_severity;
use crate::doctor::{DoctorDiagnostic, DoctorFile, DoctorSeverity};

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

/// spec 0029 — `LZI-COMMENT-PROSE-001`: flag EVERY `#` comment line in every
/// `.lzi`/`.lzx` file (full-line AND inline), driving the design surface to zero
/// prose comments. WARNING by default; ERROR under the iron-hand preset (the
/// `LZI-*` prefix escalates via `resolve_lzi_hygiene_severity`). LziHygiene never
/// gates, so these never refuse-emit.
///
/// Runs over the doctor's parsed `DoctorFile` set (which carries BOTH `.lzi` and
/// `.lzx`), not the `.lzi`-only `walk_lzi_sources`, so the rule covers both
/// design surfaces per spec. Honors the same path exemptions the `.lzi` walker
/// uses (toplevel/contract/fixture files) so it stays quiet on non-feature
/// source.
pub(crate) fn comment_prose_diagnostics(
    project_root: &Path,
    files: &[DoctorFile],
    preset: Option<LziHygienePreset>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();
    for file in files {
        let ext = file.path.extension().and_then(|s| s.to_str());
        if !matches!(ext, Some("lzi") | Some("lzx")) {
            continue;
        }
        // Mirror the `.lzi` walker's exemptions (toplevel app/registry files,
        // `contracts/`, fixture sub-trees) so PROSE-001 stays quiet on
        // non-feature source. Compute the path relative to the project root the
        // same way the walker does.
        let relative = file.path.strip_prefix(project_root).unwrap_or(&file.path);
        let name = file
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if is_exempt_path(relative, name) {
            continue;
        }

        for finding in comment_prose_001::scan_lzi_comment_prose(&file.source) {
            let severity = resolve_lzi_hygiene_severity(
                DoctorSeverity::Warning,
                comment_prose_001::CODE,
                preset,
            );
            diagnostics.push(DoctorDiagnostic {
                path: file.path.clone(),
                line: finding.line,
                column: finding.column,
                severity,
                code: comment_prose_001::CODE.to_owned(),
                message: finding.message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            });
        }
    }
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

    // ── spec 0029 — LZI-COMMENT-PROSE-001 dispatch ──────────────────────────

    fn doctor_file(path: &Path, source: &str) -> DoctorFile {
        DoctorFile {
            path: path.to_path_buf(),
            source: source.to_string(),
            local_diagnostics: Vec::new(),
            lzx: None,
        }
    }

    #[test]
    fn comment_prose_fires_on_lzi_and_lzx() {
        let root = Path::new("/proj");
        let files = vec![
            doctor_file(
                Path::new("/proj/features/billing/billing.lzi"),
                "# rationale that belongs in ctx.md\nfeature billing\n",
            ),
            doctor_file(
                Path::new("/proj/features/billing/billing.lzx"),
                "# ── Public ──\nexperience billing\n",
            ),
        ];
        let diags = comment_prose_diagnostics(root, &files, None);
        assert_eq!(diags.len(), 2, "both .lzi and .lzx fire: {diags:?}");
        assert!(diags.iter().all(|d| d.code == "LZI-COMMENT-PROSE-001"));
        assert!(diags.iter().all(|d| d.severity == DoctorSeverity::Warning));
    }

    #[test]
    fn comment_prose_clean_file_reports_zero() {
        let root = Path::new("/proj");
        let files = vec![doctor_file(
            Path::new("/proj/features/billing/billing.lzi"),
            "@doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")\nfeature billing\n  purpose \"x\"\n",
        )];
        assert!(comment_prose_diagnostics(root, &files, None).is_empty());
    }

    #[test]
    fn comment_prose_respects_path_exemptions() {
        let root = Path::new("/proj");
        // app.lzi (toplevel) + a fixtures/ subtree are exempt — no findings even
        // with a `#` comment present.
        let files = vec![
            doctor_file(Path::new("/proj/app.lzi"), "# header\napp Acme\n"),
            doctor_file(
                Path::new("/proj/tests/fixtures/x.lzi"),
                "# header\nfeature x\n",
            ),
        ];
        assert!(comment_prose_diagnostics(root, &files, None).is_empty());
    }

    #[test]
    fn comment_prose_iron_hand_is_error() {
        let root = Path::new("/proj");
        let files = vec![doctor_file(
            Path::new("/proj/features/billing/billing.lzi"),
            "# prose\nfeature billing\n",
        )];
        let diags = comment_prose_diagnostics(
            root,
            &files,
            Some(lazuli_doctor::lzi_hygiene::preset::LziHygienePreset::TddIronHand),
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, DoctorSeverity::Error);
    }
}
