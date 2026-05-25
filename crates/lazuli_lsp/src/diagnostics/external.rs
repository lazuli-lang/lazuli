//! Diagnostics for the `contract` family (external contracts).
//!
//! External contracts (`contract <namespace.version>` in `contracts/*.lzi`)
//! declare the public shape of a service: imports, records, operations,
//! and events. This module owns the file-local shape checks on that
//! surface plus the small contract-token validators
//! ([`is_contract_name`], [`is_contract_type_token`]) the dispatcher
//! consumes.
//!
//! ## Producer
//!
//! [`external_contract_diagnostics`] is the single entry-point dispatched
//! from `crate::dispatch`. All sub-helpers stay pub(crate) and ride the
//! `pub(crate) use diagnostics::external::*;` re-export so existing
//! `crate::*` paths used by neighbouring catalog modules and
//! `dispatch.rs` keep resolving. Strictly additive ABI.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    is_identifier, is_quoted_lzx_literal, is_type_name, leading_spaces, named_block_name,
    simple_canonical_diagnostic,
};

pub(crate) fn external_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_contract = false;
    let mut current_child: Option<&'static str> = None;
    let mut in_event_payload = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            in_contract = trimmed.starts_with("contract ");
            current_child = None;
            in_event_payload = false;
            if in_contract {
                let parts: Vec<_> = trimmed.split_whitespace().collect();
                if parts.len() != 2 || !is_contract_name(parts[1]) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "contract-header",
                        "external contracts use `contract <namespace.version>`, e.g. `contract acme.ai.v1`.",
                    ));
                }
            }
            continue;
        }

        if !in_contract {
            continue;
        }

        match leading {
            2 => {
                current_child = None;
                in_event_payload = false;
                if let Some(rest) = trimmed.strip_prefix("purpose ") {
                    if !is_quoted_lzx_literal(rest.trim()) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-purpose",
                            "contract purpose uses a quoted sentence.",
                        ));
                    }
                } else if let Some(rest) = trimmed.strip_prefix("compatibility ") {
                    if !matches!(rest.trim(), "backward" | "none" | "manual") {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-compatibility",
                            "contract compatibility uses `backward`, `none`, or `manual`.",
                        ));
                    }
                } else if trimmed.starts_with("import ") {
                    validate_contract_import_line(&mut diagnostics, line_index, line, trimmed);
                } else if let Some(name) = named_block_name(trimmed, "record") {
                    current_child = Some("record");
                    if !is_type_name(name) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-record",
                            "contract records use `record <TypeName>`.",
                        ));
                    }
                } else if let Some(name) = named_block_name(trimmed, "operation") {
                    current_child = Some("operation");
                    if !is_identifier(name) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-operation",
                            "contract operations use `operation <name>`.",
                        ));
                    }
                } else if let Some(name) = named_block_name(trimmed, "event") {
                    current_child = Some("event");
                    if !is_identifier(name) {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-event",
                            "contract events use `event <name>`.",
                        ));
                    }
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "contract-shape",
                        "contract blocks use `purpose`, `compatibility`, `import`, `record`, `operation`, and `event` children.",
                    ));
                }
            }
            4 => match current_child {
                Some("record") => {
                    validate_contract_field_line(&mut diagnostics, line_index, line)
                }
                Some("operation") => {
                    validate_contract_operation_line(&mut diagnostics, line_index, line, trimmed)
                }
                Some("event") => {
                    if let Some(rest) = trimmed.strip_prefix("topic ") {
                        in_event_payload = false;
                        if !is_quoted_lzx_literal(rest.trim()) {
                            diagnostics.push(simple_canonical_diagnostic(
                                line_index,
                                line,
                                DiagnosticSeverity::WARNING,
                                "contract-event-topic",
                                "contract event topics use `topic \"event.name\"`.",
                            ));
                        }
                    } else if trimmed == "payload" {
                        in_event_payload = true;
                    } else {
                        diagnostics.push(simple_canonical_diagnostic(
                            line_index,
                            line,
                            DiagnosticSeverity::WARNING,
                            "contract-event",
                            "contract event children use `topic \"...\"` or `payload`.",
                        ));
                    }
                }
                _ => diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "contract-shape",
                    "four-space contract declarations must belong to `record`, `operation`, or `event` blocks.",
                )),
            },
            6 => {
                if current_child == Some("event") && in_event_payload {
                    validate_contract_field_line(&mut diagnostics, line_index, line);
                } else {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "contract-shape",
                        "six-space contract declarations are only valid inside event `payload`.",
                    ));
                }
            }
            _ => diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "contract-shape",
                "contract declarations use two, four, or six spaces of indentation.",
            )),
        }
    }

    diagnostics
}

