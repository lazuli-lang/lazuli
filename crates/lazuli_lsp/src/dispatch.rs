//! Top-level dispatcher that turns a Lazuli source string into the
//! aggregate `Vec<Diagnostic>` published to the LSP client (and to the
//! CLI doctor via `diagnostics_for_source_with_profile`).
//!
//! The dispatcher walks three mutually-exclusive shapes:
//!
//! * **Canonical `.lzi` source** (`app`/`registry`/`profile`/`workspace`
//!   /`contract`/`design`/`feature`/top-level `env`) — fan out across
//!   ~70 catalog-specific diagnostic producers, then optionally include
//!   the `lazuli_doctor` file-local cross-checks, then apply the active
//!   security profile.
//! * **`.lzx` surface source** — narrow walk for the experience-level
//!   route contract diagnostics plus the shared namespace/extension
//!   checks.
//! * **Free-form feature skeletons** — parse + analyzer lowering only;
//!   any error here short-circuits with a single `lazuli-syntax` or
//!   `lazuli-analyzer` diagnostic.
//!
//! The dispatch is intentionally a flat list of `.extend(...)` calls so
//! authoring a new catalog producer is "add one line." That shape is the
//! entire reason this module exists separately from `lib.rs`: it isolates
//! the registry from the ~22k lines of producer bodies that surround it.
//!
//! ## See also
//! * `lib.rs::diagnostics_for_with_profile_inner` — re-exported as
//!   `pub(crate)` from this module for the LSP backend + CLI entry
//!   points.
//! * `lib.rs::apply_security_profile` — narrows the canonical-source
//!   output by the active profile carried in the
//!   `lazuli_doctor_config::ResolvedDoctorConfig`.
//! * `crate::doctor_file_local_diagnostics` — the bridge that pulls
//!   `lazuli_doctor` checks into the live LSP stream (proposal R2.F).

use lazuli_doctor_config::ResolvedDoctorConfig;
use lazuli_syntax::parse_feature_skeletons;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    active_session_query_diagnostics, agent_contract_diagnostics, agent_discriminator_diagnostics,
    agent_evals_diagnostics, agent_expose_diagnostics, agent_tools_diagnostics,
    anchor_whitelist_diagnostics, api_contract_diagnostics, app_operational_contract_diagnostics,
    app_unknown_kind_diagnostics, apply_security_profile, approval_contract_diagnostics,
    audience_unknown_kind_diagnostics, auth_security_diagnostics, cache_contract_diagnostics,
    canonical_order_diagnostics, command_contract_diagnostics,
    command_rate_limit_contract_diagnostics, command_statement_unknown_diagnostics,
    command_validator_diagnostics, cors_contract_diagnostics, crypto_contract_diagnostics,
    defaults_policy_syntax_diagnostics, derived_field_diagnostics, emits_derived_diagnostics,
    env_schema_diagnostics, env_top_level_legacy_diagnostics, error_contract_diagnostics,
    escape_route_security_diagnostics, event_consumer_payload_diagnostics,
    event_job_tenant_from_diagnostics, event_kind_diagnostics, event_locator_diagnostics,
    event_payload_reference_diagnostics, event_trace_trigger_diagnostics,
    extension_declaration_diagnostics, extension_reference_diagnostics,
    external_call_contract_diagnostics, external_contract_diagnostics,
    feature_requirements_contract_diagnostics, feature_unknown_kind_diagnostics,
    field_security_policy_diagnostics, file_capability_contract_diagnostics, first_line_range,
    generated_summary_diagnostics, has_many_diagnostics, headers_contract_diagnostics,
    idempotency_key_diagnostics, is_canonical_source, is_lzx_source, lookup_shorthand_diagnostics,
    lzx_contract_diagnostics, lzx_route_contract_diagnostics, namespace_reference_diagnostics,
    non_goals_shape_diagnostics, notification_contract_diagnostics, policy_namespace_diagnostics,
    scalar_alias_diagnostics,
    previously_mode_diagnostics, profile_contract_diagnostics, query_filter_index_diagnostics,
    query_mode_diagnostics, query_order_default_diagnostics, query_pagination_diagnostics,
    query_search_syntax_diagnostics, query_statement_unknown_diagnostics, range_from_span,
    refs_block_diagnostics, registry_contract_diagnostics, registry_unknown_kind_diagnostics,
    required_field_nil_rule_diagnostics, reserved_trace_event_diagnostics,
    retention_contract_diagnostics, rule_self_diagnostics, scheduled_job_tenancy_diagnostics,
    scope_override_policy_diagnostics, secret_rotation_contract_diagnostics,
    sessions_unknown_kind_diagnostics, sql_return_type_diagnostics,
    surface_unknown_kind_diagnostics, target_binding_diagnostics, test_block_diagnostics,
    type_namespace_diagnostics, validation_syntax_diagnostics, view_unknown_kind_diagnostics,
    webhook_security_diagnostics, webhook_tenant_from_diagnostics, workspace_contract_diagnostics,
    write_window_contract_diagnostics,
};

