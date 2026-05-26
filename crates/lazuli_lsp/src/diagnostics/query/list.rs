//! `query.list`-specific checks: default order, paginate scope/value,
//! filter-index redundancy, search syntax, and the
//! `active_sessions` temporal-validity guard.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, leading_spaces, simple_canonical_diagnostic};

pub(crate) fn query_order_default_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut in_query_list = false;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading <= 4 {
            in_query_list = leading == 4 && trimmed.starts_with("query.list ");
        }

        if in_query_list && leading == 6 && trimmed == "order created_at desc" {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-order-default",
                "`query.list` defaults to `order created_at desc`; omit the line unless the query intentionally uses a different order.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn query_pagination_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query_mode: Option<&str> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading <= 4 {
            current_query_mode = if leading == 4 && trimmed.starts_with("query.") {
                trimmed.split_whitespace().next()
            } else {
                None
            };
        }

        let Some(value) = trimmed.strip_prefix("paginate ") else {
            continue;
        };

        if current_query_mode != Some("query.list") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-pagination-scope",
                "`paginate` is a `query.list` contract; lookup and SQL queries should model limits explicitly in their own params or SQL.",
            ));
        }

        if !matches!(value.trim().parse::<u64>(), Ok(page_size) if page_size > 0) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-pagination-size",
                "`paginate` should declare a positive integer default page size, e.g. `paginate 50`.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn query_filter_index_diagnostics(source: &str) -> Vec<Diagnostic> {
    let generated = generated_query_filter_indexes(source);
    if generated.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(index) = trimmed.strip_prefix("index ") else {
            continue;
        };
        let index = normalize_index_value(index);

        if generated.contains(&index) {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-filter-index-generated",
                "`query.list` equality filters generate this tenant-aware index; omit the explicit `index` unless the query needs a non-default index shape.",
            ));
        }
    }

    diagnostics
}

pub(crate) fn query_search_syntax_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.starts_with('#') {
            continue;
        }

        if trimmed.contains("= params.search") {
            diagnostics.push(simple_canonical_diagnostic(
                line_index,
                line,
                DiagnosticSeverity::WARNING,
                "query-search-syntax",
                "text matching should use `search params.search over ...` instead of an equality-looking filter such as `name = params.search`.",
            ));
        }
    }

    diagnostics
}

#[derive(Debug)]
pub(crate) struct ActiveSessionQueryFacts {
    line_index: usize,
    line: String,
    has_temporal_scope: bool,
    expires_not_nil: Option<(usize, String)>,
}

pub(crate) fn active_session_query_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut current_query: Option<ActiveSessionQueryFacts> = None;

    for (line_index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if leading_spaces(line) == 4 && trimmed.starts_with("query.list ") {
            if let Some(query) = current_query.take() {
                diagnostics.extend(active_session_query_facts_diagnostics(query));
            }

            if trimmed
                .split_whitespace()
                .nth(1)
                .is_some_and(|name| name == "active_sessions")
            {
                current_query = Some(ActiveSessionQueryFacts {
                    line_index,
                    line: line.to_owned(),
                    has_temporal_scope: false,
                    expires_not_nil: None,
                });
            }
            continue;
        }

        if leading_spaces(line) <= 4 && !trimmed.is_empty() {
            if let Some(query) = current_query.take() {
                diagnostics.extend(active_session_query_facts_diagnostics(query));
            }
            continue;
        }

        let Some(query) = current_query.as_mut() else {
            continue;
        };

        if trimmed.contains("expires_at > ctx.now")
            || trimmed.contains("expires_at >= ctx.now")
            || trimmed.contains("guarantees expires_at > ctx.now")
            || trimmed.contains("guarantees expires_at >= ctx.now")
        {
            query.has_temporal_scope = true;
        }

        if trimmed == "expires_at != nil" {
            query.expires_not_nil = Some((line_index, line.to_owned()));
        }
    }

    if let Some(query) = current_query {
        diagnostics.extend(active_session_query_facts_diagnostics(query));
    }

    diagnostics
}

