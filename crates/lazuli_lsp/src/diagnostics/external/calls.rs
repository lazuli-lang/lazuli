//! `calls <slot>.<operation>` shape + reliability surface for
//! commands and jobs that issue external calls.
//!
//! Checks that the slot resolves to a `requires integration` slot in
//! the enclosing feature, that an explicit `timeout` and `retry` are
//! present, and that jobs add `idempotency by ...`.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{
    command_name_if, is_identifier, leading_spaces, named_block_name, simple_canonical_diagnostic,
};

use super::requirements::parse_feature_integration_requirement;

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
