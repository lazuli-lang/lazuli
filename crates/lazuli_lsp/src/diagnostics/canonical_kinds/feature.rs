//! `feature X` indent-2 unknown-kind diagnostic.
//!
//! The parser silently drops blocks whose head keyword is unknown — a
//! typo like `comand` or `wokflow` produces no diagnostic and just
//! erases the block from the IR. This producer flags every indent-2
//! token inside `feature X` that doesn't match the closed
//! `FEATURE_BODY_KINDS` catalog, suggesting the closest match via
//! Damerau-Levenshtein distance ≤ 2.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use super::closest_feature_body_kind;
use crate::{leading_spaces, simple_canonical_diagnostic};

/// 2026-05-15 — Closed catalog of indent-2 kind keywords that may
/// child of `feature X`. Used by `feature_unknown_kind_diagnostics`
/// to detect typos like `comand` (command) / `quiery` (query) /
/// `wokflow` (workflow) — surfaced 2026-05-15 when Lucas wrote
/// `comand move` and the LSP stayed silent.
///
/// Keep this list aligned with the parser's accepted feature-body
/// vocabulary. Sorted alphabetically for diff hygiene.
pub(crate) const FEATURE_BODY_KINDS: &[&str] = &[
    "agent",
    "aggregate",
    "api",
    "attach_ctx",
    "auth",
    "cache",
    "channel",
    "command",
    "compatibility",
    "context",
    "defaults",
    "delegated_to",
    "domain",
    "emits",
    "enum",
    "errors",
    "escape_route",
    "event",
    "event.trace",
    "event_group",
    "events",
    "extends",
    "extensions",
    "import",
    "imports",
    "invariants",
    "job",
    "mcp_server",
    "non_goals",
    "notification",
    "operation",
    "out_of_scope",
    "permission",
    "poller",
    "policies",
    "purpose",
    "query.list",
    "query.lookup",
    "query.sql",
    "query.view",
    "record",
    "refs",
    "report",
    "requires",
    "role",
    "secret_rotation",
    "subscription",
    "surface",
    "tenant_migration",
    "tests",
    "tools",
    "translation",
    "uses",
    "view",
    "webhook",
    "webhook_event",
    "workflow",
];

/// 2026-05-15 — file-local diagnostic that flags any indent-2 word
/// inside `feature X` body which is NOT a known kind keyword.
/// Suggests the closest known kind via Damerau-Levenshtein distance ≤ 2
/// when one exists; otherwise lists all valid kinds. Fires as a
/// WARNING (not ERROR) so the user can keep typing while the squiggle
/// nudges them to fix.
///
/// Ignores comments, blank lines, and lines whose first token starts
/// with `@` (decorator/anchor reference) or contains `(`/`:` (typed
/// field decl, namespaced-decorator call, key-value).
pub(crate) fn feature_unknown_kind_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut inside_feature = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = leading_spaces(line);
        if leading == 0 {
            inside_feature = trimmed.starts_with("feature ");
            continue;
        }
        if !inside_feature || leading != 2 {
            continue;
        }
        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };
        // Skip decorators, anchors, namespaced refs, key-value lines.
        if first.starts_with('@')
            || first.contains('(')
            || first.contains(':')
            || first.contains('=')
        {
            continue;
        }
        if FEATURE_BODY_KINDS.contains(&first) {
            continue;
        }
        let suggestion = closest_feature_body_kind(first, 2);
        let message = match suggestion {
            Some(suggested) => {
                format!("unknown feature block kind `{first}`. Did you mean `{suggested}`?")
            }
            None => format!(
                "unknown feature block kind `{first}`. Valid kinds: command / api / query.list / query.lookup / query.sql / query.view / view / webhook / job / agent / notification / poller / report / channel / cache / aggregate / events / event_group / event.trace / workflow / surface / extensions / tests / auth / errors / policies / domain / defaults / uses / purpose / context / non_goals / role / permission / etc."
            ),
        };
        // ERROR not WARNING: an unknown kind keyword causes the
        // parser to SILENTLY skip the entire block — the user-intended
        // command/api/query never enters the IR, and the regenerated
        // dist looks like the feature simply forgot to declare it.
        // Compile-blocking visibility is the right choice.
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            line,
            DiagnosticSeverity::ERROR,
            "feature-unknown-kind",
            &message,
        ));
    }

    diagnostics
}
