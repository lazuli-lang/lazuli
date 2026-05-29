//! In-editor doctor engine run (Layer 2 / full parity).
//!
//! Wave D3 — the LSP now runs the *same* package-orchestration engine the
//! CLI runs (`lazuli_doctor_run::run_package`) so cross-feature and
//! project-level findings (e.g. `REF-CROSS-FEATURE-UNKNOWN-001`, coverage
//! findings, RBAC catalog checks) surface as live editor squiggles —
//! not just on `lazuli doctor` CLI runs.
//!
//! ## Two-layer model
//!
//! The backend keeps the **synchronous** file-local pass
//! (`diagnostics_for_uri_with_config`) for typing responsiveness — those
//! are the [`is_lsp_owned`] codes (shape / contract / kebab-security).
//! Separately, a **debounced background task** runs the package engine
//! against the workspace root and publishes the [`DOCTOR_OWNED`] findings
//! (everything `run_package` emits that the LSP file-local pass does not),
//! remapped onto each open document's `Url` + a feature-header range.
//!
//! ## Partition (total + disjoint)
//!
//! Every code `run_package` can emit falls into exactly one bucket:
//!
//! - **LSP-owned** — produced by the LSP's synchronous file-local pass
//!   (and fed INTO the engine as its file-local injector). Filtered OUT of
//!   the engine's published stream here so they never double-fire.
//! - **Doctor-owned** — the package-level / cross-feature findings the
//!   synchronous pass cannot compute. Published by the background run.
//!
//! [`partition_is_total_and_disjoint`] (exercised by a unit test) asserts
//! the two predicates never overlap and that an engine finding is always
//! classified.

use std::path::Path;

use lazuli_doctor_config::DoctorProfile;
use lazuli_doctor_run::{DoctorDiagnostic, DoctorSeverity, run_package};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range, Url};

use crate::diagnostics::doctor_local::feature_header_range;
use crate::diagnostics_for_source_with_profile;

/// Classify a doctor finding code as **LSP-owned** (produced by the LSP's
/// synchronous file-local pass) vs **doctor-owned** (only the package
/// engine computes it).
///
/// LSP-owned codes are the file-local shape / contract diagnostics plus
/// the kebab-case security opt-out codes the synchronous pass emits. The
/// engine's file-local injector is that same pass, so the package run's
/// stream contains them too — we drop them from the published background
/// stream so they aren't double-published over the synchronous ones.
///
/// The classifier is intentionally code-shape-based (not an enumerated
/// allowlist) so the partition stays total as rules are added: anything
/// that is a kebab-case "contract/shape" code is LSP-owned; everything
/// else (the `SCREAMING-KEBAB-NNN` rule-catalog codes, coverage findings,
/// manifest/version checks) is doctor-owned.
pub(crate) fn is_lsp_owned(code: &str) -> bool {
    // Rule-catalog codes are `SCREAMING-KEBAB` optionally suffixed with a
    // `-NNN` numeric tail (e.g. `REF-CROSS-FEATURE-UNKNOWN-001`,
    // `MANIFEST-REQUIRED-001`, `SCHEMA-RICH-001`). The synchronous LSP
    // pass does NOT compute those — they are package/cross-feature work —
    // so they are doctor-owned.
    if is_screaming_rule_code(code) {
        return false;
    }
    // Everything else the engine can surface in the merged stream is a
    // file-local shape / contract / security diagnostic the synchronous
    // pass owns (lower / kebab-case codes like `env-schema-contract`,
    // `app-env-contract`, `auth-password-*`, …) plus the bare
    // `"diagnostic"` fallback for code-less parser errors.
    true
}

/// `true` when `code` is the engine's own (package-level) finding —
/// the complement of [`is_lsp_owned`].
pub(crate) fn is_doctor_owned(code: &str) -> bool {
    !is_lsp_owned(code)
}

