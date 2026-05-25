//! Diagnostics for the `query.*` family (list / lookup / sql / view).
//!
//! Largest single Lazuli vocabulary surface in canonical source — every
//! read path is declared here. The producers split along orthogonal
//! concerns:
//!
//! | Producer | Concern |
//! |---|---|
//! | [`query_mode_diagnostics`] | every query declaration uses an explicit mode (`query.list`, `query.lookup`, `query.sql`, `query.view`); legacy bare `query` is rejected. |
//! | [`previously_mode_diagnostics`] | `previously migrated|alias` must follow the right shape per scope (field / header / transition / other). |
//! | [`query_order_default_diagnostics`] | `query.list` defaults to `order created_at desc` — flag the redundant line. |
//! | [`query_pagination_diagnostics`] | `paginate` is `query.list`-only; the value must be a positive integer. |
//! | [`query_filter_index_diagnostics`] | suggest dropping `index` declarations that match an already-generated tenant-aware index from a `query.list` equality filter. |
//! | [`query_search_syntax_diagnostics`] | reject `name = params.search` (equality) in favour of `search params.search over ...`. |
//! | [`active_session_query_diagnostics`] | `query.list active_sessions` must guard temporal validity via `expires_at > ctx.now` (or `guarantees`). |
//! | [`lookup_shorthand_diagnostics`] | single-key `query.lookup` should use shorthand `by <field>: <Type>`. |
//!
//! Shared helpers (`single_tenancy_axis`, `query_block_*`,
//! `filter_index_field`, `normalize_index_value`, `typed_param`,
//! `lookup_key_assignment`) stay here because every consumer is
//! query-adjacent. `is_identifier` / `is_type_name` live in `lib.rs`
//! since 40+ other catalogs depend on them.

use std::collections::HashSet;

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity};

use crate::{is_identifier, leading_spaces, simple_canonical_diagnostic};

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
                "query declarations should use an explicit mode: `query.list <name>`, `query.lookup <name>`, `query.sql <name>`, or `query.view <name>`. The kind belongs in the header so cold-readers see it before the body.",
            ));
        } else if let Some(mode) = first.strip_prefix("query.") {
            // Strip parens/args used in references like `query.by_id(id: route.id)`.
            let mode = mode
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            if !matches!(mode, "list" | "lookup" | "sql" | "view") {
                diagnostics.push(simple_canonical_diagnostic(
                    line_index,
                    line,
                    DiagnosticSeverity::WARNING,
                    "query-mode",
                    "unknown query mode. Use `query.list`, `query.lookup`, `query.sql`, or `query.view`.",
                ));
            }
        }
    }

    diagnostics
}

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
