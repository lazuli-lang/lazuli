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
use crate::doctor_class_lsp_severity;

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

/// End-to-end (D3): a package/cross-feature ("doctor-owned") finding
/// (`VOCAB-AUDIT-001`) flows through the in-editor package-engine run
/// (`run_package`) and its editor severity is **mode-aware** — it tracks
/// the workspace `[doctor] profile`, matching what `lazuli doctor` emits.
/// This proves the engine threads the profile from the backend all the
/// way to the published Layer-2 squiggle.
#[test]
fn wired_finding_severity_reflects_workspace_config() {
    // A write command without `audit` is the textbook VOCAB-AUDIT-001
    // trigger. Vocabulary rules resolve `Strict -> Warning`,
    // `Production -> Error` through `doctor_severity_for`.
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

    // Strict workspace: vocabulary rule fires at WARNING in-editor.
    let strict = doctor_engine_diagnostics_for("widget", source, SecurityProfile::Strict);
    let strict_hits = doctor_diagnostics_with_code(&strict, "VOCAB-AUDIT-001");
    assert!(
        !strict_hits.is_empty(),
        "VOCAB-AUDIT-001 should fire through the in-editor engine under strict"
    );
    assert_eq!(
        strict_hits[0].severity,
        Some(DiagnosticSeverity::WARNING),
        "vocabulary posture is WARNING under a strict workspace",
    );

    // Production workspace: the SAME finding escalates to ERROR — the
    // editor severity tracks the workspace mode, not a hardcoded literal.
    let production = doctor_engine_diagnostics_for("widget", source, SecurityProfile::Production);
    let prod_hits = doctor_diagnostics_with_code(&production, "VOCAB-AUDIT-001");
    assert!(
        !prod_hits.is_empty(),
        "VOCAB-AUDIT-001 should still fire under production"
    );
    assert_eq!(
        prod_hits[0].severity,
        Some(DiagnosticSeverity::ERROR),
        "editor severity must escalate to ERROR under a production workspace",
    );
}

/// D3 named deliverable — a **cross-feature** package finding
/// (`REF-CROSS-FEATURE-UNKNOWN-001`) now surfaces in-editor through the
/// package engine. A field FK `target @feature.other.Thing` whose feature
/// isn't in `uses` is the textbook trigger; the synchronous file-local
/// pass cannot compute it (it needs the whole package), so this proves the
/// Layer-2 engine run delivers package-level findings to the editor.
#[test]
fn cross_feature_ref_unknown_surfaces_in_editor() {
    let source = r#"
feature orders
  domain
    resource Order
      customer_id: ID target @feature.customers.Customer
"#;
    let diags = doctor_engine_diagnostics_for("orders", source, SecurityProfile::Strict);
    let hits = doctor_diagnostics_with_code(&diags, "REF-CROSS-FEATURE-UNKNOWN-001");
    assert!(
        !hits.is_empty(),
        "REF-CROSS-FEATURE-UNKNOWN-001 should surface in-editor via the package engine; got: {:?}",
        diags
            .iter()
            .filter_map(|d| d.code.as_ref())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hits[0].severity,
        Some(DiagnosticSeverity::ERROR),
        "cross-feature ref errors are ERROR severity",
    );
    assert_eq!(hits[0].source.as_deref(), Some("lazuli-doctor"));
}
