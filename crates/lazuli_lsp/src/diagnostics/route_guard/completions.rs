//! Completion items for the IR Route-Guards LSP layer.
//!
//! The public `route_guard_completions` orchestrator decides which set
//! of items to surface based on the cursor's textual prefix + context
//! (see `super::context`). Item builders (`snippet_completion`,
//! `route_path_completion_items`, …) are reused by the lifecycle and
//! code-action layers via the crate-root re-export.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Documentation, Position};

use crate::{collect_policy_categories_for_feature, collect_query_refs, line_prefix_at_position};

use super::context::{
    at_app_child_completion_line, collect_route_paths, in_app_route_guard_block,
    in_guard_policy_child_context, in_view_or_audience_guard_context, route_guard_context_feature,
};

pub(crate) const ROUTE_GUARD_DEFAULT_CLAUSES: &[&str] = &[
    "default_policy",
    "default_unauthenticated_redirect",
    "default_unauthorized_redirect",
];

/// Completion for `ir-route-guards` authoring positions. The helper is
/// intentionally text-walk based, matching the existing LSP convention:
/// it gives immediate editor help even before parser/analyzer cells know
/// the new surface.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::route_guard_completions;
/// use tower_lsp::lsp_types::Position;
///
/// // Outside any route-guard trigger — None.
/// let result = route_guard_completions(
///     "feature billing\n",
///     Position { line: 0, character: 0 },
/// );
/// assert!(result.is_none());
/// ```
pub fn route_guard_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);
    let trimmed_before = before.trim_start();

    if route_guard_redirect_path_trigger(trimmed_before).is_some() {
        return Some(route_path_completion_items(
            source,
            redirect_trigger_has_open_quote(trimmed_before),
        ));
    }

    if let Some(rest) = trimmed_before.strip_prefix("actor_query ") {
        if rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            && at_app_child_completion_line(source, position)
        {
            return Some(query_ref_completion_items(source));
        }
    }

    if let Some(rest) = trimmed_before.strip_prefix("default_policy ") {
        if rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '.')
            && in_app_route_guard_block(source, position).is_some()
        {
            let feature = route_guard_context_feature(source, position);
            return Some(policy_ref_completion_items(
                source,
                feature.as_deref(),
                true,
            ));
        }
    }

    if let Some(rest) = trimmed_before.strip_prefix("policy ") {
        if rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '@' || c == '.')
            && in_view_or_audience_guard_context(source, position)
        {
            let feature = route_guard_context_feature(source, position);
            return Some(policy_ref_completion_items(
                source,
                feature.as_deref(),
                true,
            ));
        }
    }

    let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
    let is_partial_word = !trimmed_before.is_empty()
        && trimmed_before
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !(is_blank_indented || is_partial_word) {
        return None;
    }

    if in_app_route_guard_block(source, position).is_some() {
        return Some(route_guard_default_clause_completion_items());
    }

    if in_guard_policy_child_context(source, position) {
        return Some(vec![
            snippet_completion(
                "on_unauthenticated redirect",
                "on_unauthenticated redirect \"${1:/sign-in}\"",
                "Redirect when no actor is signed in.",
            ),
            snippet_completion(
                "on_unauthorized redirect",
                "on_unauthorized redirect \"${1:/403}\"",
                "Redirect when a signed-in actor fails the policy.",
            ),
        ]);
    }

    if in_view_or_audience_guard_context(source, position) {
        return Some(vec![snippet_completion(
            "policy @policy.<name>",
            "policy @policy.${1:name}\n  on_unauthenticated redirect \"${2:/sign-in}\"\n  on_unauthorized redirect \"${3:/403}\"",
            "Declare a route guard policy and per-view redirects.",
        )]);
    }

    if at_app_child_completion_line(source, position) {
        return Some(vec![
            snippet_completion(
                "route_guard",
                "route_guard\n  default_policy @scope.authenticated\n  default_unauthenticated_redirect \"${1:/sign-in}\"\n  default_unauthorized_redirect \"${2:/403}\"",
                "Declare app-level route guard defaults.",
            ),
            snippet_completion(
                "actor_query <feature>.query.<name>",
                "actor_query ${1:account.query.me}",
                "Wire the query that resolves the active actor.",
            ),
        ]);
    }

    None
}

