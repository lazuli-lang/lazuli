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

use lazuli_doctor_config::ResolvedDoctorConfig;
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
///
/// `pub` (not `pub(crate)`) only so the anti-drift parity test
/// (`crate::test_surface`) can assert the partition is total + disjoint
/// over the sampled codes; not part of the LSP's public API contract.
pub fn is_lsp_owned(code: &str) -> bool {
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
///
/// `pub` (not `pub(crate)`) only so the anti-drift parity test
/// (`crate::test_surface`) can assert the partition is total + disjoint;
/// not part of the LSP's public API contract.
pub fn is_doctor_owned(code: &str) -> bool {
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

/// Build the single synthetic diagnostic published when the package engine
/// fails to *load* the workspace (broken/missing manifest, parse error,
/// unreadable root). Anchored at line 0 of the open document so the editor
/// shows a top-of-file marker rather than silently going green.
///
/// The code is the SCREAMING-KEBAB `LOAD-FAILED-001`, which
/// [`is_doctor_owned`] keeps doctor-owned (published by the Layer-2
/// background run, never double-fired by the file-local pass). It is built
/// through [`to_lsp_diagnostic`] — by reporting the synthetic `line: 1,
/// column: 1` with no `feature_name`, the remapper lands it at line 0,
/// exactly as it does for project-level findings.
fn load_failure_diagnostic(doc_source: &str, err: impl std::fmt::Display) -> Diagnostic {
    let finding = DoctorDiagnostic {
        path: std::path::PathBuf::new(),
        line: 1,
        column: 1,
        severity: DoctorSeverity::Error,
        code: "LOAD-FAILED-001".to_owned(),
        message: format!("doctor could not load package: {err}"),
        category: None,
        feature_name: None,
        construct: None,
        fix: None,
        group: None,
    };
    to_lsp_diagnostic(doc_source, &finding)
}

/// Run the package doctor engine against `workspace_root` and return the
/// **doctor-owned** findings that belong to `doc_uri`, remapped to editor
/// `Diagnostic`s over `doc_source`.
///
/// `config` is the workspace severity [`ResolvedDoctorConfig`] resolved by
/// the backend (profile + presets + per-rule overrides), built
/// buffer-preferring from the open `Lazurite.toml` editor buffer when that
/// file is open, else from disk. The engine's file-local injector reuses
/// the LSP's own synchronous pass at the config's profile so the merged
/// stream stays consistent with `lazuli doctor`; the package layer's
/// severity is driven by the same `config` — so an unsaved `[doctor]` edit
/// changes both layers' published severities live.
///
/// v2 — previously this took only the `DoctorProfile`; the engine
/// internally re-derived presets/overrides from its on-disk manifest read,
/// so unsaved `[doctor]` preset/override edits did not reach package
/// findings. Now the full config is threaded into `run_package`.
///
/// On a package **load failure** (no manifest, parse error, unreadable
/// root) the engine no longer goes silently green: it surfaces ONE
/// synthetic doctor-owned `LOAD-FAILED-001` diagnostic at line 0 so the
/// editor signals that the package layer could not run. The synchronous
/// file-local pass still owns the editing-in-progress squiggles; this
/// single diagnostic is purely the "Layer 2 could not load" signal.
pub(crate) fn doctor_owned_for_document(
    workspace_root: &Path,
    doc_uri: &Url,
    doc_source: &str,
    config: &ResolvedDoctorConfig,
) -> Vec<Diagnostic> {
    let Ok(doc_path) = doc_uri.to_file_path() else {
        return Vec::new();
    };

    let profile = config.profile.0;

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

    let package = match run_package(workspace_root, config, &file_local, Vec::new()) {
        Ok(package) => package,
        // Load failure (missing/parse-broken manifest, unreadable root, …):
        // surface a single synthetic doctor-owned diagnostic instead of an
        // empty set so the editor does not go silently green.
        Err(e) => return vec![load_failure_diagnostic(doc_source, &e)],
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

    /// `LOAD-FAILED-001` is SCREAMING-KEBAB, so the partition keeps it
    /// doctor-owned (published by the Layer-2 background run, never dropped
    /// as a double-fire of the file-local pass).
    #[test]
    fn load_failed_code_is_doctor_owned() {
        assert!(is_doctor_owned("LOAD-FAILED-001"));
        assert!(!is_lsp_owned("LOAD-FAILED-001"));
    }

    /// Regression: when `run_package` fails to LOAD the workspace (here: an
    /// empty directory with no `.lzi`/`.lzx` and no manifest), the engine
    /// must surface ONE synthetic `LOAD-FAILED-001` diagnostic at line 0 —
    /// NOT an empty set that would let the editor go silently green.
    #[test]
    fn doctor_owned_for_document_surfaces_load_failure() {
        // Unique empty workspace root — `collect_package_paths` finds no
        // `.lzi`/`.lzx`, so `run_package` returns `Err`.
        let root = std::env::temp_dir().join(format!(
            "lazuli-lsp-loadfail-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&root).expect("create temp workspace root");

        let doc_path = root.join("feature.lzi");
        let doc_uri = Url::from_file_path(&doc_path).expect("file url from absolute path");
        let config = ResolvedDoctorConfig::default();

        let diagnostics = doctor_owned_for_document(&root, &doc_uri, "feature x\n", &config);

        // Cleanup regardless of assertion outcome.
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(
            diagnostics.len(),
            1,
            "a load failure must surface exactly one synthetic diagnostic, got {diagnostics:?}"
        );
        let diag = &diagnostics[0];
        assert_eq!(
            diag.code,
            Some(NumberOrString::String("LOAD-FAILED-001".to_owned())),
            "the synthetic diagnostic must carry the LOAD-FAILED-001 code"
        );
        assert_eq!(diag.range.start.line, 0, "anchored at line 0");
        assert!(
            diag.message.contains("doctor could not load package"),
            "message should explain the load failure, got: {}",
            diag.message
        );
    }
}
