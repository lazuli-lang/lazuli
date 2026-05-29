//! Sub-file of the inline LSP test suite — see `mod.rs` for the
//! shared preamble and helpers.
//!
//! W2 coverage: doctor-class diagnostic severity is resolved through the
//! shared `lazuli_doctor_config` resolver, so the editor severity is
//! mode-aware — it changes with the workspace `[doctor]` profile / preset
//! / per-rule override, and matches what `lazuli doctor` would emit.
#![allow(unused_imports)]
use lazuli_doctor_config::{DoctorProfile, ResolvedDoctorConfig};
use tower_lsp::lsp_types::DiagnosticSeverity;

use super::*;
use crate::{diagnostics_for_with_config, doctor_class_lsp_severity};

/// The `VOCAB-CONTEXT-PURPOSE-001` trio is the task's named example: it
/// is suppressed (`None`) under the `off` coverage preset and escalated
/// to `Error` under `tdd-iron-hand`, while staying at its intrinsic
/// `Warning` base under a plain strict workspace. The bridge that maps a
/// finding's `(code, base_severity)` to an editor severity must reflect
/// each of those modes.
#[test]
fn vocab_context_trio_severity_is_mode_aware() {
    let trio = [
        "VOCAB-CONTEXT-PURPOSE-001",
        "VOCAB-CONTEXT-NONGOALS-001",
        "VOCAB-CONTEXT-CTXMD-001",
    ];

    // Plain strict workspace (no preset): intrinsic Warning base is kept.
    let strict = ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap();
    for code in trio {
        assert_eq!(
            doctor_class_lsp_severity(code, DiagnosticSeverity::WARNING, &strict),
            Some(DiagnosticSeverity::WARNING),
            "{code} should stay WARNING under a plain strict workspace",
        );
    }

    // `tdd-iron-hand` coverage preset escalates the trio to Error.
    let iron = ResolvedDoctorConfig::resolve(
        Some("[doctor.coverage]\npreset = \"tdd-iron-hand\"\n"),
        DoctorProfile::Strict,
    )
    .unwrap();
    for code in trio {
        assert_eq!(
            doctor_class_lsp_severity(code, DiagnosticSeverity::WARNING, &iron),
            Some(DiagnosticSeverity::ERROR),
            "{code} should escalate to ERROR under tdd-iron-hand",
        );
    }

    // `off` coverage preset suppresses the trio entirely — no diagnostic.
    let off = ResolvedDoctorConfig::resolve(
        Some("[doctor.coverage]\npreset = \"off\"\n"),
        DoctorProfile::Strict,
    )
    .unwrap();
    for code in trio {
        assert_eq!(
            doctor_class_lsp_severity(code, DiagnosticSeverity::WARNING, &off),
            None,
            "{code} should be suppressed (no diagnostic) under the off preset",
        );
    }
}

/// A manifest per-rule `severity_override` must move the editor severity
/// of an otherwise intrinsic-`Error` correctness rule. This is the
/// level-1 precedence the bridge inherits from the shared resolver.
#[test]
fn manifest_override_moves_doctor_class_severity() {
    let code = "HOOK-TARGET-001"; // correctness rule, intrinsic ERROR base.

    // No manifest: intrinsic Error base is kept (NOT downgraded to the
    // profile default — would diverge from `lazuli doctor`).
    let bare = ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap();
    assert_eq!(
        doctor_class_lsp_severity(code, DiagnosticSeverity::ERROR, &bare),
        Some(DiagnosticSeverity::ERROR),
    );

    // A manifest override down-grades it to a hint.
    let overridden = ResolvedDoctorConfig::resolve(
        Some(
            "[doctor.test_discipline.severity_override.\"HOOK-TARGET-001\"]\n\
             severity = \"hint\"\n\
             reason = \"under migration\"\n",
        ),
        DoctorProfile::Strict,
    )
    .unwrap();
    assert_eq!(
        doctor_class_lsp_severity(code, DiagnosticSeverity::ERROR, &overridden),
        Some(DiagnosticSeverity::HINT),
    );
}

/// End-to-end: a wired doctor-class finding (`VOCAB-AUDIT-001`, surfaced
/// through `doctor_local`) flows through the full diagnostic pass and its
/// editor severity reflects a workspace manifest override — proving the
/// config threads from `diagnostics_for_with_config` all the way to the
/// `doctor_local` trees bridge.
#[test]
fn wired_finding_severity_reflects_workspace_config() {
    // A write command without `audit` is the textbook VOCAB-AUDIT-001
    // trigger (intrinsic ERROR base in the LSP bridge).
    let source = r#"
feature widget
  purpose "Widgets"

  domain
    resource Widget

  policies
    create: @role.admin

  command create
    policy @policy.create
    rate_limit "30 per hour per user"
    creates Widget
"#;

    // Default strict workspace: the rule fires at its intrinsic ERROR.
    let strict = ResolvedDoctorConfig::resolve(None, DoctorProfile::Strict).unwrap();
    let diags = diagnostics_for_with_config(source, &strict);
    let hits = doctor_diagnostics_with_code(&diags, "VOCAB-AUDIT-001");
    assert!(!hits.is_empty(), "VOCAB-AUDIT-001 should fire");
    assert_eq!(
        hits[0].severity,
        Some(DiagnosticSeverity::ERROR),
        "intrinsic posture is ERROR under a plain strict workspace",
    );

    // Same source, but the workspace overrides the rule to a warning:
    // the editor severity tracks the workspace config, not a hardcoded
    // literal.
    let overridden = ResolvedDoctorConfig::resolve(
        Some(
            "[doctor.test_discipline.severity_override.\"VOCAB-AUDIT-001\"]\n\
             severity = \"warning\"\n\
             reason = \"audit backfill scheduled\"\n",
        ),
        DoctorProfile::Strict,
    )
    .unwrap();
    let diags = diagnostics_for_with_config(source, &overridden);
    let hits = doctor_diagnostics_with_code(&diags, "VOCAB-AUDIT-001");
    assert!(!hits.is_empty(), "VOCAB-AUDIT-001 should still fire");
    assert_eq!(
        hits[0].severity,
        Some(DiagnosticSeverity::WARNING),
        "editor severity must follow the workspace [doctor] override",
    );
}
