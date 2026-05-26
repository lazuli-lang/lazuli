//! `query.lookup` shorthand check.
//!
//! Single-key lookups should use shorthand
//! `query.lookup by_id by id: ID` instead of a separate `params` block
//! + `key id = params.id` pair. Includes the `typed_param` /
//! `lookup_key_assignment` parsers used to recognise the explicit form.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{leading_spaces, simple_canonical_diagnostic};

#[derive(Debug)]
pub(crate) struct LookupQueryFacts {
    line_index: usize,
    line: String,
    params: Vec<(String, String)>,
    key: Option<(String, String)>,
}

pub(crate) fn lookup_shorthand_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<LookupQueryFacts> = None;
    let mut current_child: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 4 && trimmed.starts_with("query.lookup ") {
            if let Some(query) = current_query.take() {
                diagnostics.extend(lookup_query_diagnostics(query));
            }

            current_query = if trimmed.contains(" by ") {
                None
            } else {
                Some(LookupQueryFacts {
                    line_index,
                    line: line.to_owned(),
                    params: Vec::new(),
                    key: None,
                })
            };
            current_child = None;
            continue;
        }

        if leading_spaces(line) <= 4 {
            if let Some(query) = current_query.take() {
                diagnostics.extend(lookup_query_diagnostics(query));
            }
            current_child = None;
            continue;
        }

        let Some(query) = current_query.as_mut() else {
            continue;
        };

        if leading_spaces(line) == 6 {
            if trimmed == "params" {
                current_child = Some("params");
            } else if let Some((lhs, rhs)) = lookup_key_assignment(trimmed) {
                query.key = Some((lhs.to_owned(), rhs.to_owned()));
                current_child = None;
            } else {
                current_child = None;
            }
        } else if leading_spaces(line) == 8 && current_child == Some("params") {
            if let Some((name, ty)) = typed_param(trimmed) {
                query.params.push((name.to_owned(), ty.to_owned()));
            }
        }
    }

    if let Some(query) = current_query {
        diagnostics.extend(lookup_query_diagnostics(query));
    }

    diagnostics
}

pub(crate) fn lookup_query_diagnostics(query: LookupQueryFacts) -> Vec<Diagnostic> {
    let Some((key_field, key_param)) = query.key.as_ref() else {
        return Vec::new();
    };

    if query.params.len() == 1 && query.params[0].0 == *key_field && query.params[0].0 == *key_param
    {
        vec![simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "query-lookup-shorthand",
            "single-key lookup queries should use shorthand, e.g. `query.lookup by_id by id: ID`.",
        )]
    } else {
        Vec::new()
    }
}

pub(crate) fn typed_param(trimmed_line: &str) -> Option<(&str, &str)> {
    let (name, rest) = trimmed_line.split_once(':')?;
    let name = name.trim();
    let ty = rest.trim().split_whitespace().next()?;

    if name.is_empty() || ty.is_empty() {
        None
    } else {
        Some((name, ty))
    }
}

pub(crate) fn lookup_key_assignment(trimmed_line: &str) -> Option<(&str, &str)> {
    let rest = trimmed_line.strip_prefix("key ")?;
    let (lhs, rhs) = rest.split_once('=')?;
    let lhs = lhs.trim();
    let rhs = rhs.trim().strip_prefix("params.")?.trim();

    if lhs.is_empty() || rhs.is_empty() {
        None
    } else {
        Some((lhs, rhs))
    }
}
