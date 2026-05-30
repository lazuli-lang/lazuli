//! `query.<mode>` header recognition.
//!
//! Rejects legacy bare `query` declarations and unknown mode suffixes.
//! The closed set is `list | lookup | sql | view | compose`.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

pub(crate) fn query_mode_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some(first) = trimmed.split_whitespace().next() else {
            continue;
        };

        // Only validate query declarations, not references. Declarations live
        // at indent 2 (legacy top-level) or 4 (canonical, inside `domain`)
        // inside a feature; references appear in `invalidates`, `source`,
        // `target`, `let`, etc. at deeper indents.
        let leading = leading_spaces(line);
        if leading != 2 && leading != 4 {
            continue;
        }

        if first == "query" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-mode",
                "query declarations should use an explicit mode: `query.list <name>`, `query.lookup <name>`, `query.sql <name>`, `query.view <name>`, or `query.compose <name>`. The kind belongs in the header so cold-readers see it before the body.",
            ));
        } else if let Some(mode) = first.strip_prefix("query.") {
            // Strip parens/args used in references like `query.by_id(id: route.id)`.
            let mode = mode
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            if !matches!(mode, "list" | "lookup" | "sql" | "view" | "compose") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "query-mode",
                    "unknown query mode. Use `query.list`, `query.lookup`, `query.sql`, `query.view`, or `query.compose`.",
                ));
            }
        }
    }

    diagnostics
}
