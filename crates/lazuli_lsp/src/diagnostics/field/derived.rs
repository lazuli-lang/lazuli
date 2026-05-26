//! `derived from` field-shape contract + `has_many` association shape.
//!
//! Derived fields are read-time computed; the parser rejects `required`,
//! `optional`, and default values for them. `has_many` associations
//! follow the shape `has_many <name>: <Type> [inverse <field>]`.
//!
//! Parser helpers (`split_derived_from`, `contains_top_level_eq`,
//! `field_typed_rhs`) live here too because every consumer is
//! field-shape adjacent.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::simple_canonical_diagnostic;

pub(crate) fn derived_field_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some(rest) = field_typed_rhs(trimmed) else {
            continue;
        };

        let (before_derived, after_derived) = match split_derived_from(rest) {
            Some(parts) => parts,
            None => continue,
        };

        if after_derived.trim().is_empty() {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "derived-field-contract",
                "`derived from` requires an expression: `<name>: <Type> derived from <expression>`.",
            ));
            continue;
        }

        let mut emitted_requiredness = false;
        for forbidden in ["required", "optional"] {
            if before_derived
                .split_whitespace()
                .any(|token| token == forbidden)
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "derived-field-contract",
                    "`derived from` fields are computed at read time and must not declare `required` or `optional`.",
                ));
                emitted_requiredness = true;
                break;
            }
        }

        if !emitted_requiredness && contains_top_level_eq(after_derived) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "derived-field-contract",
                "`derived from` fields are computed at read time and must not declare `default` (no trailing `= <value>`).",
            ));
        }
    }

    diagnostics
}

pub(crate) fn has_many_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("has_many ") else {
            continue;
        };

        let Some((name_part, type_part)) = rest.split_once(':') else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "has-many-contract",
                "`has_many` collections use `has_many <name>: <Type> [inverse <field>]`.",
            ));
            continue;
        };

        let name = name_part.trim();
        if name.is_empty() || name.contains(' ') {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "has-many-contract",
                "`has_many` requires a single identifier before `:`.",
            ));
            continue;
        }

        let mut tokens = type_part.split_whitespace();
        let Some(_type_token) = tokens.next() else {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::ERROR,
                "has-many-contract",
                "`has_many` requires a target type after `:`.",
            ));
            continue;
        };

        match tokens.next() {
            None => {}
            Some("inverse") => {
                if tokens.next().is_none() {
                    diagnostics.push(simple_canonical_diagnostic(
                        line_index,
                        line,
                        DiagnosticSeverity::ERROR,
                        "has-many-contract",
                        "`inverse` requires a field name on the target resource.",
                    ));
                }
            }
            Some(unexpected) => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::ERROR,
                    "has-many-contract",
                    &format!(
                        "unexpected `{unexpected}` after `has_many <name>: <Type>`. Only `inverse <field>` is allowed.",
                    ),
                ));
            }
        }
    }

    diagnostics
}

pub(crate) fn split_derived_from(rhs: &str) -> Option<(&str, &str)> {
    if let Some(pos) = rhs.find(" derived from ") {
        return Some((&rhs[..pos], &rhs[pos + " derived from ".len()..]));
    }
    if let Some(stripped) = rhs.strip_suffix(" derived from") {
        return Some((stripped, ""));
    }
    None
}

pub(crate) fn contains_top_level_eq(expr: &str) -> bool {
    let mut depth_paren: i32 = 0;
    let mut in_string = false;
    let mut prev = ' ';
    for ch in expr.chars() {
        match ch {
            '"' if prev != '\\' => in_string = !in_string,
            '(' if !in_string => depth_paren += 1,
            ')' if !in_string => depth_paren -= 1,
            '=' if !in_string && depth_paren == 0 && prev == ' ' => return true,
            _ => {}
        }
        prev = ch;
    }
    false
}

pub(crate) fn field_typed_rhs(trimmed: &str) -> Option<&str> {
    let (lhs, rhs) = trimmed.split_once(':')?;
    if lhs.contains(' ') || lhs.is_empty() {
        return None;
    }
    let rhs = rhs.trim_start();
    if rhs.is_empty() || rhs.starts_with('"') {
        return None;
    }
    Some(rhs)
}
