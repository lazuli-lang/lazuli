//! `scope override` inside a query block.
//!
//! Replacing inherited tenant/soft-delete safety scope is a deliberate
//! security opt-out — this checker enforces that the query carries an
//! explicit `policy` and that the override block has a `reason "..."`
//! child.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

#[derive(Debug)]
pub(crate) struct QuerySecurityFacts {
    line_index: usize,
    line: String,
    has_policy: bool,
    has_scope_override: bool,
    has_scope_override_reason: bool,
}

pub(crate) fn scope_override_policy_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<QuerySecurityFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4 && trimmed.starts_with("query.") {
            if let Some(query) = current_query.take() {
                diagnostics.extend(query_scope_override_diagnostics(query));
            }
            current_query = Some(QuerySecurityFacts {
                line_index,
                line: line.to_owned(),
                has_policy: false,
                has_scope_override: false,
                has_scope_override_reason: false,
            });
            continue;
        }

        if leading_spaces(line) <= 4 && !trimmed.is_empty() {
            if let Some(query) = current_query.take() {
                diagnostics.extend(query_scope_override_diagnostics(query));
            }
            continue;
        }

        let Some(query) = current_query.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 6 && trimmed.starts_with("policy ") {
            query.has_policy = true;
        } else if leading_spaces(line) == 6 && trimmed.starts_with("scope override") {
            query.has_scope_override = true;
        } else if leading_spaces(line) == 8 && trimmed.starts_with("reason ") {
            query.has_scope_override_reason = true;
        }
    }

    if let Some(query) = current_query {
        diagnostics.extend(query_scope_override_diagnostics(query));
    }

    diagnostics
}

pub(crate) fn query_scope_override_diagnostics(query: QuerySecurityFacts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if query.has_scope_override && !query.has_policy {
        diagnostics.push(simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "scope-override-policy",
            "`scope override` replaces inherited tenant/soft-delete safety scope; the query must declare an explicit `policy @policy.*`.",
        ));
    }

    if query.has_scope_override && !query.has_scope_override_reason {
        diagnostics.push(simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "scope-override-reason",
            "`scope override` should include a `reason \"...\"` child explaining why inherited tenant/soft-delete scope is replaced.",
        ));
    }

    diagnostics
}