/// `SCREAMING-KEBAB` (uppercase + digits + `-`) with at least one `-`.
/// Matches the rule-catalog code shape (`VOCAB-…`, `REF-…`, `MANIFEST-…`,
/// `SCHEMA-RICH-001`) and excludes lower/kebab contract codes
/// (`env-schema-contract`) and the `"diagnostic"` fallback.
fn is_screaming_rule_code(code: &str) -> bool {
    !code.is_empty()
        && code.contains('-')
        && code
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
}

/// Convert an engine [`DoctorSeverity`] into the editor severity.
fn lsp_severity(severity: DoctorSeverity) -> DiagnosticSeverity {
    match severity {
        DoctorSeverity::Error => DiagnosticSeverity::ERROR,
        DoctorSeverity::Warning => DiagnosticSeverity::WARNING,
        DoctorSeverity::Info => DiagnosticSeverity::INFORMATION,
        DoctorSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

/// Remap one engine [`DoctorDiagnostic`] onto an open document's editor
/// range. The engine anchors package findings at `(line, column)` in the
/// owning `.lzi`; when that file is the open `doc`, we honour the engine's
/// line/col, falling back to the `feature_name` header range (then line 0)
/// when the engine reports the synthetic `line: 1, column: 1` it uses for
/// project-level findings.
fn to_lsp_diagnostic(doc_source: &str, finding: &DoctorDiagnostic) -> Diagnostic {
    let range = if finding.line > 1 || finding.column > 1 {
        let line = finding.line.saturating_sub(1) as u32;
        let character = finding.column.saturating_sub(1) as u32;
        Range {
            start: Position { line, character },
            end: Position {
                line,
                character: character.saturating_add(1),
            },
        }
    } else {
        finding
            .feature_name
            .as_deref()
            .and_then(|name| feature_header_range(doc_source, name))
            .unwrap_or_else(|| Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            })
    };

    Diagnostic {
        range,
        severity: Some(lsp_severity(finding.severity)),
        code: Some(NumberOrString::String(finding.code.clone())),
        code_description: None,
        source: Some("lazuli-doctor".to_owned()),
        message: finding.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Run the package doctor engine against `workspace_root` and return the
/// **doctor-owned** findings that belong to `doc_uri`, remapped to editor
/// `Diagnostic`s over `doc_source`.
///
/// `profile` is the workspace `[doctor] profile` (resolved by the
/// backend); the engine's file-local injector reuses the LSP's own
/// synchronous pass at that profile so the merged stream stays consistent
/// with `lazuli doctor`. Returns an empty vec on any load failure (no
/// manifest, parse error, unreadable root) — the synchronous pass already
/// covers the editing-in-progress case.
pub(crate) fn doctor_owned_for_document(
    workspace_root: &Path,
    doc_uri: &Url,
    doc_source: &str,
    profile: DoctorProfile,
) -> Vec<Diagnostic> {
    let Ok(doc_path) = doc_uri.to_file_path() else {
        return Vec::new();
    };

    // File-local injector: the engine consumes the LSP's own synchronous
    // file-local pass per file, exactly as the CLI feeds it
    // `lazuli_lsp::diagnostics_for_source_with_profile`. The engine folds
    // them into its stream; we filter them back out below (they are
    // LSP-owned and published synchronously) to avoid double-fire.
    let file_local = |path: &Path, source: &str| -> Vec<DoctorDiagnostic> {
        diagnostics_for_source_with_profile(source, profile)
            .iter()
            .map(|d| lsp_to_doctor(path, d))
            .collect()
    };

    let Ok(package) = run_package(workspace_root, profile, &file_local, Vec::new()) else {
        return Vec::new();
    };

    package
        .diagnostics()
        .into_iter()
        .filter(|finding| is_doctor_owned(&finding.code))
        .filter(|finding| finding_belongs_to(&finding.path, workspace_root, &doc_path))
        .map(|finding| to_lsp_diagnostic(doc_source, &finding))
        .collect()
}

/// Does an engine finding's path point at the open document?
///
/// The engine renders finding paths **relative to the workspace root**
/// (via its `doctor_rule_path` strip), while the open document is an
/// absolute path. Resolve the finding path against the root (absolute
/// paths pass through) before comparing, and also accept a bare-suffix
/// match so the comparison is robust to path-normalization differences
/// across platforms.
fn finding_belongs_to(finding_path: &Path, workspace_root: &Path, doc_path: &Path) -> bool {
    if finding_path == doc_path {
        return true;
    }
    let resolved = if finding_path.is_absolute() {
        finding_path.to_path_buf()
    } else {
        workspace_root.join(finding_path)
    };
    resolved == doc_path || doc_path.ends_with(finding_path)
}

/// Inverse of `to_lsp_diagnostic` for the file-local injector: lift a
/// `tower_lsp` `Diagnostic` (the LSP file-local pass output) into the
/// engine's [`DoctorDiagnostic`] envelope, mirroring the CLI's
/// `doctor_diagnostic_from_lsp` adapter.
fn lsp_to_doctor(path: &Path, diagnostic: &Diagnostic) -> DoctorDiagnostic {
    let severity = match diagnostic.severity {
        Some(DiagnosticSeverity::ERROR) => DoctorSeverity::Error,
        Some(DiagnosticSeverity::WARNING) => DoctorSeverity::Warning,
        Some(DiagnosticSeverity::INFORMATION) => DoctorSeverity::Info,
        Some(DiagnosticSeverity::HINT) => DoctorSeverity::Hint,
        _ => DoctorSeverity::Warning,
    };
    let code = diagnostic
        .code
        .as_ref()
        .map(|code| match code {
            NumberOrString::String(value) => value.clone(),
            NumberOrString::Number(value) => value.to_string(),
        })
        .unwrap_or_else(|| "diagnostic".to_owned());

    DoctorDiagnostic {
        path: path.to_path_buf(),
        line: diagnostic.range.start.line as usize + 1,
        column: diagnostic.range.start.character as usize + 1,
        severity,
        code,
        message: diagnostic.message.clone(),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The partition MUST be total + disjoint: every code is classified by
    /// exactly one predicate. `is_doctor_owned` is the strict complement of
    /// `is_lsp_owned`, so this holds by construction — assert it across a
    /// representative sample spanning both buckets and the edge cases
    /// (empty, fallback, single-segment).
    #[test]
    fn partition_is_total_and_disjoint() {
        let codes = [
            // doctor-owned (SCREAMING-KEBAB rule catalog)
            "REF-CROSS-FEATURE-UNKNOWN-001",
            "MANIFEST-REQUIRED-001",
            "SCHEMA-RICH-001",
            "VOCAB-CONTEXT-PURPOSE-001",
            "REF-POLYMORPHIC-TARGET-001",
            "AUDIT-MATERIALIZE-TARGET-001",
            // lsp-owned (kebab contract / fallback)
            "env-schema-contract",
            "app-env-contract",
            "auth-password-no-session",
            "diagnostic",
            "",
            "WORD",
        ];
        for code in codes {
            // Disjoint: never both.
            assert!(
                !(is_lsp_owned(code) && is_doctor_owned(code)),
                "{code}: classified as BOTH lsp-owned and doctor-owned"
            );
            // Total: always one.
            assert!(
                is_lsp_owned(code) || is_doctor_owned(code),
                "{code}: classified as NEITHER"
            );
        }
    }

    #[test]
    fn doctor_owned_recognises_cross_feature_code() {
        assert!(is_doctor_owned("REF-CROSS-FEATURE-UNKNOWN-001"));
        assert!(!is_lsp_owned("REF-CROSS-FEATURE-UNKNOWN-001"));
    }

    #[test]
    fn lsp_owned_recognises_kebab_contract_code() {
        assert!(is_lsp_owned("env-schema-contract"));
        assert!(!is_doctor_owned("env-schema-contract"));
    }
}
