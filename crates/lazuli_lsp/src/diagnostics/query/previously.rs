//! `previously migrated|alias` migration-history shape check.
//!
//! Spelling lives across catalogs (records, commands, views, agents,
//! …); this checker recognises field / header / transition / other
//! scopes and rejects inline forms with a per-scope diagnostic message.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::simple_canonical_diagnostic;

pub(crate) fn previously_mode_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((head, tail)) = trimmed.split_once(" previously ") else {
            continue;
        };

        let tail = tail.trim_start();
        if !tail.starts_with("migrated ") && !tail.starts_with("alias ") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "previously-mode-contract",
                "`previously` should declare `migrated` or `alias` so migration-only history is distinct from compatibility aliases.",
            ));
            continue;
        }

        match inline_previously_kind(head, tail) {
            InlinePreviouslyKind::Field => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "previously-field-inline",
                    "field-level `previously migrated|alias <old>` should be a child of the field, not inline before `:`. Keep `<name>: <Type> = <value>` contiguous and put `previously migrated <old>` on the next line indented one level deeper.",
                ));
            }
            InlinePreviouslyKind::Header => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "previously-header-inline",
                    "header-level `previously migrated|alias <old>` should be a child of the block, not inline. Keep the kind + name on the header line and put `previously migrated <old>` on the next line indented one level deeper so cold-readers see one concept per line.",
                ));
            }
            InlinePreviouslyKind::Transition => {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "previously-transition-inline",
                    "workflow transitions should keep the `<name>: <state> -> <state>` shape contiguous; declare `previously migrated <old>` as a transition child on the next line.",
                ));
            }
            InlinePreviouslyKind::Other => {}
        }
    }

    diagnostics
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InlinePreviouslyKind {
    Field,
    Header,
    Transition,
    Other,
}

pub(crate) fn inline_previously_kind(head: &str, tail: &str) -> InlinePreviouslyKind {
    let head = head.trim();
    if head.is_empty() {
        return InlinePreviouslyKind::Other;
    }
    let first = head.split_whitespace().next().unwrap_or("");

    // Block headers (`resource <Name>`, `command <name>`, etc.) — the
    // identifier comes first, then `previously migrated <old>`. Tail has
    // *no* `:` (no field/transition shape) and the head is two tokens
    // (kind + name).
    if matches!(
        first,
        "resource"
            | "record"
            | "enum"
            | "command"
            | "workflow"
            | "job"
            | "webhook"
            | "api"
            | "view"
            | "rule"
            | "agent"
            | "feature"
            | "notification"
    ) {
        return InlinePreviouslyKind::Header;
    }

    // Transition shape: `<name>: <state> -> <state>` (with optional `previously
    // migrated <old>` between name and `:`). Detected by the `->` token in
    // tail.
    if tail.contains(" -> ") {
        return InlinePreviouslyKind::Transition;
    }

    // Field shape: a single identifier head followed by `previously migrated
    // <old>: <Type>`.
    if head.contains(' ') {
        return InlinePreviouslyKind::Other;
    }
    if tail.contains(':') {
        return InlinePreviouslyKind::Field;
    }

    InlinePreviouslyKind::Other
}
