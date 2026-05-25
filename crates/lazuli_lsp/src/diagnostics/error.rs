//! Diagnostics for `errors` blocks — both feature-level (`errors`
//! under a feature) and command-level (`errors` under a command), plus
//! `error <Name> status <code> expose ...` cases.
//!
//! The catalog enforces three things:
//!
//! - Feature-level `default hide|expose` is a closed catalog.
//! - Every error case has the shape `error <Name> status <http-status>
//!   expose message, code, data`.
//! - Exposure fields are limited to `message`, `code`, `data`.
//!
//! Contract-operation `error` cases inside a top-level `contract` block
//! follow a different shape (schema-defined fields) — those are
//! validated by `validate_contract_operation_line` in lib.rs and not
//! by this catalog.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

pub(crate) fn error_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_feature_errors = false;
    let mut in_command_errors = false;
    let mut in_contract = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 0 {
            in_contract = trimmed.starts_with("contract ");
        }

        if leading_spaces(line) <= 2 {
            in_feature_errors = leading_spaces(line) == 2 && trimmed == "errors";
            in_command_errors = false;
            continue;
        }

        if leading_spaces(line) == 4 && trimmed == "errors" {
            in_command_errors = true;
            continue;
        }

        if leading_spaces(line) <= 4 && in_command_errors {
            in_command_errors = false;
        }

        if in_feature_errors && leading_spaces(line) == 4 {
            if let Some(mode) = trimmed.strip_prefix("default ") {
                if !matches!(mode.trim(), "hide" | "expose") {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "error-contract",
                        "feature error defaults use `default hide` or `default expose`.",
                    ));
                }
            } else if trimmed.contains(" message @translation.") {
                // IR Error-Vocab (Cell PARSE-1) — `<code> message
                // @translation.<key>` is the new feature-level catch-all
                // form. Closed-catalog enforcement lives in doctor
                // (ERR-VOCAB-CODE-UNKNOWN). The LSP-side shape check
                // intentionally accepts the line so the new surface
                // round-trips clean through `lazuli check`.
            } else if !valid_error_exposure_line(trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "error-contract",
                    "error exposure uses `expose client 4xx|5xx message, code, data` so public/private error payloads are explicit.",
                ));
            }
        }

        // Inside a top-level `contract <name>` block, `error` cases on
        // operations expose schema-defined fields, not the
        // command-level `message|code|data` envelope. The
        // contract-operation validator handles the shape; skip the
        // command-level rules here.
        if in_contract {
            continue;
        }

        if trimmed.starts_with("error ") {
            if let Some(message) = error_case_contract_error(trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "error-contract",
                    message,
                ));
            }
        }

        if in_command_errors && leading_spaces(line) == 6 {
            let candidate = format!("error {trimmed}");
            if let Some(message) = error_case_contract_error(&candidate) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "error-contract",
                    message,
                ));
            }
        }
    }

    diagnostics
}

pub(crate) fn valid_error_exposure_line(line: &str) -> bool {
    let parts: Vec<_> = line.split_whitespace().collect();
    parts.len() >= 4
        && parts[0] == "expose"
        && parts[1] == "client"
        && matches!(parts[2], "4xx" | "5xx")
        && error_exposure_fields_valid(parts[3..].join(" ").as_str())
}

pub(crate) fn error_case_contract_error(line: &str) -> Option<&'static str> {
    let parts: Vec<_> = line.split_whitespace().collect();
    if parts.len() < 6 || parts[0] != "error" || parts[2] != "status" || parts[4] != "expose" {
        return Some(
            "error cases use `error <Name> status <http-status> expose message, code, data`.",
        );
    }

    if !is_http_status_code(parts[3]) {
        return Some("error status should be an HTTP status code from 100 to 599.");
    }

    if !error_exposure_fields_valid(parts[5..].join(" ").as_str()) {
        return Some("error exposure fields are limited to `message`, `code`, and `data`.");
    }

    None
}

pub(crate) fn error_exposure_fields_valid(fields: &str) -> bool {
    fields
        .split(',')
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .all(|field| matches!(field, "message" | "code" | "data"))
}

pub(crate) fn is_http_status_code(value: &str) -> bool {
    matches!(value.parse::<u16>(), Ok(status) if (100..=599).contains(&status))
}
