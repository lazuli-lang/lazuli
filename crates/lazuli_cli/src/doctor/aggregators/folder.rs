//! Folder-layout aggregator.
//!
//! Walks the project's frontend folder vocabulary and surfaces five
//! closed-catalog rules:
//!
//! - `feature-orphan-component`   (singular/plural client topologies)
//! - `pages-bypass`               (canonical pages/ vs. routes/ folder)
//! - `type-duplicate`             (generated vs. hand-authored type drift)
//! - `cross-feature-import`       (feature isolation guard)
//! - `VOCAB-CLIENT-SRC-001`       (closed-catalog `src/` shape)
//!
//! All five rules are anti-pattern checks against the post-Wave-H 7+6
//! frontend canon. See `docs/decisions/client_src_canonical_architecture_2026-05-17.md`
//! and `docs/proposals/lazurite-frontend-folder-canon.md` §4 for the
//! authoritative catalog. The first four respect the active security
//! profile; the last (`VOCAB-CLIENT-SRC-001`) is pinned to Error because
//! the framework's MVVM shape rejects the bad vocabulary structurally —
//! a Warning would let AI-authors ignore it.

use std::path::Path;

use lazuli_lsp::SecurityProfile;

use crate::doctor::folder;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, doctor_rule_path, doctor_rule_severity};

/// Aggregate every folder-layout finding into the canonical
/// `DoctorDiagnostic` envelope. Returns an empty vec when the project
/// has no client topology yet.
pub(crate) fn diagnostics(
    project_root: &Path,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_rule_severity(security_profile);
    let mut diagnostics = Vec::new();

    diagnostics.extend(
        folder::feature_orphan::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.path),
                line: 1,
                column: 1,
                severity,
                code: folder::feature_orphan::Finding::CODE.to_owned(),
                message: finding.message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
    );
    diagnostics.extend(
        folder::pages_bypass::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.path),
                line: 1,
                column: 1,
                severity,
                code: folder::pages_bypass::Finding::CODE.to_owned(),
                message: finding.message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
    );
    diagnostics.extend(
        folder::type_duplicate::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.user_file),
                line: 1,
                column: 1,
                severity,
                code: folder::type_duplicate::Finding::CODE.to_owned(),
                message: finding.message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
    );
    diagnostics.extend(
        folder::cross_feature_import::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.source_file),
                line: 1,
                column: 1,
                severity,
                code: folder::cross_feature_import::Finding::CODE.to_owned(),
                message: finding.message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
    );
    // VOCAB-CLIENT-SRC-001: closed-catalog enforcement of the client
    // `src/` layout per
    // `docs/decisions/client_src_canonical_architecture_2026-05-17.md`.
    // Severity is hard-coded to Error regardless of security profile —
    // the anti-pattern is structural (folder vocabulary that the
    // framework's MVVM shape rejects), so a Warning would let AI-authors
    // ignore it.
    diagnostics.extend(
        folder::vocab_client_src_001::check(project_root)
            .into_iter()
            .map(|finding| DoctorDiagnostic {
                path: doctor_rule_path(project_root, finding.path),
                line: 1,
                column: 1,
                severity: DoctorSeverity::Error,
                code: folder::vocab_client_src_001::Finding::CODE.to_owned(),
                message: finding.message,
                category: None,
                feature_name: None,
                construct: None,
                fix: None,
                group: None,
            }),
    );

    diagnostics
}
