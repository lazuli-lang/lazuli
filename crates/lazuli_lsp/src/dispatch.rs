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
    previously_mode_diagnostics, profile_contract_diagnostics, query_filter_index_diagnostics,
    query_mode_diagnostics, query_order_default_diagnostics, query_pagination_diagnostics,
    query_search_syntax_diagnostics, query_statement_unknown_diagnostics, range_from_span,
    refs_block_diagnostics, registry_contract_diagnostics, registry_unknown_kind_diagnostics,
    required_field_nil_rule_diagnostics, reserved_trace_event_diagnostics,
    retention_contract_diagnostics, rule_self_diagnostics, scheduled_job_tenancy_diagnostics,
    scope_override_policy_diagnostics, secret_rotation_contract_diagnostics,
    sql_return_type_diagnostics, surface_unknown_kind_diagnostics, target_binding_diagnostics,
    test_block_diagnostics, type_namespace_diagnostics, validation_syntax_diagnostics,
    view_unknown_kind_diagnostics, webhook_security_diagnostics, webhook_tenant_from_diagnostics,
    workspace_contract_diagnostics, write_window_contract_diagnostics,
};

pub(crate) fn diagnostics_for_with_profile_inner(
    source: &str,
    config: &ResolvedDoctorConfig,
    _include_doctor: bool,
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
        // by the backend's debounced Layer-2 task. `_include_doctor` is
        // retained on the signature for the two callers' intent only.
        let _ = _include_doctor;
        return apply_security_profile(diagnostics, config);
    }

    if is_lzx_source(source) {
        let mut diagnostics = lzx_contract_diagnostics(source);
        diagnostics.extend(lzx_route_contract_diagnostics(source));
        diagnostics.extend(namespace_reference_diagnostics(source));
        diagnostics.extend(extension_reference_diagnostics(source));
        return diagnostics;
    }

    let features = match parse_feature_skeletons(source) {
        Ok(features) => features,
        Err(error) => {
            return vec![Diagnostic {
                range: range_from_span(source, error.span()),
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("lazuli-syntax".to_owned()),
                message: error.to_string(),
                related_information: None,
                tags: None,
                data: None,
            }];
        }
    };

    for feature in &features {
        if let Err(error) = lazuli_analyzer::lower_feature_skeleton(feature) {
            return vec![Diagnostic {
                range: first_line_range(source),
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("lazuli-analyzer".to_owned()),
                message: error.to_string(),
                related_information: None,
                tags: None,
                data: None,
            }];
        }
    }

    Vec::new()
}
