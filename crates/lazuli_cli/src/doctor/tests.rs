//! Doctor command test suite.
//!
//! Extracted verbatim from `doctor/mod.rs` to keep mod.rs lean.
//! Do not add new tests outside of this `mod tests { ... }` block - the
//! wrapper preserves string-literal indentation for the existing tests.

mod tests {
    use crate::doctor::aggregators::auth::auth_diagnostics;
    use crate::doctor::aggregators::cross_feature::{
        APP_URLS_MISSING_MESSAGE, app_urls_missing_diagnostics, scope_owner_column_diagnostics,
    };
    use crate::doctor::aggregators::env_manifest::{
        cap_file_policy_implicit_diagnostics, dedupe_env_contract_diagnostics,
        manifest_required_diagnostics, suppress_env_schema_when_declared,
    };
    use crate::doctor::aggregators::field_health::{
        collect_unresolved_field_refs, field_derived_from_unresolved_diagnostics,
        resource_unique_qualifier_unknown_diagnostics, resource_validates_path_unknown_diagnostics,
    };
    use crate::doctor::aggregators::runtime_version::{
        lazuli_version_001_diagnostics, lazuli_version_002_diagnostics, schema_rich_gap_diagnostics,
    };
    use crate::doctor::aggregators::ts_consumers::{
        import_deprecated_alias_diagnostics, manual_param_coercion_diagnostics,
    };
    use crate::doctor::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    mod test_support_core {
        include!("tests/test_support_core.rs");
    }
    use test_support_core::*;

    mod codegen_pattern {
        include!("tests/codegen_pattern.rs");
    }


    mod test_support_packages {
        include!("tests/test_support_packages.rs");
    }
    use test_support_packages::*;

    mod manifest_plugin {
        include!("tests/manifest_plugin.rs");
    }

    mod surface_command {
        include!("tests/surface_command.rs");
    }

    mod app_manifest_registry {
        include!("tests/app_manifest_registry.rs");
    }

    mod integration_a {
        include!("tests/integration_a.rs");
    }

    mod integration_b {
        include!("tests/integration_b.rs");
    }

    mod integration_c {
        include!("tests/integration_c.rs");
    }

    mod design_query {
        include!("tests/design_query.rs");
    }

    mod version {
        include!("tests/version.rs");
    }

    mod hints_types {
        include!("tests/hints_types.rs");
    }

    mod agents_eval {
        include!("tests/agents_eval.rs");
    }
    mod app_logging {
        include!("tests/app_logging.rs");
    }

    mod scope_derived {
        include!("tests/scope_derived.rs");
    }

    mod event_health {
        include!("tests/event_health.rs");
    }

    mod cors_approval {
        include!("tests/cors_approval.rs");
    }

    mod event_trace_authored {
        include!("tests/event_trace_authored.rs");
    }

    mod expose_cap {
        include!("tests/expose_cap.rs");
    }

    mod auth_a {
        include!("tests/auth_a.rs");
    }

    mod auth_b {
        include!("tests/auth_b.rs");
    }

    mod lifecycle_misc {
        include!("tests/lifecycle_misc.rs");
    }

    mod observability_deprecated {
        include!("tests/observability_deprecated.rs");
    }

    mod locale_messages {
        include!("tests/locale_messages.rs");
    }

    mod policy_cache {
        include!("tests/policy_cache.rs");
    }

    mod openapi_webhook {
        include!("tests/openapi_webhook.rs");
    }

    mod notification {
        include!("tests/notification.rs");
    }

    mod auth_session {
        include!("tests/auth_session.rs");
    }

    mod headers_app_security {
        include!("tests/headers_app_security.rs");
    }

    mod err_vocab {
        include!("tests/err_vocab.rs");
    }

    mod route_guard_lifecycle {
        include!("tests/route_guard_lifecycle.rs");
    }
}
