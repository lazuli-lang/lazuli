//! `DoctorPackage::diagnostics` — the doctor dispatcher.
//!
//! Fans out every `*_diagnostics` aggregator on the loaded
//! `DoctorPackage`, then sorts + suppresses the result.
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 3.
//! The aggregators themselves live in `doctor/aggregators/*` or as
//! `pub(super) fn *_diagnostics` items in `doctor/mod.rs`.

use std::collections::BTreeSet;

use super::aggregators::cross_feature::{
    app_urls_missing_diagnostics, collect_known_audiences, scope_owner_column_diagnostics,
};
use super::aggregators::env_manifest::{
    cap_file_policy_implicit_diagnostics, dedupe_env_contract_diagnostics,
    manifest_required_diagnostics, suppress_env_schema_when_declared,
};
use super::aggregators::field_health::{
    field_derived_from_unresolved_diagnostics, resource_unique_qualifier_unknown_diagnostics,
    resource_validates_path_unknown_diagnostics,
};
use super::aggregators::runtime_version::{
    lazuli_version_001_diagnostics, lazuli_version_002_diagnostics, schema_rich_gap_diagnostics,
};
use super::aggregators::ts_consumers::{
    import_deprecated_alias_diagnostics, manual_param_coercion_diagnostics,
};
use super::package::DoctorPackage;
use super::{
    DoctorDiagnostic, DoctorSeverity, LZIR_SCHEMA, aggregators, approval_diagnostics,
    approval_missing_children_diagnostics, auth_refresh, cap_file_storage_diagnostics,
    check_auth_session_callsite_001, check_codegen_wrap_001, check_pattern_draft_stale_001,
    collect_callable_bodies_for_eval_order, collect_known_roles,
    creates_empty_bindings_diagnostics, cross_feature_type_unresolved_diagnostics, doctor_rule_path,
    duplicate_query_name_diagnostics, feature_uses_missing_diagnostics,
    lazurite_manifest_diagnostics, lifecycle_gate,
    missing_policy_on_query_diagnostics, mutation_without_readback_diagnostics,
    operational_env_names, policy_reachability_diagnostics, policy_ref_unresolved_diagnostics,
    query_view_sql_file_diagnostics,
    rbac_catalog_diagnostics, rbac_catalog_missing_diagnostics, rbac_missing_policy_diagnostics,
    rbac_role_undeclared_diagnostics, report_diagnostics, returns_list_001, returns_list_002,
    route_guard, route_id_effect_consistency_diagnostics, updates_missing_updated_at_diagnostics,
    vocab_grammar_form_diagnostics,
};

include!("dispatch_impl1.rs");
include!("dispatch_impl2.rs");
