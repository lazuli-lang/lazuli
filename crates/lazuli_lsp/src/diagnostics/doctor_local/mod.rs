//! Doctor file-local diagnostic wiring.
//!
//! Bridges the `lazuli_doctor` file-local check leaves (8 sub-trees,
//! 46+ codes per the R2.F audit) into the LSP so each rule surfaces
//! as live editor squiggles, not just on `lazuli doctor` CLI runs.
//! Cross-file checks that need `AppManifest` / `AppRegistry` /
//! object-storage caps / design-token allowlist / `.tsx` filesystem
//! walk stay in `lazuli_cli::doctor` since the LSP has no project
//! context.
//!
//! Public surface (re-exported at the crate root via
//! `pub(crate) use diagnostics::doctor_local::*;` in `lib.rs`):
//! [`doctor_diagnostic`], [`feature_header_range`], and
//! [`doctor_file_local_diagnostics`]. The per-tree fan-out
//! (`wire_correctness`, …) lives in [`trees`] and is private to
//! this module.
//!
//! See `editors/vscode/AUDIT-LSP-DOCTOR-GAP.md` (R2.F) for the catalog
//! of 46+ file-local-portable codes wired here.

use lazuli_doctor_config::{
    DoctorSeverity, ResolvedDoctorConfig, RuleCategory, effective_severity_over_base,
};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::{first_line_range, leading_spaces};

mod trees;

