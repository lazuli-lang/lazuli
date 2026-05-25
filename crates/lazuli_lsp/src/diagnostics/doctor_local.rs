//! Doctor file-local diagnostic wiring.
//!
//! Bridges the  file-local check leaves (8 sub-trees, 46+
//! codes per ) into the LSP so each
//! rule surfaces as live editor squiggles, not just on  CLI
//! runs. Cross-file checks that need AppManifest / AppRegistry /
//! object-storage caps / design-token allowlist /  filesystem walk
//! stay in  since the LSP has no project context.
//!
//! Public surface (re-exported at the crate root via
//!  in ):
//! , , and
//! . The  macro is
//! module-local — used only by .

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::{first_line_range, leading_spaces};

// ── doctor file-local diagnostics ────────────────────────────────────────────
//
// Wires the file-local checks from `lazuli_doctor` (8 sub-trees) into the LSP
// so each rule surfaces as live editor squiggles, not just on `lazuli doctor`
// CLI runs. See `editors/vscode/AUDIT-LSP-DOCTOR-GAP.md` (R2.F) for the catalog
// of 46+ file-local-portable codes this wires up.
//
// Cross-file checks (need `AppManifest` / `AppRegistry` / object-storage caps /
// design-token allowlist / `.tsx` filesystem walk) stay in `lazuli_cli::doctor`
// since the LSP has no project context.

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

/// Macro: dispatch a `(feature, path) -> Vec<Finding>` doctor leaf and push
/// each finding as an LSP `Diagnostic`. Severity defaults to `ERROR`; callers
/// override per-leaf for the warning-severity rules (e.g. SLUG-UNIQUENESS).
macro_rules! wire_feature_check {
    ($source:expr, $diags:expr, $feature:expr, $path:expr, $module:path, $severity:expr) => {{
        use $module as leaf;
        for finding in leaf::check($feature, $path) {
            $diags.push(doctor_diagnostic(
                $source,
                Some(&$feature.name),
                leaf::Finding::CODE,
                finding.message(),
                $severity,
            ));
        }
    }};
    ($source:expr, $diags:expr, $feature:expr, $path:expr, $module:path) => {{
        wire_feature_check!(
            $source,
            $diags,
            $feature,
            $path,
            $module,
            DiagnosticSeverity::ERROR
        );
    }};
}

pub(crate) fn doctor_file_local_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    // VOCAB-GRAMMAR-FORM-001 runs on raw source (no lowering needed) and
    // surfaces 1-indexed line/column itself.
    let synthetic_path = std::path::Path::new("source.lzi");
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
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(tower_lsp::lsp_types::NumberOrString::String(
                lazuli_doctor::vocab::vocab_grammar_form_001::Finding::CODE.to_owned(),
            )),
            code_description: None,
            source: Some("lazuli-doctor".to_owned()),
            message: finding.message(),
            related_information: None,
            tags: None,
            data: None,
        });
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
        // correctness (7)
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::correctness::channel_payload_unresolved_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::correctness::command_input_shadows_field_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::correctness::composite_key_contract_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::correctness::duplicate_query_name
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::correctness::full_text_type_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::correctness::hook_target_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::correctness::resource_lock_contract_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::correctness::route_id_effect_consistency
        );

        // domain (4) — SLUG-UNIQUENESS-IMPLICIT is warning per the audit.
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::domain::aggregate_contains_unknown
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::domain::aggregate_root_unknown
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::domain::invariant_predicate_invalid
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::domain::slug_uniqueness_implicit,
            DiagnosticSeverity::WARNING
        );

        // lifecycle (10)
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::enum_duplicate
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::field_double_declared
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::invariant_param_unresolved
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::no_initial_state
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::state_duplicate
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::terminal_has_outgoing
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::timestamp_type
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::transition_from_undeclared
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::transition_to_undeclared
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::lifecycle::unreachable_state
        );

        // vocab (11 feature-based; vocab_grammar_form_001 wired above on source)
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_audit_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_audit_002
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_cap_missing_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_derived_read_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_event_orphan_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_event_payload_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_event_producer_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_json_typed_001
        );
        // NOTE: `vocab_lifecycle_001.rs` exists in the crate but is not
        // exported via `vocab/mod.rs` (orphan file, pending extraction wave).
        // Re-add the `wire_feature_check!` line here once the parent module
        // publishes it.
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_union_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::vocab::vocab_union_002
        );

        // encryption (2 of 6 — the other 4 need `AppManifest` / `AppRegistry`).
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::encryption::e2ee_event
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::encryption::tenancy
        );

        // poller (10 of 12 — `cursor_field_type_001` and
        // `exponential_no_cap_001` exist in the crate but are not exported
        // via `poller/mod.rs`; re-wire when the parent module publishes them).
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::cursor_missing_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::dual_scheduler_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::handler_orphan_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::idempotency_attempts_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::max_retries_unbounded_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::no_terminal_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::quirk_catalog_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::terminal_field_enum_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::terminal_no_emit_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::poller::tick_too_fast_001
        );

        // report (8 of 11 — signed_no_storage / storage_ambiguous /
        // format_unknown need registry / AST context not available here).
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::report::report_column_mismatch_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::report::report_columns_empty_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::report::report_filename_token_unknown_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::report::report_path_collision_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::report::report_policy_public_no_rate_limit_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::report::report_signed_ttl_forbidden_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::report::report_signed_ttl_missing_001
        );
        wire_feature_check!(
            source,
            diagnostics,
            feature,
            synthetic_path,
            lazuli_doctor::report::report_source_kind_001
        );
    }

    diagnostics
}
