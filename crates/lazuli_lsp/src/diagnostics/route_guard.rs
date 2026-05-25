//! Diagnostics, completions, hovers, and code-action backends for the
//! IR Route-Guards family (`policy`, `on_unauthenticated`,
//! `on_unauthorized`, `default_policy`, `default_*_redirect`).
//!
//! Mirror structure of `diagnostics/lifecycle.rs`: the language-layer
//! support for `route_guard` lives here, and the code-action frontend
//! in `code_actions/route_guard.rs` calls into these helpers via the
//! `pub(crate) use diagnostics::route_guard::*;` re-export in lib.rs.
//!
//! Two public entry points are surfaced to `lib.rs::server`:
//! [`route_guard_completions`] and [`route_guard_hover`]. The rest are
//! `pub(crate)`.
//!
//! Shared helpers (`block_kind_at`, `feature_name`, `leading_spaces`,
//! `position_at_line_start`, `simple_edit_action`,
//! `collect_policy_categories_for_feature`) stay in lib.rs because
//! they're consumed by sibling modules.

use std::collections::HashSet;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Documentation, Position};

use crate::{
    block_kind_at, byte_index_for_utf16_position, collect_policy_categories_for_feature,
    collect_query_refs, feature_name, leading_spaces, line_prefix_at_position,
};

// ── IR Route-Guards — LSP completion / hover / code actions ────────────────

pub(crate) const ROUTE_GUARD_DEFAULT_CLAUSES: &[&str] = &[
    "default_policy",
    "default_unauthenticated_redirect",
    "default_unauthorized_redirect",
];

#[derive(Debug, Clone)]
pub(crate) struct RouteGuardBlock {
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) end_line: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct RouteGuardViewBlock {
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) end_line: usize,
    pub(crate) feature_hint: Option<String>,
}

/// Completion for `ir-route-guards` authoring positions. The helper is
/// intentionally text-walk based, matching the existing LSP convention:
/// it gives immediate editor help even before parser/analyzer cells know
/// the new surface.
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

pub fn route_guard_hover(source: &str, position: Position, word: &str) -> Option<String> {
    if word.starts_with("policy.") {
        if let Some(hover) = route_guard_policy_ref_hover(source, position, word) {
            return Some(hover);
        }
    }

    match word {
        "policy" if in_view_or_audience_guard_context(source, position) => {
            let layer = if enclosing_audience_block(source, position).is_some()
                && enclosing_view_block(source, position).is_none()
            {
                "Per-audience default route guard inherited by views unless they declare their own `policy`."
            } else {
                "Per-view route guard evaluated on every navigation to this view."
            };
            Some(format!(
                "`policy`\n\n{layer} Redirects use `on_unauthenticated` and `on_unauthorized`."
            ))
        }
        "on_unauthenticated" => Some(
            "`on_unauthenticated`\n\nRedirect target when the active actor is not signed in. Falls back through view, audience, then app defaults."
                .to_owned(),
        ),
        "on_unauthorized" => Some(
            "`on_unauthorized`\n\nRedirect target when the signed-in actor fails the guard policy. Falls back through view, audience, then app defaults."
                .to_owned(),
        ),
        "route_guard" => Some(
            "`route_guard`\n\nApp-level fallback route guard block carrying `default_policy`, `default_unauthenticated_redirect`, and `default_unauthorized_redirect`."
                .to_owned(),
        ),
        "actor_query" => Some(
            "`actor_query`\n\nApp-level `<feature>.query.<name>` reference used by the runtime SDK to resolve the current actor for route guards."
                .to_owned(),
        ),
        "default_unauthenticated_redirect" => Some(
            "`default_unauthenticated_redirect`\n\nInside `app.route_guard`, fallback path for unauthenticated users when a view or audience does not override it."
                .to_owned(),
        ),
        "default_unauthorized_redirect" => Some(
            "`default_unauthorized_redirect`\n\nInside `app.route_guard`, fallback path for signed-in users who fail a guard policy when no narrower layer overrides it."
                .to_owned(),
        ),
        _ => None,
    }
}