/// Map a resolved [`DoctorSeverity`] to the editor [`DiagnosticSeverity`].
///
/// This mapping lives in `lazuli_lsp` (not the config crate) because
/// `lazuli_doctor_config` is deliberately `tower-lsp`-free. It is the
/// inverse of `lazuli_cli::doctor::diagnostic::from_lsp`.
pub(crate) fn lsp_severity(severity: DoctorSeverity) -> DiagnosticSeverity {
    match severity {
        DoctorSeverity::Error => DiagnosticSeverity::ERROR,
        DoctorSeverity::Warning => DiagnosticSeverity::WARNING,
        DoctorSeverity::Info => DiagnosticSeverity::INFORMATION,
        DoctorSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

/// Resolve the editor severity for a doctor-class finding through the
/// shared [`effective_severity_over_base`] resolver.
///
/// `base_severity` is the rule's intrinsic posture — the hardcoded
/// `ERROR` / `WARNING` literal the LSP bridge used to emit directly, and
/// the same posture the CLI doctor's aggregators emit for these
/// file-local codes. The resolver keeps that base as the floor and only
/// lets a manifest override / coverage preset / category preset *move*
/// it; it deliberately does NOT apply the level-4 per-profile category
/// default (that match governs the CLI's VOCAB-CONTEXT + test-discipline
/// paths, not these hardcoded file-local severities — applying it would
/// silently downgrade an `Error`-base correctness rule to `Warning` at
/// `strict`, diverging from `lazuli doctor`).
///
/// The category is derived from the rule code prefix —
/// [`RuleCategory::from_code_prefix`] — exactly as the CLI doctor does
/// for codes that don't carry an explicit category.
///
/// Returns `None` when the rule is SILENT under the active config
/// (e.g. the `Off` coverage preset suppressing the `VOCAB-CONTEXT-*`
/// family). Callers emit no diagnostic for `None`.
pub(crate) fn doctor_class_lsp_severity(
    code: &str,
    base_severity: DiagnosticSeverity,
    config: &ResolvedDoctorConfig,
) -> Option<DiagnosticSeverity> {
    let base = match base_severity {
        DiagnosticSeverity::ERROR => DoctorSeverity::Error,
        DiagnosticSeverity::WARNING => DoctorSeverity::Warning,
        DiagnosticSeverity::INFORMATION => DoctorSeverity::Info,
        DiagnosticSeverity::HINT => DoctorSeverity::Hint,
        _ => DoctorSeverity::Warning,
    };
    effective_severity_over_base(code, base, RuleCategory::from_code_prefix(code), config)
        .map(lsp_severity)
}

/// Build a `Diagnostic` for any `lazuli_doctor` finding that exposes the
/// canonical `Finding::CODE: &'static str` + `fn message(&self) -> String`
/// shape. We don't have AST spans on the IR-level findings, so the range
/// targets the `feature <name>` line when locatable, else line 0.
pub(crate) fn doctor_diagnostic(
    source: &str,
    feature_name: Option<&str>,
    code: &str,
    message: String,
    severity: DiagnosticSeverity,
) -> Diagnostic {
    let range = feature_name
        .and_then(|name| feature_header_range(source, name))
        .unwrap_or_else(|| first_line_range(source));

    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            code.to_owned(),
        )),
        code_description: None,
        source: Some("lazuli-doctor".to_owned()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Best-effort lookup of the `feature <name>` declaration line so doctor
/// findings can attach to the feature header instead of line 0.
pub(crate) fn feature_header_range(source: &str, feature_name: &str) -> Option<Range> {
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let after_kw = trimmed.strip_prefix("feature ")?.trim_start();
        if after_kw
            .split(|c: char| c.is_whitespace())
            .next()
            .map(|n| n.eq_ignore_ascii_case(feature_name))
            .unwrap_or(false)
        {
            let leading = leading_spaces(line);
            return Some(Range {
                start: Position {
                    line: idx as u32,
                    character: leading as u32,
                },
                end: Position {
                    line: idx as u32,
                    character: line.len() as u32,
                },
            });
        }
    }
    None
}

pub(crate) fn doctor_file_local_diagnostics(
    source: &str,
    config: &ResolvedDoctorConfig,
) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // VOCAB-GRAMMAR-FORM-001 runs on raw source (no lowering needed)
    // and surfaces 1-indexed line/column itself.
    let synthetic_path = std::path::Path::new("source.lzi");
    let grammar_form_code = lazuli_doctor::vocab::vocab_grammar_form_001::Finding::CODE;
    if let Some(severity) =
        doctor_class_lsp_severity(grammar_form_code, DiagnosticSeverity::WARNING, config)
    {
        for finding in lazuli_doctor::vocab::vocab_grammar_form_001::check(source, synthetic_path) {
            let line_zero = finding.line.saturating_sub(1) as u32;
            let col_zero = finding.column.saturating_sub(1) as u32;
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: line_zero,
                        character: col_zero,
                    },
                    end: Position {
                        line: line_zero,
                        character: col_zero + finding.old.chars().count() as u32,
                    },
                },
                severity: Some(severity),
                code: Some(tower_lsp::lsp_types::NumberOrString::String(
                    grammar_form_code.to_owned(),
                )),
                code_description: None,
                source: Some("lazuli-doctor".to_owned()),
                message: finding.message(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }

    // Everything else needs lowered IR. Bail silently if parsing or
    // lowering fails — shape diagnostics already surface those errors
    // via other paths.
    let Ok(skeletons) = lazuli_syntax::parse_feature_skeletons(source) else {
        return diagnostics;
    };
    let features: Vec<lazuli_ir::Feature> = skeletons
        .iter()
        .filter_map(|skeleton| lazuli_analyzer::lower_feature_skeleton(skeleton).ok())
        .collect();

    for feature in &features {
        trees::wire_correctness(source, &mut diagnostics, feature, synthetic_path, config);
        trees::wire_domain(source, &mut diagnostics, feature, synthetic_path, config);
        trees::wire_lifecycle(source, &mut diagnostics, feature, synthetic_path, config);
        trees::wire_vocab(source, &mut diagnostics, feature, synthetic_path, config);
        trees::wire_encryption(source, &mut diagnostics, feature, synthetic_path, config);
        trees::wire_poller(source, &mut diagnostics, feature, synthetic_path, config);
        trees::wire_report(source, &mut diagnostics, feature, synthetic_path, config);
    }

    diagnostics
}
