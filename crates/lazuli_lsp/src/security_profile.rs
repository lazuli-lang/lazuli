//! Security-profile narrowing applied to the canonical-source
//! diagnostic stream just before the LSP emits it.
//!
//! Codes that authors flag as **enforcement** (e.g. `command-policy`,
//! `command-rate-limit`, `webhook-verify`, `crypto-tier`) carry their
//! own severity; this layer rewrites them to match the active
//! [`SecurityProfile`]:
//!
//! * `Prototype` — enforcement codes become WARNING.
//! * `Strict` / `Production` — enforcement codes become ERROR;
//!   `Production` additionally promotes the documented opt-out code
//!   (`security-opt-out`) from WARNING to ERROR.
//!
//! The helpers are re-exported at the crate root via
//! `pub(crate) use crate::security_profile::*;` in `lib.rs` so
//! `crate::dispatch::diagnostics_for_with_profile_inner` keeps
//! calling them through the same paths.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::types::SecurityProfile;

pub(crate) fn apply_security_profile(
    mut diagnostics: Vec<Diagnostic>,
    security_profile: SecurityProfile,
) -> Vec<Diagnostic> {
    for diagnostic in &mut diagnostics {
        let Some(code) = diagnostic_code(diagnostic) else {
            continue;
        };

        if is_security_enforcement_code(code) {
            diagnostic.severity = Some(match security_profile {
                SecurityProfile::Prototype => DiagnosticSeverity::WARNING,
                SecurityProfile::Strict | SecurityProfile::Production => DiagnosticSeverity::ERROR,
            });
        } else if security_profile == SecurityProfile::Production && is_security_opt_out_code(code)
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