/// Which consumer is driving the dispatch — selects whether the canonical
/// branch runs the real parser/lower as a backstop after the ~70
/// text-pattern producers.
///
/// * [`DiagnosticMode::Cli`] — the `lazuli check` batch path. Runs the
///   parser/lower backstop synchronously so a genuine syntax/lowering
///   error inside a STRICT block that has *no* text-pattern producer
///   (e.g. an unknown child in a `job`/`poller`/`notification` block)
///   still surfaces — the confirmed BUG 2 Part B exit-0 regression.
/// * [`DiagnosticMode::Editor`] — the LSP per-keystroke synchronous
///   Layer-1 pass (and the file-local injector the package engine reuses).
///   Does **not** run the parser backstop: the editor already receives
///   parse/lower failures via the debounced Layer-2 `run_package` stream
///   (`crate::doctor_engine`, now doctor-owned after the BUG 2 Part A
///   `PARSE-FAILED-001` / `LOWER-FAILED-001` rename), so running it here
///   too would double-fire and cause squiggle flicker.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DiagnosticMode {
    Cli,
    Editor,
}

pub(crate) fn diagnostics_for_with_profile_inner(
    source: &str,
    config: &ResolvedDoctorConfig,
    mode: DiagnosticMode,
) -> Vec<Diagnostic> {
    if is_canonical_source(source) {
        let mut diagnostics = canonical_order_diagnostics(source);
        diagnostics.extend(feature_unknown_kind_diagnostics(source));
        // 2026-05-15 typo-detection sweep — six sibling contexts where
        // a typo silently drops the block. See `closest_kind`.
        diagnostics.extend(app_unknown_kind_diagnostics(source));
        diagnostics.extend(registry_unknown_kind_diagnostics(source));
        diagnostics.extend(view_unknown_kind_diagnostics(source));
        diagnostics.extend(surface_unknown_kind_diagnostics(source));
        diagnostics.extend(command_statement_unknown_diagnostics(source));
        diagnostics.extend(query_statement_unknown_diagnostics(source));
        diagnostics.extend(audience_unknown_kind_diagnostics(source));
        diagnostics.extend(sessions_unknown_kind_diagnostics(source));
        diagnostics.extend(query_mode_diagnostics(source));
        diagnostics.extend(previously_mode_diagnostics(source));
        diagnostics.extend(app_operational_contract_diagnostics(source));
        diagnostics.extend(registry_contract_diagnostics(source));
        diagnostics.extend(profile_contract_diagnostics(source));
        diagnostics.extend(workspace_contract_diagnostics(source));
        diagnostics.extend(external_contract_diagnostics(source));
        diagnostics.extend(feature_requirements_contract_diagnostics(source));
        diagnostics.extend(external_call_contract_diagnostics(source));
        diagnostics.extend(generated_summary_diagnostics(source));
        diagnostics.extend(non_goals_shape_diagnostics(source));
        diagnostics.extend(defaults_policy_syntax_diagnostics(source));
        diagnostics.extend(lookup_shorthand_diagnostics(source));
        diagnostics.extend(namespace_reference_diagnostics(source));
        diagnostics.extend(scalar_alias_diagnostics(source));
        diagnostics.extend(refs_block_diagnostics(source));
        diagnostics.extend(policy_namespace_diagnostics(source));
        diagnostics.extend(scope_override_policy_diagnostics(source));
        diagnostics.extend(query_order_default_diagnostics(source));
        diagnostics.extend(query_pagination_diagnostics(source));
        diagnostics.extend(query_filter_index_diagnostics(source));
        diagnostics.extend(query_search_syntax_diagnostics(source));
        diagnostics.extend(active_session_query_diagnostics(source));
        diagnostics.extend(command_rate_limit_contract_diagnostics(source));
        diagnostics.extend(event_job_tenant_from_diagnostics(source));
        diagnostics.extend(scheduled_job_tenancy_diagnostics(source));
        diagnostics.extend(crypto_contract_diagnostics(source));
        diagnostics.extend(file_capability_contract_diagnostics(source));
        diagnostics.extend(sql_return_type_diagnostics(source));
        diagnostics.extend(type_namespace_diagnostics(source));
        diagnostics.extend(validation_syntax_diagnostics(source));
        diagnostics.extend(derived_field_diagnostics(source));
        diagnostics.extend(has_many_diagnostics(source));
        diagnostics.extend(agent_contract_diagnostics(source));
        diagnostics.extend(agent_tools_diagnostics(source));
        diagnostics.extend(agent_evals_diagnostics(source));
        diagnostics.extend(agent_discriminator_diagnostics(source));
        diagnostics.extend(agent_expose_diagnostics(source));
        diagnostics.extend(reserved_trace_event_diagnostics(source));
        diagnostics.extend(approval_contract_diagnostics(source));
        diagnostics.extend(cors_contract_diagnostics(source));
        diagnostics.extend(headers_contract_diagnostics(source));
        diagnostics.extend(secret_rotation_contract_diagnostics(source));
        diagnostics.extend(notification_contract_diagnostics(source));
        diagnostics.extend(emits_derived_diagnostics(source));
        diagnostics.extend(extension_declaration_diagnostics(source));
        diagnostics.extend(event_payload_reference_diagnostics(source));
        diagnostics.extend(event_kind_diagnostics(source));
        diagnostics.extend(event_trace_trigger_diagnostics(source));
        diagnostics.extend(event_consumer_payload_diagnostics(source));
        diagnostics.extend(event_locator_diagnostics(source));
        diagnostics.extend(target_binding_diagnostics(source));
        diagnostics.extend(rule_self_diagnostics(source));
        diagnostics.extend(required_field_nil_rule_diagnostics(source));
        diagnostics.extend(command_validator_diagnostics(source));
        diagnostics.extend(error_contract_diagnostics(source));
        diagnostics.extend(cache_contract_diagnostics(source));
        diagnostics.extend(api_contract_diagnostics(source));
        diagnostics.extend(anchor_whitelist_diagnostics(source));
        diagnostics.extend(test_block_diagnostics(source));
        diagnostics.extend(command_contract_diagnostics(source));
        diagnostics.extend(field_security_policy_diagnostics(source));
        diagnostics.extend(retention_contract_diagnostics(source));
        diagnostics.extend(write_window_contract_diagnostics(source));
        diagnostics.extend(env_schema_diagnostics(source));
        diagnostics.extend(env_top_level_legacy_diagnostics(source));
        diagnostics.extend(webhook_security_diagnostics(source));
        diagnostics.extend(webhook_tenant_from_diagnostics(source));
        diagnostics.extend(escape_route_security_diagnostics(source));
        diagnostics.extend(auth_security_diagnostics(source));
        diagnostics.extend(extension_reference_diagnostics(source));
        diagnostics.extend(idempotency_key_diagnostics(source));
        // D3 — the doctor file-local mirror is gone. Those package /
        // cross-feature ("doctor-owned") findings are now produced by the
        // in-editor package-engine run (`crate::doctor_engine`), published
        // by the backend's debounced Layer-2 task.
        //
        // BUG 2 Part B — parser/lower backstop, CLI-only. The ~70
        // producers above are text-pattern detectors; several STRICT
        // blocks (`job`, `poller`, `notification`, `report`, `cache`,
        // `agent`, lifecycle, auth sub-blocks, `locale_negotiate`, …) have
        // NO producer, so a genuine syntax error inside one of them is
        // silently dropped and `lazuli check` exits 0. We run the real
        // parser + lower here and append their failures, deduped against
        // any ERROR a producer already reported on the same line so a
        // block that DOES have a producer (command/query/view/surface/
        // sessions) still shows exactly once. Editor Layer-1 skips this
        // (it gets parse/lower failures from Layer-2 instead) — no
        // cross-layer dedup is needed.
        if mode == DiagnosticMode::Cli {
            let parse_backstop = parse_backstop_diagnostics(source);
            append_deduped_by_line(&mut diagnostics, parse_backstop);
        }
        return apply_security_profile(diagnostics, config);
    }

    if is_lzx_source(source) {
        let mut diagnostics = lzx_contract_diagnostics(source);
        diagnostics.extend(lzx_route_contract_diagnostics(source));
        diagnostics.extend(namespace_reference_diagnostics(source));
        diagnostics.extend(scalar_alias_diagnostics(source));
        diagnostics.extend(extension_reference_diagnostics(source));
        return diagnostics;
    }

    let features = match parse_feature_skeletons(source) {
        Ok(features) => features,
        Err(error) => return vec![parse_error_diagnostic(source, &error)],
    };

    for feature in &features {
        if let Err(error) = lazuli_analyzer::lower_feature_skeleton(feature) {
            return vec![lower_error_diagnostic(source, &error)];
        }
    }

    Vec::new()
}