pub(crate) fn validate_contract_import_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    if !matches!(
        parts.as_slice(),
        ["import", format, source]
            if matches!(*format, "openapi" | "asyncapi" | "proto" | "json_schema" | "avro")
                && is_quoted_lzx_literal(source)
    ) {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "contract-import",
            "contract imports use `import openapi|asyncapi|proto|json_schema|avro \"./path\"`.",
        ));
    }
}

pub(crate) fn validate_contract_operation_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    let parts: Vec<_> = trimmed.split_whitespace().collect();
    let valid = matches!(parts.as_slice(), ["transport", value] if matches!(*value, "http" | "rpc" | "event"))
        || matches!(parts.as_slice(), ["method", value] if matches!(*value, "GET" | "POST" | "PUT" | "PATCH" | "DELETE"))
        || matches!(parts.as_slice(), ["path", value] if is_quoted_lzx_literal(value))
        || matches!(parts.as_slice(), ["input", value] if is_type_name(value))
        || matches!(parts.as_slice(), ["output", value] if is_type_name(value))
        || matches!(parts.as_slice(), ["output", "stream", value] if is_type_name(value))
        || matches!(parts.as_slice(), ["auth", value] if matches!(*value, "service" | "none" | "propagate"))
        || matches!(parts.as_slice(), ["timeout", value] if is_quoted_lzx_literal(value))
        || is_contract_operation_retry(&parts)
        || is_contract_operation_idempotency(&parts)
        || is_contract_operation_error(&parts);

    if !valid {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "contract-operation",
            "operation children use `transport http|rpc|event`, `method GET|POST|PUT|PATCH|DELETE`, `path \"...\"`, `input Type`, `output [stream] Type`, `auth service|none|propagate`, `timeout \"...\"`, `retry <n> [backoff <strategy>]`, `idempotency by <field>[, <field>...]`, or `error <Name> status <code> [expose <field>...]`.",
        ));
    }
}

pub(crate) fn is_contract_operation_retry(parts: &[&str]) -> bool {
    if parts.first().copied() != Some("retry") {
        return false;
    }
    match parts.len() {
        2 => parts[1].parse::<u32>().is_ok(),
        4 => {
            parts[1].parse::<u32>().is_ok()
                && parts[2] == "backoff"
                && matches!(parts[3], "exponential" | "linear" | "fixed")
        }
        _ => false,
    }
}

pub(crate) fn is_contract_operation_idempotency(parts: &[&str]) -> bool {
    parts.len() >= 3
        && parts[0] == "idempotency"
        && parts[1] == "by"
        && parts.iter().skip(2).all(|t| !t.is_empty())
}

pub(crate) fn is_contract_operation_error(parts: &[&str]) -> bool {
    if parts.first().copied() != Some("error") {
        return false;
    }
    if parts.len() < 2 || !is_type_name(parts[1]) {
        return false;
    }
    // Allow `error <Name>` alone, or with optional `status <code>` and
    // `expose <field>...` clauses in any order.
    let mut iter = parts.iter().skip(2);
    while let Some(token) = iter.next() {
        match *token {
            "status" => {
                let Some(value) = iter.next() else {
                    return false;
                };
                if value.parse::<u16>().is_err() {
                    return false;
                }
            }
            "expose" => {
                if iter.next().is_none() {
                    return false;
                }
                // Consume the rest as expose fields.
                while iter.next().is_some() {}
                return true;
            }
            _ => return false,
        }
    }
    true
}

pub(crate) fn validate_contract_field_line(diagnostics: &mut Vec<Diagnostic>, line_index: usize, line: &str) {
    let trimmed = line.trim_start();
    let Some((name, rest)) = trimmed.split_once(':') else {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "contract-field",
            "contract fields use `<name>: <Type> required|optional`.",
        ));
        return;
    };

    let parts: Vec<_> = rest.split_whitespace().collect();
    if !is_identifier(name.trim())
        || parts.len() < 2
        || !is_contract_type_token(parts[0])
        || !parts
            .last()
            .is_some_and(|last| matches!(*last, "required" | "optional"))
        || parts[1..parts.len() - 1]
            .iter()
            .any(|part| !part.starts_with('@'))
    {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::WARNING,
            "contract-field",
            "contract fields use `<name>: <Type> [@pii.* ...] required|optional`.",
        ));
    }
}

pub(crate) fn is_contract_name(value: &str) -> bool {
    let mut parts = value.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    is_identifier(first)
        && parts.all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

pub(crate) fn is_contract_type_token(value: &str) -> bool {
    value.starts_with("@semantic.")
        || value.starts_with("@cap.")
        || is_type_name(value)
        || matches!(
            value,
            "ID" | "Text" | "Integer" | "Decimal" | "Float" | "Boolean" | "DateTime" | "Date"
        )
}
