//! Per-tree fan-out of `lazuli_doctor` file-local feature checks.
//!
//! Each sub-tree gets its own dispatcher (`wire_correctness`,
//! `wire_domain`, …) called by `doctor_file_local_diagnostics` for
//! every parsed feature. Splitting by tree keeps the orchestrator
//! readable while preserving the LSP / doctor ABI contract documented
//! in `super::mod`.
//!
//! See `editors/vscode/AUDIT-LSP-DOCTOR-GAP.md` (R2.F) for the catalog
//! of 46+ file-local-portable codes wired here.

use lazuli_doctor_config::ResolvedDoctorConfig;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::{doctor_class_lsp_severity, doctor_diagnostic};

/// Dispatch a `(feature, path) -> Vec<Finding>` doctor leaf and push
/// each finding as an LSP `Diagnostic`. The `$severity` argument is the
/// rule's intrinsic ("base") posture — `ERROR` by default; callers pass
/// `WARNING` for the warning-severity rules (e.g. SLUG-UNIQUENESS).
///
/// W2: the base posture is no longer emitted verbatim. Each finding's
/// `(code, base_severity, category)` is resolved through the shared
/// `lazuli_doctor_config` resolver so the workspace `[doctor]` profile /
/// preset / per-rule override decides the editor severity (or suppresses
/// the diagnostic entirely when the resolver returns `None`).
macro_rules! wire_feature_check {
    ($source:expr, $diags:expr, $feature:expr, $path:expr, $config:expr, $module:path, $severity:expr) => {{
        use $module as leaf;
        if let Some(severity) = doctor_class_lsp_severity(leaf::Finding::CODE, $severity, $config) {
            for finding in leaf::check($feature, $path) {
                $diags.push(doctor_diagnostic(
                    $source,
                    Some(&$feature.name),
                    leaf::Finding::CODE,
                    finding.message(),
                    severity,
                ));
            }
        }
    }};
    ($source:expr, $diags:expr, $feature:expr, $path:expr, $config:expr, $module:path) => {{
        wire_feature_check!(
            $source,
            $diags,
            $feature,
            $path,
            $config,
            $module,
            DiagnosticSeverity::ERROR
        );
    }};
}

pub(crate) fn wire_correctness(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
    feature: &lazuli_ir::Feature,
    synthetic_path: &std::path::Path,
    config: &ResolvedDoctorConfig,
) {
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::correctness::channel_payload_unresolved_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::correctness::command_input_shadows_field_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::correctness::composite_key_contract_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::correctness::duplicate_query_name
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::correctness::full_text_type_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::correctness::hook_target_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::correctness::resource_lock_contract_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::correctness::route_id_effect_consistency
    );
}

pub(crate) fn wire_domain(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
    feature: &lazuli_ir::Feature,
    synthetic_path: &std::path::Path,
    config: &ResolvedDoctorConfig,
) {
    // SLUG-UNIQUENESS-IMPLICIT is warning per the R2.F audit.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::aggregate_contains_unknown
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::aggregate_root_unknown
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::invariant_predicate_invalid
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::slug_uniqueness_implicit,
        DiagnosticSeverity::WARNING
    );
    // GAP-NEW-001 — conditional `unique ... when` predicate / column
    // field-reference check.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::constraint_unique_when_invalid
    );
    // GAP-09 — input-value-predicate policy atom `input.*` field-reference
    // + atom-namespace check.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::policy_predicate_invalid
    );
    // W3 GAP-03 — `computed_date from <base> offset <offset>` base/offset
    // type-check.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::computed_date_expr_invalid
    );
    // W4 GAP-08 — `schedule_rule from @fn.<name>(<arg>) offset <offset>`
    // binding-fn / rule-arg / offset type-check.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::schedule_rule_invalid
    );
    // W4 GAP-AUDIT-02 — `command` updates/deletes an `append_only` resource.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::resource_append_only_invalid
    );
    // GAP-AUDIT-01 — `command` ... `audit materialize @feature.<f>.<R>`
    // must target a reachable append_only OperationLog. Cross-feature
    // resolution needs the package (CLI dispatch); the LSP surfaces the
    // same-feature subset (`check_local`) live in the editor.
    {
        use lazuli_doctor::cross_feature::audit_materialize_target_001 as leaf;
        if let Some(severity) =
            doctor_class_lsp_severity(leaf::Finding::CODE, DiagnosticSeverity::ERROR, config)
        {
            for finding in leaf::check_local(feature, synthetic_path) {
                diagnostics.push(doctor_diagnostic(
                    source,
                    Some(&feature.name),
                    leaf::Finding::CODE,
                    finding.message(),
                    severity,
                ));
            }
        }
    }
    // W4 GAP-REORDER-01 — `reorder <Resource> by <field>` position-field
    // type-check.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::reorder_position_field_invalid
    );
    // GAP-07 — `many_through <Junction> to <Partner>` partner-endpoint
    // resolution + payload-type legality.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::domain::many_through_endpoint_001
    );
}