pub(crate) fn route_guard_policy_ref_hover(
    source: &str,
    position: Position,
    word: &str,
) -> Option<String> {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    let policy_ref = format!("@{word}");
    if !line.contains(&policy_ref) || !in_view_or_audience_guard_context(source, position) {
        return None;
    }
    let feature = route_guard_context_feature(source, position);
    let (atoms, source_label) = resolve_policy_atoms(source, feature.as_deref(), &policy_ref)
        .unwrap_or_else(|| {
            (
                Vec::new(),
                "unresolved policy category in this document".to_owned(),
            )
        });
    let atoms_text = if atoms.is_empty() {
        "unresolved".to_owned()
    } else {
        atoms
            .iter()
            .map(|atom| format!("`{atom}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let alignment = route_guard_backend_alignment(source, position, &policy_ref);
    Some(
        [
            format!("**`{policy_ref}`** — route guard policy reference."),
            String::new(),
            format!("**Resolved atoms**: {atoms_text}"),
            String::new(),
            format!("**Source**: {source_label}"),
            String::new(),
            format!("**Backend alignment**: {alignment}"),
        ]
        .join("\n"),
    )
}

pub(crate) fn collect_route_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("path ") else {
            continue;
        };
        if let Some(path) = first_quoted_value(rest) {
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    paths
}

pub(crate) fn first_quoted_value(value: &str) -> Option<String> {
    let open = value.find('"')?;
    let rest = &value[open + 1..];
    let close = rest.find('"')?;
    Some(rest[..close].to_owned())
}

pub(crate) fn route_guard_context_feature(source: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            for prefix in ["feature ", "surface ", "experience "] {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    let name = rest.split_whitespace().next().unwrap_or("");
                    if !name.is_empty() {
                        return Some(name.to_owned());
                    }
                }
            }
        }
    }
    None
}

pub(crate) fn in_app_body_context(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            return trimmed.starts_with("app ");
        }
        if indent < cursor_indent && trimmed.starts_with("route_guard") {
            return false;
        }
    }
    false
}

pub(crate) fn at_app_child_completion_line(source: &str, position: Position) -> bool {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    leading_spaces(line) == 2 && in_app_body_context(source, position)
}

pub(crate) fn in_app_route_guard_block(
    source: &str,
    position: Position,
) -> Option<RouteGuardBlock> {
    let block = app_route_guard_block(source)?;
    let line_idx = position.line as usize;
    let line = source.lines().nth(line_idx).unwrap_or("");
    let indent = leading_spaces(line);
    if line_idx > block.header_line && line_idx < block.end_line && indent > block.header_indent {
        Some(block)
    } else {
        None
    }
}

pub(crate) fn app_route_guard_block(source: &str) -> Option<RouteGuardBlock> {
    let lines: Vec<&str> = source.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed == "route_guard" || trimmed.starts_with("route_guard ") {
            let header_indent = leading_spaces(line);
            let end_line = find_block_end(&lines, idx, header_indent);
            return Some(RouteGuardBlock {
                header_line: idx,
                header_indent,
                end_line,
            });
        }
    }
    None
}

pub(crate) fn in_view_or_audience_guard_context(source: &str, position: Position) -> bool {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    let trimmed = line.trim_start();
    if trimmed.starts_with("policy ") {
        return enclosing_view_block(source, position).is_some()
            || enclosing_audience_block(source, position).is_some();
    }
    enclosing_view_block(source, position).is_some()
        || enclosing_audience_block(source, position).is_some()
}

pub(crate) fn in_guard_policy_child_context(source: &str, position: Position) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent >= cursor_indent {
            continue;
        }
        if trimmed.starts_with("policy ") {
            let pos = Position {
                line: idx as u32,
                character: indent as u32,
            };
            return in_view_or_audience_guard_context(source, pos);
        }
        return false;
    }
    false
}

pub(crate) fn enclosing_view_block(
    source: &str,
    position: Position,
) -> Option<RouteGuardViewBlock> {
    enclosing_named_block(source, position, "view")
}

pub(crate) fn enclosing_audience_block(
    source: &str,
    position: Position,
) -> Option<RouteGuardViewBlock> {
    enclosing_named_block(source, position, "audience")
}

pub(crate) fn enclosing_named_block(
    source: &str,
    position: Position,
    keyword: &str,
) -> Option<RouteGuardViewBlock> {
    let lines: Vec<&str> = source.lines().collect();
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let cursor_line = lines.get(cursor_line_idx).copied().unwrap_or("");
    let cursor_indent = leading_spaces(cursor_line);
    for idx in (0..=cursor_line_idx).rev() {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if idx == cursor_line_idx {
            if !trimmed.starts_with(&format!("{keyword} ")) {
                continue;
            }
        } else if indent >= cursor_indent {
            continue;
        }
        if trimmed.starts_with(&format!("{keyword} ")) {
            let end_line = find_block_end(&lines, idx, indent);
            return Some(RouteGuardViewBlock {
                header_line: idx,
                header_indent: indent,
                end_line,
                feature_hint: route_guard_context_feature(
                    source,
                    Position {
                        line: idx as u32,
                        character: indent as u32,
                    },
                ),
            });
        }
        if indent == 0 {
            return None;
        }
    }
    None
}

pub(crate) fn find_block_end(lines: &[&str], header_line: usize, header_indent: usize) -> usize {
    for (idx, line) in lines.iter().enumerate().skip(header_line + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) <= header_indent {
            return idx;
        }
    }
    lines.len()
}

