//! Block-header recognition + scalar-child validation for `app`.
//!
//! The app dispatcher in `app/mod.rs` walks each indent-2 line and
//! asks two questions:
//!
//! 1. Is this a known multi-line block header (`uses`, `targets`,
//!    `services`, `runtime`, ...) — answered by `app_child_block`.
//! 2. Is this a known scalar child (`title`, `version`,
//!    `lazuli_version`, `default_locale`, ...) — answered by
//!    `is_app_scalar_child` and shape-checked by
//!    `validate_app_scalar_child`.
//!
//! Headers that *should* live on a bare line (no trailing tokens)
//! are flagged by `validate_app_child_header`. The two
//! `named_block_name` helpers exist here because they are the
//! generic backbone for `command_name_if` and any future
//! `<keyword> <name>` extractors.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, simple_canonical_diagnostic};

pub(crate) fn app_child_block(trimmed: &str) -> Option<&'static str> {
    let first = trimmed.split_whitespace().next()?;
    match first {
        "uses" => Some("uses"),
        "packs" => Some("packs"),
        "bindings" => Some("bindings"),
        "targets" => Some("targets"),
        "environments" => Some("environments"),
        "urls" => Some("urls"),
        "cors" => Some("cors"),
        // Roadmap §1.10 — `headers` block. Body validated by
        // `headers_contract_diagnostics`; the LSP only needs to
        // recognize the header so warnings don't fire on the
        // children.
        "headers" => Some("headers"),
        // Roadmap §1.2 — HTTP hygiene blocks. Bodies are validated
        // by doctor's app_(cookie|proxy|limits)_contract_diagnostics
        // (closed catalog, parseable size/duration). LSP only needs
        // to recognize the header so warnings don't fire on the
        // children.
        "cookie" => Some("cookie"),
        "proxy" => Some("proxy"),
        "limits" => Some("limits"),
        "env" => Some("env"),
        "integrations" => Some("integrations"),
        "capabilities" => Some("capabilities"),
        "architecture" => Some("architecture"),
        "services" => Some("services"),
        "communication" => Some("communication"),
        "runtime" => Some("runtime"),
        "deploy" => Some("deploy"),
        "route_guard" => Some("route_guard"),
        // Observability bucket cycle row 36 — `app.logging` /
        // `app.tracing` are first-class app blocks. Child slots
        // (`level`/`format`/`redact`/`sample_rate` for logging;
        // `propagate`/`sample_rate`/`exporter` for tracing) are
        // closed-catalog-checked by doctor.
        "logging" => Some("logging"),
        "tracing" => Some("tracing"),
        // i18n bucket cycle — `app.locale` block (default / supported /
        // fallback). Supersedes bare `default_locale` scalar.
        "locale" => Some("locale"),
        // Encryption bucket cycle — `app.encryption` block carries one
        // `key @key.<scope>` per scope referenced by
        // `@cap.Encrypted` / `@cap.E2ee` field sites. Body grammar
        // (`source` / `algorithm` / `rotation`) is doctor-validated;
        // the LSP only needs to recognize the header so warnings
        // don't fire on the children.
        "encryption" => Some("encryption"),
        // BUG-1 — `error_page <NNN>` opens an app-level error-page block
        // whose indent-4 children (`template` / `audience`) are validated
        // against the registry catalog by `validate_app_block_child`. The
        // trailing HTTP status is part of the header, so the bare-header
        // check in `validate_app_child_header` deliberately does NOT list
        // `error_page`.
        "error_page" => Some("error_page"),
        _ => None,
    }
}

pub(crate) fn command_name_if(trimmed: &str) -> Option<String> {
    named_block_name(trimmed, "command").map(str::to_owned)
}

pub(crate) fn named_block_name<'a>(trimmed: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = trimmed.strip_prefix(keyword)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let name = rest.split_whitespace().next()?;
    is_identifier(name).then_some(name)
}

pub(crate) fn is_app_scalar_child(trimmed: &str) -> bool {
    matches!(
        trimmed.split_whitespace().next(),
        Some(
            "title"
                | "version"
                // ABI pin enforced by doctor LAZULI-VERSION-001;
                // accept here so the LSP doesn't redundantly warn
                // that `lazuli_version "0.15"` isn't a recognized
                // app block.
                | "lazuli_version"
                | "default_locale"
                | "default_timezone"
                | "auth_failed_redirect"
                | "actor_query"
                | "not_found"
        )
    )
}

pub(crate) fn validate_app_child_header(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let first = trimmed.split_whitespace().next().unwrap_or_default();
    if matches!(
        first,
        "targets"
            | "bindings"
            | "packs"
            | "environments"
            | "urls"
            | "env"
            | "integrations"
            | "capabilities"
            | "architecture"
            | "services"
            | "communication"
            | "runtime"
            | "route_guard"
            | "deploy"
    ) && trimmed != first
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-operational-contract",
            "multi-line app manifest blocks use a bare block header, with entries nested below it.",
        ));
    }
}

pub(crate) fn validate_app_scalar_child(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() < 2 {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-operational-contract",
            "app scalar declarations need a value.",
        ));
    }
}
