//! Diagnostics for the `profile` contract family.
//!
//! Profiles (`profile <environment_name>` top-level blocks) declare
//! environment-specific overrides of `urls`, `bindings`, `integrations`,
//! and `deploy` settings. This module owns the file-local shape check
//! on that surface.
//!
//! ## Producer
//!
//! [`profile_contract_diagnostics`] is the single entry-point dispatched
//! from `crate::dispatch`. Sub-helpers stay pub(crate) so any future
//! cross-catalog caller keeps its `crate::*` path. The dispatcher calls
//! `adapter_source_provenance` (still in lib.rs root, app-cluster
//! adjacent) via `crate::*`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    adapter_source_provenance, is_identifier, is_quoted_lzx_literal, leading_spaces,
    simple_canonical_diagnostic,
};

pub(crate) fn profile_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_profile = false;
    let mut current_child: Option<&str> = None;
    let mut saw_child = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 0 {
            if in_profile && !saw_child {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index.saturating_sub(1),
                    "profile",
                    DiagnosticSeverity::WARNING,
                    "profile-contract",
                    "profiles should declare at least one `urls`, `bindings`, `integrations`, or `deploy` override.",
                ));
            }

            in_profile = trimmed.starts_with("profile ");
            current_child = None;
            saw_child = false;

            if in_profile {
                match trimmed.split_whitespace().collect::<Vec<_>>().as_slice() {
                    ["profile", name] if is_identifier(name) => {}
                    _ => diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "profile-contract",
                        "profile headers use `profile <environment_name>`.",
                    )),
                }
            }
            continue;
        }

        if !in_profile {
            continue;
        }

        match leading {
            2 => {
                current_child = profile_child_kind(trimmed);
                if current_child.is_some() {
                    saw_child = true;
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "profile-contract",
                        "profile blocks support `urls`, `bindings`, `integrations`, and `deploy` children.",
                    ));
                }
            }
            4 => match current_child {
                Some("urls") => validate_profile_url_line(&mut diagnostics, line_index, line),
                Some("bindings") => {
                    if !is_profile_binding_line(trimmed) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "profile-binding-contract",
                            "profile bindings use `<feature>.<slot> = integrations.<name>` or `registry.integrations.<name>`.",
                        ));
                    }
                }
                Some("integrations") => {
                    validate_profile_integration_line(&mut diagnostics, line_index, line)
                }
                Some("deploy") => validate_profile_deploy_line(&mut diagnostics, line_index, line),
                _ => diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "profile-contract",
                    "four-space profile declarations must belong to `urls`, `bindings`, `integrations`, or `deploy`.",
                )),
            },
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "profile-contract",
                "profile declarations use two-space sections and four-space override lines.",
            )),
        }
    }

    if in_profile && !saw_child {
        diagnostics.push(simple_canonical_diagnostic(
            source.lines().count().saturating_sub(1),
            "profile",
            DiagnosticSeverity::WARNING,
            "profile-contract",
            "profiles should declare at least one `urls`, `bindings`, `integrations`, or `deploy` override.",
        ));
    }

    diagnostics
}

pub(crate) fn profile_child_kind(trimmed: &str) -> Option<&'static str> {
    match trimmed {
        "urls" => Some("urls"),
        "bindings" => Some("bindings"),
        "integrations" => Some("integrations"),
        "deploy" => Some("deploy"),
        _ => None,
    }
}

pub(crate) fn validate_profile_url_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if matches!(parts.as_slice(), [target, url] if is_identifier(target) && is_quoted_lzx_literal(url))
    {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "profile-url-contract",
        "profile URL overrides use `<target> \"https://...\"`, e.g. `web \"https://app.example\"`.",
    ));
}

pub(crate) fn is_profile_binding_line(trimmed: &str) -> bool {
    let Some((target, source)) = trimmed.split_once('=') else {
        return false;
    };
    let Some((feature, slot)) = target.trim().split_once('.') else {
        return false;
    };
    is_identifier(feature)
        && is_identifier(slot)
        && (source
            .trim()
            .strip_prefix("integrations.")
            .is_some_and(is_identifier)
            || source
                .trim()
                .strip_prefix("registry.integrations.")
                .is_some_and(is_identifier))
}

pub(crate) fn validate_profile_integration_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if matches!(parts.as_slice(), [name, "environment", environment] if is_identifier(name) && is_identifier(environment))
        || matches!(parts.as_slice(), [name, "adapter", adapter] if is_identifier(name) && adapter_source_provenance(adapter).is_some())
    {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "profile-integration-contract",
        "profile integration overrides use `<integration> environment sandbox|production` or `<integration> adapter <source>`, where adapter sources are `@runtime/...`, `@lazuli/plugin-publisher/name`, `@adapter.local`, or a local path.",
    ));
}

pub(crate) fn validate_profile_deploy_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    match parts.as_slice() {
        ["topology", value]
        | ["migrations", value]
        | ["migration_lock", value]
        | ["destructive_migrations", value]
        | ["rollback", value]
            if is_identifier(value) =>
        {
            return;
        }
        _ => {}
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "profile-deploy-contract",
        "profile deploy overrides use `topology`, `migrations`, `migration_lock`, `destructive_migrations`, or `rollback` with an identifier value.",
    ));
}
