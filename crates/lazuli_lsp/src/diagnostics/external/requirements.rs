//! `feature.requires integration <slot>: <Contract>` shape check.
//!
//! Sibling family of `contract.rs` — this layer ensures every
//! feature-level integration requirement is recognised, and exposes
//! the `parse_feature_integration_requirement` parser used by
//! `external_call_contract_diagnostics` to know which slots are
//! declared.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, is_type_name, leading_spaces, simple_canonical_diagnostic};

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
