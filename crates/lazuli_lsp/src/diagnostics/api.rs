//! Diagnostics for the `api <name>` declaration.
//!
//! The `api` primitive expresses a custom HTTP endpoint with explicit
//! authorization, generated clients, and a handler boundary. Each block
//! is required to declare `method`, `path`, `output`, `policy`, and
//! `handler`. Method must be one of GET/POST/PUT/PATCH/DELETE. Paths
//! must be absolute. Every `:slot` segment in `path` should be backed
//! by a typed `route <name>: <Type>` declaration so generated handlers
//! and clients stay type-safe.
//!
//! Two-pass design: [`api_contract_diagnostics`] walks every line and
//! flushes one [`ApiContractFacts`] into [`api_facts_diagnostics`] per
//! `api` block.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    leading_spaces, lzx_declared_path_params, route_slot_name, simple_canonical_diagnostic,
    unquote_lzx_literal,
};

#[derive(Debug)]
pub(crate) struct ApiContractFacts {
    line_index: usize,
    line: String,
    has_method: bool,
    has_path: bool,
    has_output: bool,
    has_policy: bool,
    has_handler: bool,
    routes: HashSet<String>,
    path_params: Vec<String>,
}

pub(crate) fn api_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_api: Option<ApiContractFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 2 && trimmed.starts_with("api ") {
            if let Some(api) = current_api.take() {
                diagnostics.extend(api_facts_diagnostics(api));
            }
            current_api = Some(ApiContractFacts {
                line_index,
                line: line.to_owned(),
                has_method: false,
                has_path: false,
                has_output: false,
                has_policy: false,
                has_handler: false,
                routes: HashSet::new(),
                path_params: Vec::new(),
            });
            continue;
        }

        if leading_spaces(line) <= 2 {
            if let Some(api) = current_api.take() {
                diagnostics.extend(api_facts_diagnostics(api));
            }
            continue;
        }

        let Some(api) = current_api.as_mut() else {
            continue;
        };

        if leading_spaces(line) != 4 {
            continue;
        }

        if let Some(method) = trimmed.strip_prefix("method ") {
            api.has_method = true;
            if !matches!(method.trim(), "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "api-contract",
                    "api methods should be one of `GET`, `POST`, `PUT`, `PATCH`, or `DELETE`.",
                ));
            }
        } else if let Some(path) = trimmed.strip_prefix("path ") {
            api.has_path = true;
            let path = unquote_lzx_literal(path.trim());
            if !path.starts_with('/') {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "api-contract",
                    "api paths should be absolute and start with `/`.",
                ));
            }
            api.path_params.extend(lzx_declared_path_params(path));
        } else if let Some(route) = trimmed.strip_prefix("route ") {
            if let Some(name) = route_slot_name(route) {
                api.routes.insert(name.to_owned());
            }
        } else if let Some(output) = trimmed.strip_prefix("output ") {
            api.has_output = true;
            if output.trim_start().starts_with("stream ") {
                let stream_type = output.trim_start().trim_start_matches("stream ").trim();
                if stream_type.is_empty() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "api-contract",
                        "streaming APIs use `output stream <Type>` so generated clients know the stream item shape.",
                    ));
                }
            }
        } else if trimmed.starts_with("policy ") {
            api.has_policy = true;
        } else if trimmed.starts_with("handler ") {
            api.has_handler = true;
        }
    }

    if let Some(api) = current_api {
        diagnostics.extend(api_facts_diagnostics(api));
    }

    diagnostics
}

pub(crate) fn api_facts_diagnostics(api: ApiContractFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut missing = Vec::new();

    if !api.has_method {
        missing.push("method");
    }
    if !api.has_path {
        missing.push("path");
    }
    if !api.has_output {
        missing.push("output");
    }
    if !api.has_policy {
        missing.push("policy");
    }
    if !api.has_handler {
        missing.push("handler");
    }

    if !missing.is_empty() {
        diagnostics.push(simple_canonical_diagnostic(
            api.line_index,
            &api.line,
            DiagnosticSeverity::ERROR,
            "api-contract",
            &format!(
                "custom APIs should declare {} so HTTP shape, authorization, generated clients, and handler boundaries are explicit.",
                missing.join(", ")
            ),
        ));
    }

    for path_param in api.path_params {
        if !api.routes.contains(&path_param) {
            diagnostics.push(simple_canonical_diagnostic(
                api.line_index,
                &api.line,
                DiagnosticSeverity::WARNING,
                "api-route-contract",
                &format!(
                    "api path parameter `{path_param}` should be declared with `route {path_param}: <Type>` so generated handlers and clients are type-safe.",
                ),
            ));
        }
    }

    diagnostics
}
