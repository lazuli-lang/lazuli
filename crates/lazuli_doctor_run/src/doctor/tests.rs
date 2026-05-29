//! Doctor command test suite.
//!
//! Extracted verbatim from `doctor/mod.rs` to keep mod.rs lean.
//! Do not add new tests outside of this `mod tests { ... }` block - the
//! wrapper preserves string-literal indentation for the existing tests.

mod tests {
    // Imports trimmed after the R10-A split: every `#[test]` body now lives in
    // a sibling sub-module under `tests/` and carries its own `use` lines.
    // The parent retains `test_support_core` + `test_support_packages` shim
    // declarations so sub-modules can reach them via `super::*` and so the
    // `mod` ordering keeps the existing test path
    // `crate::doctor::tests::tests::<sub_module>::<test_fn>`.

    mod test_support_core {
        include!("tests/test_support_core.rs");
    }

    mod test_support_packages {
        include!("tests/test_support_packages.rs");
    }

    mod codegen_pattern {
        include!("tests/codegen_pattern.rs");
    }

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

    mod auth_actor_subject {
        include!("tests/auth_actor_subject.rs");
    }

    mod session_query_temporal_validity {
        include!("tests/session_query_temporal_validity.rs");
    }

    mod session_cookie {
        include!("tests/session_cookie.rs");
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
