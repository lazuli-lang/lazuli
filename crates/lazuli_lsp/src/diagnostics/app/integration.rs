//! `integrations` block validators for `app`.
//!
//! Integrations are how an app names its outgoing dependency slots
//! (e.g. `crm: CRMProvider`) and binds them to a concrete adapter
//! source — runtime, plugin, or local file. Provider mechanics
//! (Stripe keys, MercadoPago URLs, Kafka topics) live in adapters;
//! the app only declares the *slot* and the *adapter source
//! provenance*.
//!
//! Three layers of validation live here:
//!
//! * header  — `<name>: <CapabilityType>`
//! * child   — `adapter @runtime/...` / `environments ...` /
//!             `credentials platform|tenant|actor` / `endpoint env.X` /
//!             `auth keys env.A env.B` / `data_classification @pii.<>`
//! * credential body — `<name> <source>` at indent 8
//!
//! `adapter_source_provenance` is also re-exported because
//! `diagnostics/registry.rs` reuses it to classify the same sources.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    is_identifier, is_quoted_lzx_literal, is_type_name, simple_canonical_diagnostic, split_items,
};

pub(crate) fn validate_app_integration_header(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if parse_app_integration_header(trimmed).is_none() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-integration-contract",
            "integrations use `<name>: <CapabilityType>` such as `crm: CRMProvider`; provider details stay in adapters.",
        ));
    }
}

pub(crate) fn parse_app_integration_header(trimmed: &str) -> Option<(&str, &str)> {
    let (name, kind) = trimmed.split_once(':')?;
    let name = name.trim();
    let kind = kind.trim();
    if is_identifier(name) && is_type_name(kind) {
        Some((name, kind))
    } else {
        None
    }
}

pub(crate) fn validate_app_integration_child(
    diagnostics: &mut Vec<Diagnostic>,
    current_integration_child: &mut Option<&'static str>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["adapter", adapter] if adapter_source_provenance(adapter).is_some() => {
            *current_integration_child = None;
        }
        ["environments", rest @ ..]
            if !rest.is_empty()
                && split_items(&rest.join(" "))
                    .iter()
                    .all(|environment| is_identifier(environment)) =>
        {
            *current_integration_child = None;
        }
        ["credentials", scope] if matches!(*scope, "platform" | "tenant" | "actor") => {
            *current_integration_child = Some("credentials");
        }
        ["data_classification", classification] if classification.starts_with("@pii.") => {
            *current_integration_child = None;
        }
        // B1 (W3-blockers) — `bindings` registry sugar accepted at the
        // same indent-6 site as the canonical integration children.
        // `endpoint <source>` lowers to a single credential binding;
        // `auth keys <id-source> <secret-source>` lowers to the two
        // positional S3-style credential bindings. Both lines reuse
        // the existing `parse_credential_binding`-shaped source grammar
        // (env.X / secrets.X / literal).
        ["endpoint", source] if !source.is_empty() => {
            *current_integration_child = None;
        }
        ["auth", "keys", id_source, secret_source]
            if !id_source.is_empty() && !secret_source.is_empty() =>
        {
            *current_integration_child = None;
        }
        _ => diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-integration-contract",
            "integration children use `adapter @runtime/...`, `adapter @lazuli/plugin-publisher/name`, `adapter @adapter.<local>`, local adapter paths, `environments ...`, `credentials platform|tenant|actor`, `endpoint env.<NAME>`, `auth keys env.<ID> env.<SECRET>`, or `data_classification @pii.<class>`.",
        )),
    }
}

pub(crate) fn adapter_source_provenance(source: &str) -> Option<&'static str> {
    if source
        .strip_prefix("@runtime/")
        .is_some_and(valid_pathish_tail)
    {
        Some("runtime")
    } else if source
        .strip_prefix("@lazuli/plugin-")
        .is_some_and(valid_plugin_tail)
    {
        Some("plugin")
    } else if source.strip_prefix("@adapter.").is_some_and(is_identifier)
        || source.starts_with("./")
        || source.starts_with("../")
        || is_quoted_lzx_literal(source)
    {
        Some("local")
    } else {
        None
    }
}

pub(crate) fn valid_plugin_tail(value: &str) -> bool {
    // Mirror `app_manifest::valid_plugin_tail` — accept single-segment
    // (`@lazuli/plugin-<name>`) as well as multi-segment (`@lazuli/plugin-<publisher>/<name>`).
    // All currently-shipped Lazuli plugins use the single-segment convention.
    let segments: Vec<&str> = value.split('/').filter(|p| !p.is_empty()).collect();
    !segments.is_empty() && segments.iter().all(|s| valid_path_segment(s))
}

pub(crate) fn valid_pathish_tail(value: &str) -> bool {
    !value.is_empty() && value.split('/').all(valid_path_segment)
}

pub(crate) fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
}

pub(crate) fn validate_app_integration_credential_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let mut parts = trimmed.split_whitespace();
    let Some(name) = parts.next() else {
        return;
    };
    let source = parts.collect::<Vec<_>>().join(" ");
    if !is_identifier(name) || source.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-integration-contract",
            "integration credentials use `<credential_name> <source>`, for example `access_token env.MERCADOPAGO_ACCESS_TOKEN`.",
        ));
    }
}
