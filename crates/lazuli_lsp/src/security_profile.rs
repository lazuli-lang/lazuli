//! Security-profile narrowing applied to the canonical-source
//! diagnostic stream just before the LSP emits it.
//!
//! Codes that authors flag as **enforcement** (e.g. `command-policy`,
//! `command-rate-limit`, `webhook-verify`, `crypto-tier`) carry their
//! own severity; this layer rewrites them to match the active profile:
//!
//! * `Prototype` — enforcement codes become WARNING.
//! * `Strict` / `Production` — enforcement codes become ERROR;
//!   `Production` additionally promotes the documented opt-out code
//!   (`security-opt-out`) from WARNING to ERROR.
//!
//! ## W2 — single source of truth
//!
//! The active profile now lives on the shared
//! [`ResolvedDoctorConfig`](lazuli_doctor_config::ResolvedDoctorConfig)
//! (loaded from the workspace `[doctor] profile`), not on a separately
//! threaded `SecurityProfile`. This layer reads
//! `config.profile.0` so the LSP and CLI resolve enforcement-code
//! posture from the same loaded config.
//!
//! These enforcement / opt-out codes are kebab-case LSP-only codes with
//! no peer in the doctor catalog, so they keep their bespoke
//! profile→severity mapping rather than flowing through
//! `effective_severity` (which would resolve them under the catch-all
//! `Vocabulary` category and lose the enforcement posture). Doctor-class
//! *codes* are routed through `effective_severity` upstream in
//! `doctor_local`.
//!
//! The helpers are re-exported at the crate root via
//! `pub(crate) use crate::security_profile::*;` in `lib.rs` so
//! `crate::dispatch::diagnostics_for_with_profile_inner` keeps
//! calling them through the same paths.

use lazuli_doctor_config::{DoctorProfile, ResolvedDoctorConfig};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

pub(crate) fn apply_security_profile(
    mut diagnostics: Vec<Diagnostic>,
    config: &ResolvedDoctorConfig,
) -> Vec<Diagnostic> {
    let profile = config.profile.0;
    for diagnostic in &mut diagnostics {
        let Some(code) = diagnostic_code(diagnostic) else {
            continue;
        };

        if is_security_enforcement_code(code) {
            diagnostic.severity = Some(match profile {
                DoctorProfile::Prototype => DiagnosticSeverity::WARNING,
                DoctorProfile::Strict | DoctorProfile::Production | DoctorProfile::IronHand => {
                    DiagnosticSeverity::ERROR
                }
            });
        } else if matches!(profile, DoctorProfile::Production | DoctorProfile::IronHand)
            && is_security_opt_out_code(code)
        {
            diagnostic.severity = Some(DiagnosticSeverity::ERROR);
        }
    }

    diagnostics
}

pub(crate) fn diagnostic_code(diagnostic: &Diagnostic) -> Option<&str> {
    match diagnostic.code.as_ref()? {
        tower_lsp::lsp_types::NumberOrString::String(code) => Some(code.as_str()),
        tower_lsp::lsp_types::NumberOrString::Number(_) => None,
    }
}

pub(crate) fn is_security_enforcement_code(code: &str) -> bool {
    matches!(
        code,
        "command-policy"
            | "command-rate-limit"
            | "scope-override-policy"
            | "scope-override-reason"
            | "field-security-policy"
            | "webhook-verify"
            | "webhook-idempotency"
            | "event-job-tenant-from"
            | "event-consumer-payload"
            | "crypto-tier"
            | "crypto-hash-algorithm"
            | "crypto-key-scope"
            | "crypto-token-contract"
            | "crypto-capability-arguments"
            | "escape-route-security"
            | "auth-password-algorithm"
            | "auth-password-rate-limit"
            | "auth-session-ttl"
            // IR-driven session-query temporal-validity invariant
            // (SESSION-QUERY-TEMPORAL-VALIDITY-001). Joins its
            // session-family peers `auth-session-ttl` /
            // `auth_sessions_resource_unknown`: WARNING under prototype,
            // ERROR under strict/production so it blocks under the
            // scaffolded strict profile. The warn-only in-editor
            // `active-session-temporal-scope` squiggle stays as-is.
            | "session-query-temporal-validity"
            // IR-driven preventability guard: the session resource bound by
            // `auth sessions resource <X>` must declare every column the
            // runtime resolver reads (AUTH-SESSION-CANONICAL-COLUMNS-001).
            // Joins its session-family peers — WARNING under prototype,
            // ERROR under strict/production so it blocks under the scaffolded
            // strict profile.
            | "auth-session-canonical-columns"
            | "auth_password_algorithm_hash_mismatch"
            | "auth_sessions_resource_unknown"
            | "auth_identity_field_unknown"
            | "auth_oauth_adapter_unbound"
            | "security-opt-out-reason"
    )
}

pub(crate) fn is_security_opt_out_code(code: &str) -> bool {
    matches!(code, "security-opt-out")
}