pub(crate) fn route_guard_redirect_path_trigger(trimmed_before: &str) -> Option<&'static str> {
    let triggers = [
        "on_unauthenticated redirect ",
        "on_unauthorized redirect ",
        "default_unauthenticated_redirect ",
        "default_unauthorized_redirect ",
    ];
    triggers
        .into_iter()
        .find(|trigger| trimmed_before.starts_with(trigger))
}

pub(crate) fn redirect_trigger_has_open_quote(trimmed_before: &str) -> bool {
    route_guard_redirect_path_trigger(trimmed_before)
        .map(|trigger| trimmed_before[trigger.len()..].starts_with('"'))
        .unwrap_or(false)
}

pub(crate) fn route_path_completion_items(source: &str, open_quote: bool) -> Vec<CompletionItem> {
    collect_route_paths(source)
        .into_iter()
        .map(|path| CompletionItem {
            label: path.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some("Declared route path.".to_owned()),
            insert_text: Some(if open_quote {
                path
            } else {
                format!("\"{path}\"")
            }),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn query_ref_completion_items(source: &str) -> Vec<CompletionItem> {
    collect_query_refs(source)
        .into_iter()
        .map(|query_ref| CompletionItem {
            label: query_ref,
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some("Declared query usable as `actor_query`.".to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn policy_ref_completion_items(
    source: &str,
    feature_hint: Option<&str>,
    include_atom_prefixes: bool,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    if include_atom_prefixes {
        for prefix in ["@policy.", "@scope.", "@role.", "@actor."] {
            items.push(CompletionItem {
                label: prefix.to_owned(),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some("Route guard policy reference prefix.".to_owned()),
                ..CompletionItem::default()
            });
        }
    }
    items.extend(
        collect_policy_categories_for_feature(source, feature_hint)
            .into_iter()
            .map(|name| CompletionItem {
                label: format!("@policy.{name}"),
                kind: Some(CompletionItemKind::REFERENCE),
                detail: Some("Feature-local policy category.".to_owned()),
                ..CompletionItem::default()
            }),
    );
    items
}

pub(crate) fn route_guard_default_clause_completion_items() -> Vec<CompletionItem> {
    ROUTE_GUARD_DEFAULT_CLAUSES
        .iter()
        .map(|clause| match *clause {
            "default_policy" => snippet_completion(
                "default_policy",
                "default_policy @scope.authenticated",
                "Fallback policy for unguarded routes.",
            ),
            "default_unauthenticated_redirect" => snippet_completion(
                "default_unauthenticated_redirect",
                "default_unauthenticated_redirect \"${1:/sign-in}\"",
                "Fallback redirect when no actor is signed in.",
            ),
            "default_unauthorized_redirect" => snippet_completion(
                "default_unauthorized_redirect",
                "default_unauthorized_redirect \"${1:/403}\"",
                "Fallback redirect when a signed-in actor fails policy.",
            ),
            _ => CompletionItem::default(),
        })
        .collect()
}

pub(crate) fn snippet_completion(label: &str, body: &str, detail: &str) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.to_owned()),
        insert_text: Some(body.to_owned()),
        insert_text_format: Some(tower_lsp::lsp_types::InsertTextFormat::SNIPPET),
        documentation: Some(Documentation::String(detail.to_owned())),
        ..CompletionItem::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    #[test]
    fn no_completions_outside_route_guard_trigger() {
        assert!(route_guard_completions(
            "feature billing\n",
            Position { line: 0, character: 0 }
        )
        .is_none());
    }

    #[test]
    fn no_completions_for_empty_source() {
        assert!(route_guard_completions("", Position { line: 0, character: 0 }).is_none());
    }
}
