//! Diagnostics for the `env` schema family.
//!
//! Environment variables are declared in `registry.env` (the canonical
//! home) with scope (`server` / `client` / `mobile`), type, and
//! requiredness. This module enforces:
//!
//! * [`env_top_level_legacy_diagnostics`] — warns when a `.lzi` source
//!   has both a top-level `env` block and a `feature`/`app` block but
//!   no `registry`, meaning the env declaration is being duplicated
//!   outside its canonical home.
//! * [`env_schema_diagnostics`] — file-local shape check on each
//!   `env` block (indent + `server|client|mobile NAME: Type required`
//!   form) plus client / mobile naming conventions
//!   (`PUBLIC` token, `EXPO_PUBLIC_` prefix) and `env.<NAME>` reference
//!   declared-ness against the same file.
//!
//! Both depend on shared helpers (`leading_spaces`, `parse_env_group_name`,
//! `valid_env_declaration_parts`, `has_public_token`, `path_references`,
//! `simple_canonical_diagnostic`) that stay pub(crate) elsewhere and are
//! pulled in via `use crate::*`.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    has_public_token, leading_spaces, parse_env_group_name, path_references,
    simple_canonical_diagnostic, valid_env_declaration_parts,
};

pub(crate) fn env_top_level_legacy_diagnostics(source: &str) -> Vec<Diagnostic> {
    // Warn when an `env` block lives at indent 0 in a `.lzi` source that
    // also declares `feature` or `app`. The canonical home for env schema
    // is `registry.lzi`; top-level `env` here is a legacy duplicate.
    let mut diagnostics = Vec::new();
    let mut env_at_top: Option<(usize, String)> = None;
    let mut has_feature_or_app = false;
    let mut has_registry = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) != 0 {
            continue;
        }

        if trimmed == "env" {
            env_at_top.get_or_insert((line_index, line.to_owned()));
        } else if trimmed.starts_with("feature ") || trimmed.starts_with("app ") {
            has_feature_or_app = true;
        } else if trimmed == "registry" || trimmed.starts_with("registry ") {
            has_registry = true;
        }
    }

    if let Some((line_index, line)) = env_at_top
        && has_feature_or_app
        && !has_registry
    {
        diagnostics.push(simple_canonical_diagnostic(
                line_index,
                &line,
                DiagnosticSeverity::WARNING,
                "env-top-level-legacy",
                "top-level `env` blocks in `.lzi` feature/app sources are legacy. Move env schema to `registry.lzi` (or `registry.env` inside the same package) so the declaration has a single source of truth.",
            ));
    }

    diagnostics
}

pub(crate) fn env_schema_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut declared = HashSet::new();
    let mut env_indent: Option<usize> = None;
    let mut current_env_group: Option<String> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            env_indent = if trimmed == "env" { Some(0) } else { None };
            current_env_group = None;
            continue;
        }

        if leading == 2 && trimmed == "env" {
            env_indent = Some(2);
            current_env_group = None;
            continue;
        }

        let Some(base_indent) = env_indent else {
            continue;
        };

        if leading <= base_indent {
            env_indent = None;
            current_env_group = None;
            continue;
        }

        if leading == base_indent + 2 {
            if let Some(group) = parse_env_group_name(trimmed) {
                current_env_group = Some(group.to_owned());
                continue;
            }
            current_env_group = None;
        } else if leading == base_indent + 4 && current_env_group.is_some() {
        } else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "env-schema-contract",
                "env declarations use `server|client|mobile NAME: Secret|Text|Url|Boolean|Integer required|optional [in environment]`, optionally nested under `group <name>`.",
            ));
            continue;
        }

        let parts: Vec<_> = trimmed.split_whitespace().collect();
        if !valid_env_declaration_parts(&parts) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "env-schema-contract",
                "env declarations use `server|client|mobile NAME: Secret|Text|Url|Boolean|Integer required|optional [in environment]`, optionally nested under `group <name>`.",
            ));
            continue;
        }

        let name = parts[1].trim_end_matches(':');
        declared.insert(name.to_owned());

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

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        for reference in path_references(trimmed, "env.") {
            if !declared.contains(reference) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "env-schema-reference",
                    &format!(
                        "environment reference `env.{reference}` should be declared in `registry.env` with scope, type, and requiredness. Doctor cross-checks against the package registry; this LSP rule only sees the current file.",
                    ),
                ));
            }
        }
    }

    diagnostics
}
