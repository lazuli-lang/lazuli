//! Diagnostics for the `registry` contract family.
//!
//! The top-level `registry` block is the project's external-world
//! catalogue: environment variables, declared capabilities, integration
//! slots and their child credentials, plugin packs, tools, and the
//! `webhook_event` envelope catalogue. This module owns the file-local
//! shape check on that block.
//!
//! ## Producer
//!
//! [`registry_contract_diagnostics`] is the single entry-point
//! dispatched from `crate::dispatch`. It calls the shared
//! `validate_app_*` / `validate_registry_pack_*` helpers (still in
//! `lib.rs` until the broader app cluster is extracted) via `crate::*`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    leading_spaces, parse_env_group_name, simple_canonical_diagnostic, validate_app_capability_line,
    validate_app_env_line, validate_app_integration_child, validate_app_integration_credential_line,
    validate_app_integration_header, validate_registry_pack_child, validate_registry_pack_header,
};

pub(crate) fn registry_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_registry = false;
    let mut current_child: Option<&'static str> = None;
    let mut current_env_group: Option<String> = None;
    let mut current_integration = false;
    let mut current_integration_child: Option<&'static str> = None;
    let mut current_pack = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            in_registry = trimmed == "registry";
            current_child = None;
            current_env_group = None;
            current_integration = false;
            current_integration_child = None;
            current_pack = false;
            continue;
        }

        if !in_registry {
            continue;
        }

        match leading {
            2 => {
                current_env_group = None;
                current_integration = false;
                current_integration_child = None;
                current_pack = false;
                current_child = match trimmed.split_whitespace().next().unwrap_or_default() {
                    "env" => Some("env"),
                    "capabilities" => Some("capabilities"),
                    "integrations" => Some("integrations"),
                    // B1 (W3-blockers) — `bindings` is registry-level
                    // sugar over `integrations`. Same indent-4
                    // integration header (`<name>: <CapabilityType>`)
                    // and the same canonical indent-6 children, plus
                    // the simplified `endpoint env.X` /
                    // `auth keys env.A env.B` surface. We reuse the
                    // `integrations` sentinel so the existing
                    // header + child validators apply unchanged.
                    "bindings" => Some("integrations"),
                    "packs" => Some("packs"),
                    "tools" => Some("tools"),
                    "webhook_event" => Some("webhook_event"),
                    // Webhooks expanded cycle — registry-side catalog
                    // of expected inbound envelope shapes. Indent-4
                    // entries open new envelopes; indent-6 children
                    // declare typed fields. Validation lives in the
                    // doctor path (`WEBHOOK-PAYLOAD-001` etc.); the
                    // LSP contract diagnostic only suppresses the
                    // unknown-block warning.
                    "webhook_events" => Some("webhook_events"),
                    // Roadmap §1.10 — `secret_rotation <name>` is
                    // a NAMED block at indent-2. Body shape
                    // validated by
                    // `secret_rotation_contract_diagnostics`.
                    "secret_rotation" => Some("secret_rotation"),
                    _ => {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "registry blocks use `env`, `capabilities`, `integrations`, `bindings`, `packs`, `tools`, `webhook_event <name>`, `webhook_events`, or `secret_rotation`.",
                        ));
                        None
                    }
                };
            }
            4 => match current_child {
                Some("env") => {
                    if let Some(group) = parse_env_group_name(trimmed) {
                        current_env_group = Some(group.to_owned());
                    } else {
                        current_env_group = None;
                        validate_app_env_line(&mut diagnostics, line_index, line);
                    }
                }
                Some("capabilities") => {
                    validate_app_capability_line(&mut diagnostics, line_index, line)
                }
                Some("integrations") => {
                    validate_app_integration_header(&mut diagnostics, line_index, line, trimmed);
                    current_integration = true;
                    current_integration_child = None;
                }
                Some("packs") => {
                    validate_registry_pack_header(&mut diagnostics, line_index, line, trimmed);
                    current_pack = true;
                }
                Some("webhook_event") => {
                    if !(trimmed == "payload"
                        || trimmed.starts_with("version ")
                        || trimmed.starts_with("previous_version ")
                        || trimmed.starts_with("deprecated "))
                    {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "`webhook_event` children use `payload`, `version <n>`, `previous_version <n>`, or `deprecated <bool>`.",
                        ));
                    }
                }
                _ => {}
            },
            6 => {
                if current_child == Some("env") {
                    if current_env_group.is_none() {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "six-space registry env declarations must follow `group <name>` inside `env`.",
                        ));
                    } else {
                        validate_app_env_line(&mut diagnostics, line_index, line);
                    }
                } else if current_child == Some("integrations") {
                    if !current_integration {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "integration children must follow `<name>: <CapabilityType>` under `integrations`.",
                        ));
                    } else {
                        validate_app_integration_child(
                            &mut diagnostics,
                            &mut current_integration_child,
                            line_index,
                            line,
                            trimmed,
                        );
                    }
                } else if current_child == Some("packs") {
                    if !current_pack {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-pack-contract",
                            "pack children must follow `<name> from <package-or-path>` under `packs`.",
                        ));
                    } else {
                        validate_registry_pack_child(&mut diagnostics, line_index, line, trimmed);
                    }
                } else if current_child == Some("webhook_event") {
                    if !trimmed.contains(':') {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "registry-contract",
                            "`webhook_event payload` fields use `<name>: <Type>`.",
                        ));
                    }
                }
            }
            8 => {
                if current_child == Some("integrations")
                    && current_integration_child == Some("credentials")
                {
                    validate_app_integration_credential_line(
                        &mut diagnostics,
                        line_index,
                        line,
                        trimmed,
                    );
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "registry-contract",
                        "eight-space registry declarations are only valid inside `integrations credentials`.",
                    ));
                }
            }
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "registry-contract",
                "registry declarations use two, four, six, or eight spaces of indentation.",
            )),
        }
    }

    diagnostics
}
