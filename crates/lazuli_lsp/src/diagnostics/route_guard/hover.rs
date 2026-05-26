//! Hover text + backend-alignment lookup for the IR Route-Guards LSP layer.
//!
//! `route_guard_hover` is the public entry point — the rest is the
//! supporting walk that resolves `@policy.<name>` references to their
//! atom set and compares the guard against any hosted command/query
//! `policy` lines for the enclosing view.

use tower_lsp::lsp_types::Position;

use crate::leading_spaces;

use super::context::{
    enclosing_audience_block, enclosing_view_block, find_block_end,
    in_view_or_audience_guard_context, route_guard_context_feature, RouteGuardViewBlock,
};

/// Public hover entry point for the IR Route-Guards LSP layer. Returns
/// a Markdown string for tokens in route-guard contexts — `policy`,
/// `on_unauthenticated`, `on_unauthorized`, `route_guard`,
/// `actor_query`, `default_unauthenticated_redirect`,
/// `default_unauthorized_redirect`, and resolved `policy.<name>`
/// references that walk the document's `policies` block and compare
/// the guard against any hosted backend's `policy` line. Returns
/// `None` for any other token.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_for_route_guard_keyword() {
        // `route_guard` itself is a context-free keyword hover — returns
        // text even with no surrounding view block context.
        let hover = route_guard_hover("", Position { line: 0, character: 0 }, "route_guard");
        assert!(hover.is_some());
        assert!(hover.unwrap().contains("route_guard"));
    }

    #[test]
    fn returns_none_for_unrelated_word() {
        assert!(route_guard_hover("", Position { line: 0, character: 0 }, "nonsense").is_none());
    }
}