pub(crate) fn resolve_policy_atoms(
    source: &str,
    feature_hint: Option<&str>,
    policy_ref: &str,
) -> Option<(Vec<String>, String)> {
    let name = policy_ref.strip_prefix("@policy.")?;
    let (feature, category) = if let Some((feature, category)) = name.split_once('.') {
        (Some(feature), category)
    } else {
        (feature_hint, name)
    };
    let mut current_feature: Option<String> = None;
    let mut in_policies = false;
    let mut policies_indent = 0;
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            in_policies = false;
            continue;
        }
        let feature_matches = match (feature, current_feature.as_deref()) {
            (Some(expected), Some(current)) => expected == current,
            (Some(_), None) => false,
            (None, Some(_)) => true,
            (None, None) => false,
        };
        if !feature_matches {
            continue;
        }
        if trimmed == "policies" || trimmed.starts_with("policies ") {
            in_policies = true;
            policies_indent = indent;
            continue;
        }
        if in_policies {
            if indent <= policies_indent {
                in_policies = false;
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix(&format!("{category}:")) {
                let atoms = rest
                    .split(',')
                    .map(str::trim)
                    .flat_map(|part| part.split_whitespace())
                    .filter(|token| token.starts_with('@'))
                    .map(|token| token.trim_end_matches(',').to_owned())
                    .collect::<Vec<_>>();
                let source_feature = current_feature.as_deref().unwrap_or("<unknown>");
                return Some((
                    atoms,
                    format!("`feature.{source_feature}.policies.{category}`"),
                ));
            }
        }
    }
    None
}

pub(crate) fn route_guard_backend_alignment(
    source: &str,
    position: Position,
    policy_ref: &str,
) -> String {
    let Some(view) = enclosing_view_block(source, position) else {
        return "No enclosing view found.".to_owned();
    };
    let hosted = hosted_backend_refs_for_view(source, &view);
    if hosted.is_empty() {
        return "No hosted `source` or `submit` backend found in this view.".to_owned();
    }
    let mut mismatches = Vec::new();
    for backend_ref in hosted {
        if let Some(backend_policy) = backend_policy_for_ref(source, &backend_ref) {
            if backend_policy == policy_ref {
                return format!(
                    "view hosts `{backend_ref}` (policy `{backend_policy}`); guard matches backend."
                );
            }
            mismatches.push(format!("`{backend_ref}` uses `{backend_policy}`"));
        }
    }
    if mismatches.is_empty() {
        "Hosted backend declarations have no local policy line in this document.".to_owned()
    } else {
        format!(
            "guard differs from hosted backend policy: {}.",
            mismatches.join(", ")
        )
    }
}

pub(crate) fn hosted_backend_refs_for_view(
    source: &str,
    view: &RouteGuardViewBlock,
) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut refs = Vec::new();
    for line in lines.iter().take(view.end_line).skip(view.header_line + 1) {
        let trimmed = line.trim_start();
        for prefix in ["source ", "submit "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let reference = rest.split_whitespace().next().unwrap_or("");
                let reference = reference.split('(').next().unwrap_or(reference);
                if reference.contains(".query.") || reference.contains(".command.") {
                    refs.push(reference.to_owned());
                }
            }
        }
    }
    refs
}

pub(crate) fn backend_policy_for_ref(source: &str, backend_ref: &str) -> Option<String> {
    let parts: Vec<&str> = backend_ref.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let feature = parts[0];
    let kind = parts[1];
    let name = parts[2];
    let lines: Vec<&str> = source.lines().collect();
    let mut in_feature = false;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_spaces(line);
        if indent == 0 {
            in_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == feature)
                .unwrap_or(false);
            continue;
        }
        if !in_feature || indent != 2 {
            continue;
        }
        let declaration_matches = match kind {
            "command" => trimmed
                .strip_prefix("command ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("") == name)
                .unwrap_or(false),
            "query" => ["query.list ", "query.lookup ", "query.sql ", "query.view "]
                .iter()
                .any(|prefix| {
                    trimmed
                        .strip_prefix(prefix)
                        .map(|rest| rest.split_whitespace().next().unwrap_or("") == name)
                        .unwrap_or(false)
                }),
            _ => false,
        };
        if !declaration_matches {
            continue;
        }
        let end = find_block_end(&lines, idx, indent);
        for child in lines.iter().take(end).skip(idx + 1) {
            let child_trimmed = child.trim_start();
            if let Some(rest) = child_trimmed.strip_prefix("policy ") {
                return Some(rest.split_whitespace().next().unwrap_or("").to_owned());
            }
        }
    }
    None
}
