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

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    command_name_if, is_identifier, is_quoted_lzx_literal, is_type_name, leading_spaces,
    named_block_name, simple_canonical_diagnostic,
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

pub(crate) fn validate_contract_field_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
) {
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

// ── feature_requirements + external_call (contract sibling family) ────────

pub(crate) fn feature_requirements_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_feature = false;
    let mut in_requires_block = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let leading = leading_spaces(line);
        if leading == 0 {
            in_feature = trimmed.starts_with("feature ");
            in_requires_block = false;
            continue;
        }

        if !in_feature {
            continue;
        }

        if leading == 2 {
            in_requires_block = trimmed == "requires";
            if let Some(requirement) = trimmed.strip_prefix("requires ") {
                validate_feature_requirement_line(&mut diagnostics, line_index, line, requirement);
            }
            continue;
        }

        if leading <= 2 {
            in_requires_block = false;
        }

        if in_requires_block && leading == 4 {
            validate_feature_requirement_line(&mut diagnostics, line_index, line, trimmed);
        } else if in_requires_block && leading > 4 {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "feature-requirement-contract",
                "feature requirements use four-space children such as `integration gateway: PaymentGateway`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn validate_feature_requirement_line(
    diagnostics: &mut Vec<Diagnostic>,
    line_index: usize,
    line: &str,
    trimmed: &str,
) {
    if parse_feature_integration_requirement(trimmed).is_some() {
        return;
    }

    diagnostics.push(simple_canonical_diagnostic(
        line_index,
        line,
        DiagnosticSeverity::WARNING,
        "feature-requirement-contract",
        "feature requirements currently use `integration <name>: <CapabilityType>`; bind concrete providers from app/registry.",
    ));
}

pub(crate) fn parse_feature_integration_requirement(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.trim().strip_prefix("integration ")?;
    let (name, contract) = rest.split_once(':')?;
    let name = name.trim();
    let contract = contract.trim();

    if is_identifier(name) && is_type_name(contract) {
        Some((name, contract))
    } else {
        None
    }
}

pub(crate) fn external_call_contract_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_feature = false;
    let mut requirement_slots = HashSet::new();
    let mut current_block: Option<ExternalCallBlockFacts> = None;
    let mut current_call_child = false;
    let mut in_requires_block = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading == 0 {
            if let Some(block) = current_block.take() {
                diagnostics.extend(external_call_block_diagnostics(block));
            }
            in_feature = trimmed.starts_with("feature ");
            requirement_slots.clear();
            current_call_child = false;
            in_requires_block = false;
            continue;
        }

        if !in_feature {
            continue;
        }

        if leading == 2 {
            if let Some(block) = current_block.take() {
                diagnostics.extend(external_call_block_diagnostics(block));
            }

            in_requires_block = trimmed == "requires";
            current_call_child = false;

            if let Some(requirement) = trimmed.strip_prefix("requires ")
                && let Some((slot, _)) = parse_feature_integration_requirement(requirement)
            {
                requirement_slots.insert(slot.to_owned());
            }

            if let Some(name) = command_name_if(trimmed) {
                current_block = Some(ExternalCallBlockFacts::new("command", name, line_index));
            } else if let Some(name) = named_block_name(trimmed, "job") {
                current_block = Some(ExternalCallBlockFacts::new(
                    "job",
                    name.to_owned(),
                    line_index,
                ));
            }
            continue;
        }

        if in_requires_block && leading == 4 {
            if let Some((slot, _)) = parse_feature_integration_requirement(trimmed) {
                requirement_slots.insert(slot.to_owned());
            }
            continue;
        } else if leading <= 2 {
            in_requires_block = false;
        }

        let Some(block) = current_block.as_mut() else {
            continue;
        };

        if leading == 4 {
            current_call_child = false;
            if trimmed.starts_with("timeout ") {
                block.has_timeout = true;
            } else if trimmed.starts_with("retry ") {
                block.has_retry = true;
            } else if trimmed.starts_with("idempotency by ") {
                block.has_idempotency = true;
            } else if let Some((slot, _operation)) = parse_external_call_header(trimmed) {
                block.calls.push(ExternalCallLine {
                    line_index,
                    line: line.to_owned(),
                });
                if !requirement_slots.contains(slot) {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::WARNING,
                        "external-call-requirement",
                        "`calls <slot>.<operation>` should use a slot declared by `requires integration <slot>: <Contract>`.",
                    ));
                }
                current_call_child = true;
            } else if trimmed.starts_with("calls ") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "external-call-shape",
                    "external calls use `calls <integration_slot>.<operation>`.",
                ));
                current_call_child = true;
            }
        } else if leading == 6 && current_call_child && !trimmed.contains('=') {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "external-call-arg",
                "external call children use `name = expression` argument bindings.",
            ));
        }
    }

    if let Some(block) = current_block {
        diagnostics.extend(external_call_block_diagnostics(block));
    }

    diagnostics
}

#[derive(Debug)]
pub(crate) struct ExternalCallBlockFacts {
    kind: &'static str,
    name: String,
    line_index: usize,
    calls: Vec<ExternalCallLine>,
    has_timeout: bool,
    has_retry: bool,
    has_idempotency: bool,
}

impl ExternalCallBlockFacts {
    fn new(kind: &'static str, name: String, line_index: usize) -> Self {
        Self {
            kind,
            name,
            line_index,
            calls: Vec::new(),
            has_timeout: false,
            has_retry: false,
            has_idempotency: false,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ExternalCallLine {
    line_index: usize,
    line: String,
}

pub(crate) fn external_call_block_diagnostics(block: ExternalCallBlockFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if block.calls.is_empty() {
        return diagnostics;
    }

    if !block.has_timeout {
        diagnostics.push(simple_canonical_diagnostic(
            block.line_index,
            &format!("{} {}", block.kind, block.name),
            DiagnosticSeverity::WARNING,
            "external-call-timeout",
            "`calls <slot>.<operation>` should be paired with an explicit `timeout \"...\"` on the same command/job block.",
        ));
    }

    if !block.has_retry {
        for call in &block.calls {
            diagnostics.push(simple_canonical_diagnostic(
                call.line_index,
                &call.line,
                DiagnosticSeverity::WARNING,
                "external-call-retry",
                "`calls <slot>.<operation>` should have a visible `retry <count> backoff <strategy>` policy or a future explicit no-retry marker.",
            ));
        }
    }

    if block.kind == "job" && !block.has_idempotency {
        for call in &block.calls {
            diagnostics.push(simple_canonical_diagnostic(
                call.line_index,
                &call.line,
                DiagnosticSeverity::WARNING,
                "external-call-idempotency",
                "jobs with external calls should declare `idempotency by ...` so retries cannot duplicate side effects silently.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn parse_external_call_header(trimmed: &str) -> Option<(&str, &str)> {
    let rest = trimmed.strip_prefix("calls ")?;
    let (slot, operation) = rest.trim().split_once('.')?;
    let slot = slot.trim();
    let operation = operation.trim();

    if is_identifier(slot) && is_identifier(operation) {
        Some((slot, operation))
    } else {
        None
    }
}