pub(crate) fn wire_lifecycle(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
    feature: &lazuli_ir::Feature,
    synthetic_path: &std::path::Path,
    config: &ResolvedDoctorConfig,
) {
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::enum_duplicate
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::field_double_declared
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::invariant_param_unresolved
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::no_initial_state
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::state_duplicate
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::terminal_has_outgoing
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::timestamp_type
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::transition_from_undeclared
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::transition_to_undeclared
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::unreachable_state
    );
    // Cell D additions — closed-invariant-catalog + policy-required
    // surface drift wired into the LSP per lifecycle-vocab.md §5.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::policy_required
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::no_jump_needs_linear
    );
    // LIFECYCLE-INITIAL-AMBIGUOUS is severity=warning per the proposal
    // table (3+ states, implicit initial — cold-reader clarity, not a
    // hard error).
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::initial_ambiguous,
        DiagnosticSeverity::WARNING
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::lifecycle::invariant_catalog_mismatch
    );
}

pub(crate) fn wire_vocab(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
    feature: &lazuli_ir::Feature,
    synthetic_path: &std::path::Path,
    config: &ResolvedDoctorConfig,
) {
    // 10 feature-based checks; vocab_grammar_form_001 is wired on raw
    // source in the orchestrator (no lowering needed).
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_audit_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_audit_002
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_cap_missing_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_derived_read_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_event_orphan_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_event_payload_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_event_producer_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_json_typed_001
    );
    // NOTE: `vocab_lifecycle_001.rs` exists in the crate but is not
    // exported via `vocab/mod.rs` (orphan file, pending extraction
    // wave). Re-add the `wire_feature_check!` line here once the
    // parent module publishes it.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_union_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::vocab::vocab_union_002
    );
}

pub(crate) fn wire_encryption(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
    feature: &lazuli_ir::Feature,
    synthetic_path: &std::path::Path,
    config: &ResolvedDoctorConfig,
) {
    // 2 of 6 — the other 4 need `AppManifest` / `AppRegistry`.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::encryption::e2ee_event
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::encryption::tenancy
    );
}

pub(crate) fn wire_poller(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
    feature: &lazuli_ir::Feature,
    synthetic_path: &std::path::Path,
    config: &ResolvedDoctorConfig,
) {
    // 10 of 12 — `cursor_field_type_001` and `exponential_no_cap_001`
    // exist in the crate but are not exported via `poller/mod.rs`;
    // re-wire when the parent module publishes them.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::cursor_missing_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::dual_scheduler_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::handler_orphan_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::idempotency_attempts_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::max_retries_unbounded_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::no_terminal_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::quirk_catalog_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::terminal_field_enum_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::terminal_no_emit_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::poller::tick_too_fast_001
    );
}

pub(crate) fn wire_report(
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
    feature: &lazuli_ir::Feature,
    synthetic_path: &std::path::Path,
    config: &ResolvedDoctorConfig,
) {
    // 9 of 12 — signed_no_storage / storage_ambiguous / format_unknown
    // need registry / AST context not available here.
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_column_mismatch_001
    );
    // W5 GAP-REPORT-01 — REPORT-INPUT-UNBOUND-001 (warning).
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_input_unbound_001,
        DiagnosticSeverity::WARNING
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_columns_empty_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_filename_token_unknown_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_path_collision_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_policy_public_no_rate_limit_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_signed_ttl_forbidden_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_signed_ttl_missing_001
    );
    wire_feature_check!(
        source,
        diagnostics,
        feature,
        synthetic_path,
        config,
        lazuli_doctor::report::report_source_kind_001
    );
}
