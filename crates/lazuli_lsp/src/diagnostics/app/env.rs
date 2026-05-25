//! `env` block line validators for `app`.
//!
//! `env` declarations carry the only secrets/config surface that
//! Lazuli treats as first-class. The grammar is:
//!
//! ```text
//! env
//!   server NAME: Secret|Text|Url|Boolean|Integer required|optional [in <env>]
//!   client PUBLIC_NAME: Text required
//!   mobile EXPO_PUBLIC_NAME: Text required
//!   group <name>
//!     server NAME: ...        # nested under a group
//! ```
//!
//! This module owns the line-shape check + the two visibility
//! warnings (`env-client-exposure`, `env-mobile-exposure`) that flag
//! client/mobile bindings missing the required `PUBLIC` /
//! `EXPO_PUBLIC_` token. The group header parser is also here because
//! it is purely a name-extraction helper for `env`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, simple_canonical_diagnostic, split_items};

pub(crate) fn validate_app_env_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if !valid_env_declaration_parts(&parts) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "app-env-contract",
            "app env declarations use `server|client|mobile NAME: Secret|Text|Url|Boolean|Integer required|optional [in environment]`.",
        ));
        return;
    }

    let name = parts[1].trim_end_matches(':');
    if parts[0] == "client" && !has_public_token(name) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "env-client-exposure",
            "client env names should contain a `PUBLIC` token (e.g. `PUBLIC_MERCADOPAGO_KEY` or vendor-style `MERCADOPAGO_PUBLIC_KEY`) so secret/server-only values are not accidentally bundled.",
        ));
    }

    if parts[0] == "mobile" && !name.starts_with("EXPO_PUBLIC_") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "env-mobile-exposure",
            "mobile env names should use an `EXPO_PUBLIC_` prefix so Expo-visible values are explicit.",
        ));
    }
}

/// Closes WAR-DOCTOR-ENV-01 false-positive. `PUBLIC` may appear as
/// the leading token (`PUBLIC_API_KEY`) OR as a mid-name token
/// (`MERCADOPAGO_PUBLIC_KEY`, `STRIPE_PUBLIC_KEY`). Vendor SDKs
/// frequently impose the latter shape because their key names are
/// fixed by the upstream service. As long as `PUBLIC` shows up as a
/// `_`-delimited token, the author has signalled intent to expose.
pub(crate) fn has_public_token(name: &str) -> bool {
    name.split('_').any(|token| token == "PUBLIC")
}

pub(crate) fn valid_env_declaration_parts(parts: &[&str]) -> bool {
    let has_environment_scope = parts.len() >= 6
        && parts[4] == "in"
        && split_items(&parts[5..].join(" "))
            .iter()
            .all(|environment| is_identifier(environment));

    (parts.len() == 4 || has_environment_scope)
        && matches!(parts[0], "server" | "client" | "mobile")
        && parts[1].ends_with(':')
        && matches!(parts[2], "Secret" | "Text" | "Url" | "Boolean" | "Integer")
        && matches!(parts[3], "required" | "optional")
}

pub(crate) fn parse_env_group_name(trimmed: &str) -> Option<&str> {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() == 2 && parts[0] == "group" && is_identifier(parts[1]) {
        Some(parts[1])
    } else {
        None
    }
}
