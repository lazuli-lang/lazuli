//! Target / URL / binding / pack line validators for `app`.
//!
//! `targets`, `urls`, `bindings`, and `packs` are flat single-line
//! enumerations under the app block. Each line is independent — no
//! facts accumulate. This module owns the per-line shape check for
//! all four, plus the `registry packs` header/child counterparts used
//! by `diagnostics/registry.rs`.
//!
//! The grammar is intentionally narrow:
//!
//! * `targets`  — `<backend|web|mobile> <runtime>`
//! * `urls`     — `<web|api|mobile> <environment> "<url>"`
//! * `bindings` — `<feature>.<slot> = integrations.<name>`
//! * `packs`    — `<alias> from registry.packs.<name>`
//!
//! Provider mechanics (HTTP transport, broker plumbing, SDK selection)
//! stay in adapters; this module only verifies the surface shape.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    is_identifier, is_quoted_lzx_literal, parse_feature_integration_requirement,
    simple_canonical_diagnostic, unquote_lzx_literal,
};

pub(crate) fn validate_app_target_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 2 || !matches!(parts[0], "backend" | "web" | "mobile") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "app-target-contract",
            "app targets use `backend <runtime>`, `web <runtime>`, or `mobile <runtime>`.",
        ));
    }
}

pub(crate) fn validate_app_url_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if parts.len() != 3 || !matches!(parts[0], "web" | "api" | "mobile") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-url-contract",
            "app URLs use `<web|api|mobile> <environment> \"https://...\"`.",
        ));
        return;
    }

    let url = unquote_lzx_literal(parts[2]);
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-url-contract",
            "app URLs should be absolute HTTP(S) URLs so generated clients, CORS, emails, and callbacks agree.",
        ));
    }

    if parts[1] != "local" && url.starts_with("http://") {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-url-contract",
            "non-local app URLs should use HTTPS.",
        ));
    }
}

pub(crate) fn validate_app_binding_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    if parse_app_binding_line(trimmed).is_none() {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-binding-contract",
            "app bindings use `<feature>.<slot> = integrations.<name>` or `<feature>.<slot> = registry.integrations.<name>`.",
        ));
    }
}

pub(crate) fn validate_app_pack_use_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let Some((name, source)) = trimmed.split_once(" from ") else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-pack-contract",
            "app pack entries use `<alias> from registry.packs.<name>` or `<alias> from packs.<name>`.",
        ));
        return;
    };

    let source_name = source
        .trim()
        .strip_prefix("packs.")
        .or_else(|| source.trim().strip_prefix("registry.packs."));
    if !is_identifier(name.trim()) || !source_name.is_some_and(is_identifier) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "app-pack-contract",
            "app pack entries use identifier aliases and `packs.<name>` or `registry.packs.<name>` sources.",
        ));
    }
}

pub(crate) fn validate_registry_pack_header(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let Some((name, source)) = trimmed.split_once(" from ") else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "registry-pack-contract",
            "registry packs use `<name> from @scope/package` or a local path.",
        ));
        return;
    };

    let source = source.trim();
    let valid_source = source.starts_with('@')
        || source.starts_with("./")
        || source.starts_with("../")
        || source.starts_with("http://")
        || source.starts_with("https://")
        || is_quoted_lzx_literal(source);

    if !is_identifier(name.trim()) || !valid_source {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "registry-pack-contract",
            "registry pack entries use identifier names and package/path sources such as `payments from @runtime/payments`.",
        ));
    }
}

pub(crate) fn validate_registry_pack_child(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if let Some(version) = trimmed.strip_prefix("version ") {
        if is_quoted_lzx_literal(version.trim()) {
            return;
        }
    }

    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if matches!(parts.as_slice(), ["provides", kind, name] if is_identifier(kind) && is_identifier(name))
    {
        return;
    }

    if let Some(requirement) = trimmed.strip_prefix("requires ")
        && parse_feature_integration_requirement(requirement).is_some()
    {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "registry-pack-contract",
        "pack children use `version \"...\"`, `provides feature <name>`, or `requires integration <slot>: <CapabilityType>`.",
    ));
}

pub(crate) fn parse_app_binding_line(trimmed: &str) -> Option<(&str, &str, &str)> {
    let (target, source) = trimmed.split_once('=')?;
    let target = target.trim();
    let source = source.trim();
    let (feature, slot) = target.split_once('.')?;
    let source_name = source
        .strip_prefix("integrations.")
        .or_else(|| source.strip_prefix("registry.integrations."))?;

    if is_identifier(feature) && is_identifier(slot) && is_identifier(source_name) {
        Some((feature, slot, source_name))
    } else {
        None
    }
}