/// Build the syntax-error `Diagnostic` for a `parse_feature_skeletons`
/// failure. Extracted (with byte-identical span/source/severity logic) so
/// both the free-form branch and the CLI canonical-branch backstop share
/// one definition of how a parser error renders.
fn parse_error_diagnostic(source: &str, error: &lazuli_syntax::ParseError) -> Diagnostic {
    Diagnostic {
        range: range_from_span(source, error.span()),
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("lazuli-syntax".to_owned()),
        message: error.to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Build the analyzer-error `Diagnostic` for a `lower_feature_skeleton`
/// failure. Extracted alongside [`parse_error_diagnostic`] so the
/// first-line span/source/severity logic is defined exactly once.
fn lower_error_diagnostic(source: &str, error: &lazuli_analyzer::AnalyzeError) -> Diagnostic {
    Diagnostic {
        range: first_line_range(source),
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("lazuli-analyzer".to_owned()),
        message: error.to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Run the real parser + analyzer lower over a canonical `.lzi` source and
/// return every syntax/lowering failure as a `Diagnostic`, reusing the
/// shared [`parse_error_diagnostic`] / [`lower_error_diagnostic`] builders.
///
/// A parse failure short-circuits (the skeletons are unavailable, so
/// lowering cannot run); otherwise each skeleton that fails to lower
/// contributes one diagnostic. This is the CLI-only backstop wired into the
/// canonical branch — it is intentionally *not* on the editor's
/// per-keystroke Layer-1 path.
fn parse_backstop_diagnostics(source: &str) -> Vec<Diagnostic> {
    let features = match parse_feature_skeletons(source) {
        Ok(features) => features,
        Err(error) => return vec![parse_error_diagnostic(source, &error)],
    };

    let mut diagnostics = Vec::new();
    for feature in &features {
        if let Err(error) = lazuli_analyzer::lower_feature_skeleton(feature) {
            diagnostics.push(lower_error_diagnostic(source, &error));
        }
    }
    diagnostics
}

/// Append `incoming` diagnostics to `existing`, dropping any incoming whose
/// `range.start.line` already carries an ERROR-severity diagnostic in
/// `existing`. This keeps a STRICT block that already has a text-pattern
/// producer (which emits its own line-anchored ERROR) from double-firing
/// the parser/lower backstop on the same line, while a producer-less block
/// (whose error only the backstop sees) still surfaces.
fn append_deduped_by_line(existing: &mut Vec<Diagnostic>, incoming: Vec<Diagnostic>) {
    use std::collections::HashSet;
    let error_lines: HashSet<u32> = existing
        .iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .map(|d| d.range.start.line)
        .collect();
    for diagnostic in incoming {
        if diagnostic.severity == Some(DiagnosticSeverity::ERROR)
            && error_lines.contains(&diagnostic.range.start.line)
        {
            continue;
        }
        existing.push(diagnostic);
    }
}
