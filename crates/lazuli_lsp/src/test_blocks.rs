//! `tests` block diagnostics + the indent-stack helpers they share with
//! the anchor-whitelist + view-anchor / extends-anchor inspectors.
//!
//! The producer (`test_block_diagnostics`) walks every line of a `.lzi`
//! source, maintaining a stack of open `(indent, kind)` blocks. Each
//! `tests` keyword opens a "test context" tied to its parent block —
//! `command`, `workflow.transition`, `rule`, or view `anchor`. Assertion
//! lines inside that context are then validated against the closed
//! vocabulary in `is_valid_test_assertion`. The shared helpers
//! (`stack_kind`, `test_context`, `is_transition_line`, `view_anchor`,
//! `extends_anchor`, `extensible_by_features`) are also consumed by
//! `anchor_whitelist_diagnostics` and the broader source-walk dispatch.
//!
//! Wave R7-3 extract: lifted out of `lib.rs`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

pub(crate) fn test_block_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut current_test_context: Option<String> = None;
    // Indent at which the most recent `tests` keyword sat. Assertion
    // lines live STRICTLY DEEPER; any line at this indent or shallower
    // closes the block (the legacy `<= 4` heuristic assumed tests were
    // always two levels deep, which broke when the canonical fixture
    // nested `tests` under `workflow → transition` at indent 10).
    let mut test_block_indent: Option<usize> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = leading_spaces(line);

        while stack.last().is_some_and(|(level, _)| *level >= indent) {
            stack.pop();
        }

        if let Some(block_indent) = test_block_indent
            && indent <= block_indent
        {
            current_test_context = None;
            test_block_indent = None;
        }

        if trimmed == "tests" {
            let context = test_context(&stack);
            if let Some(context) = context {
                current_test_context = Some(context);
                test_block_indent = Some(indent);
            } else {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "tests-placement",
                    "`tests` blocks are allowed only as the last child of a command, workflow transition, rule, or view anchor block.",
                ));
                current_test_context = None;
            }
        } else if let Some(context) = current_test_context.as_deref() {
            if indent >= 6 && !is_valid_test_assertion(context, trimmed) {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "tests-vocabulary",
                    "unknown test assertion for this construct. Use the closed tests vocabulary for command, workflow transition, rule, or view anchor blocks.",
                ));
            } else if indent >= 6
                && context == "command"
                && (trimmed.starts_with("permits @") || trimmed.starts_with("forbids @"))
            {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "tests-generated-policy-matrix",
                    "command policy actor-matrix tests are generated from `policy @policy.*`; author only predicate tests that add behavior beyond the policy.",
                ));
            }
        }

        if let Some(kind) = stack_kind(trimmed) {
            stack.push((indent, kind.to_owned()));
        }
    }

    diagnostics
}

pub(crate) fn stack_kind(trimmed_line: &str) -> Option<&'static str> {
    let first = trimmed_line.split_whitespace().next()?;

    if first == "command" {
        Some("command")
    } else if first == "rule" {
        Some("rule")
    // Block-form workflow transitions (`transition <name>` followed by
    // indented `from`/`to`/`policy`/`emits`/`tests` children) are the
    // canonical surface today. The legacy inline form
    // (`<state>: <state> -> <name>`) is captured by
    // `is_transition_line` below for back-compat with older fixtures.
    } else if first == "transition" {
        Some("transition")
    } else if view_anchor(trimmed_line).is_some() {
        Some("anchor")
    } else if is_transition_line(trimmed_line) {
        Some("transition")
    } else {
        None
    }
}

pub(crate) fn test_context(stack: &[(usize, String)]) -> Option<String> {
    stack
        .last()
        .filter(|(_, kind)| matches!(kind.as_str(), "command" | "transition" | "rule" | "anchor"))
        .map(|(_, kind)| kind.clone())
}

pub(crate) fn is_transition_line(trimmed_line: &str) -> bool {
    let Some((lhs, rhs)) = trimmed_line.split_once(':') else {
        return false;
    };

    !lhs.trim().is_empty() && rhs.contains("->")
}

pub(crate) fn is_valid_test_assertion(context: &str, trimmed_line: &str) -> bool {
    match context {
        "command" => {
            trimmed_line.starts_with("permits @")
                || trimmed_line.starts_with("forbids @")
                || trimmed_line.starts_with("allows when ")
                || trimmed_line.starts_with("denies when ")
        }
        "transition" => {
            trimmed_line.starts_with("allows from ")
                || trimmed_line.starts_with("denies from ")
                || trimmed_line.starts_with("allows as @")
                || trimmed_line.starts_with("denies as @")
        }
        "rule" => {
            trimmed_line.starts_with("allows when ") || trimmed_line.starts_with("denies when ")
        }
        "anchor" => {
            trimmed_line.starts_with("allows extension ")
                || trimmed_line.starts_with("denies extension ")
        }
        _ => false,
    }
}

pub(crate) fn view_anchor(trimmed_line: &str) -> Option<&str> {
    let marker = " id @anchor.";
    let (_, rest) = trimmed_line.split_once(marker)?;
    rest.split_whitespace().next()
}

pub(crate) fn extends_anchor(trimmed_line: &str) -> Option<&str> {
    let rest = trimmed_line.strip_prefix("extends @anchor.")?;
    rest.split_whitespace().next()
}

pub(crate) fn extensible_by_features(trimmed_line: &str) -> Vec<String> {
    let Some(rest) = trimmed_line.strip_prefix("extensible_by ") else {
        return Vec::new();
    };

    rest.split(',')
        .map(str::trim)
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
        .collect()
}