pub(crate) fn active_session_query_facts_diagnostics(
    query: ActiveSessionQueryFacts,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    if let Some((line_index, line)) = query.expires_not_nil {
        diagnostics.push(simple_canonical_diagnostic(
            line_index,
            &line,
            DiagnosticSeverity::WARNING,
            "active-session-temporal-scope",
            "`active_sessions` should prove temporal validity; `expires_at != nil` can include expired sessions. Use an explicit `expires_at > ctx.now` guard or a modifier `guarantees expires_at > ctx.now` contract.",
        ));
    } else if !query.has_temporal_scope {
        diagnostics.push(simple_canonical_diagnostic(
            query.line_index,
            &query.line,
            DiagnosticSeverity::WARNING,
            "active-session-temporal-scope",
            "`active_sessions` should declare temporal validity with an explicit `expires_at > ctx.now` guard or a modifier `guarantees expires_at > ctx.now` contract.",
        ));
    }

    diagnostics
}

pub(crate) fn generated_query_filter_indexes(source: &str) -> HashSet<String> {
    let lines: Vec<_> = source.lines().collect();
    let tenancy_axis = single_tenancy_axis(&lines);
    let mut indexes = HashSet::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();

        if leading_spaces(line) == 4
            && trimmed.starts_with("query.list ")
            && !query_block_has_scope_override(&lines, index)
        {
            for field in query_block_filter_index_fields(&lines, index) {
                let value = tenancy_axis
                    .as_ref()
                    .map(|tenant| format!("{tenant}, {field}"))
                    .unwrap_or(field);
                indexes.insert(normalize_index_value(&value));
            }
        }

        index += 1;
    }

    indexes
}

pub(crate) fn single_tenancy_axis(lines: &[&str]) -> Option<String> {
    let axes: HashSet<String> = lines
        .iter()
        .filter_map(|line| {
            let axis = line.trim_start().strip_prefix("tenancy ")?.trim();
            (!axis.is_empty() && axis != "none").then(|| axis.to_owned())
        })
        .collect();

    if axes.len() == 1 {
        axes.into_iter().next()
    } else {
        None
    }
}

pub(crate) fn query_block_has_scope_override(lines: &[&str], start: usize) -> bool {
    let mut index = start + 1;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        if !trimmed.is_empty() && leading_spaces(line) <= 4 {
            break;
        }
        if trimmed == "scope override" {
            return true;
        }
        index += 1;
    }

    false
}

pub(crate) fn query_block_filter_index_fields(lines: &[&str], start: usize) -> Vec<String> {
    let mut fields = Vec::new();
    let mut in_filters = false;
    let mut index = start + 1;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let leading = leading_spaces(line);

        if !trimmed.is_empty() && leading <= 4 {
            break;
        }

        if !trimmed.is_empty() {
            if leading == 6 {
                in_filters = trimmed == "filters";
            } else if in_filters
                && leading == 8
                && let Some(field) = filter_index_field(trimmed)
            {
                fields.push(field);
            }
        }

        index += 1;
    }

    fields
}

pub(crate) fn filter_index_field(filter: &str) -> Option<String> {
    if filter.contains(" has ")
        || filter.contains(" != ")
        || filter.contains(" = nil")
        || filter.contains(" != nil")
    {
        return None;
    }

    if let Some((field, param)) = filter.split_once(" when ") {
        let field = field.trim();
        let param = param.trim().strip_prefix("params.")?;
        if is_identifier(field) && field == param {
            return Some(field.to_owned());
        }
        return None;
    }

    if let Some((left, right)) = filter.split_once(" = ") {
        let left = left.trim();
        let param = right.trim().strip_prefix("params.")?;

        if is_identifier(left) && left == param {
            return Some(left.to_owned());
        }

        if let Some(relation) = left.strip_suffix(".id")
            && is_identifier(relation)
            && param == format!("{relation}_id")
        {
            return Some(relation.to_owned());
        }
    }

    None
}

pub(crate) fn normalize_index_value(value: &str) -> String {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}
