//! Code actions for the route-guard contract (proposal
//! `docs/proposals/route-guards-and-redirects.md`).
//!
//! Four quickfixes are exposed:
//!
//! 1. **Add route guard policy and redirects** — fires on a `view <name>`
//!    header line when the view doesn't already declare a `policy`.
//!    Picks a policy candidate from the backend refs the view hosts,
//!    falls back to the surrounding feature's `policies` block, and
//!    inserts `policy @policy.<name>` + the two `on_unauthenticated /
//!    on_unauthorized redirect` children at the view's body indent.
//! 2. **Promote `<path>` to `app.route_guard` default** — fires on a
//!    `on_unauthenticated redirect "<path>"` (or `on_unauthorized
//!    redirect "<path>"`) line when the same path is declared in 3+ view
//!    blocks AND no `app.route_guard.default_*_redirect` exists yet.
//!    Hoists the path into the central default.
//! 3. **Scaffold `app.route_guard` defaults** — fires inside `app`
//!    when an `actor_query` is declared but no defaults are set.
//!    Inserts `default_policy @scope.authenticated` plus the two
//!    default redirects.
//! 4. **Insert `actor_query account.query.me` stub** — fires inside
//!    `app` when an `app.route_guard` block exists but no `actor_query`
//!    sibling.
//!
//! All actions are pure text edits; no IR awareness. The module imports
//! shared helpers (`enclosing_view_block`, `simple_edit_action`,
//! `in_app_body_context`, etc.) from `lib.rs` because they're also used
//! by `route_guard_completions` and `route_guard_hover`.
//!
//! ## See also
//! * `lib.rs::RouteGuardViewBlock` — facts struct shared with completions
//!   and hover.
//! * `lib.rs::simple_edit_action` — shared single-edit builder, also used
//!   by `code_actions::lifecycle_gate`.
//! * `code_actions/error_vocab.rs` — sister scaffold for the error vocab
//!   contract.

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Position, Range, TextEdit, Url,
};

use crate::{
    RouteGuardViewBlock, app_route_guard_block, backend_policy_for_ref,
    collect_policy_categories_for_feature, enclosing_view_block, first_quoted_value,
    hosted_backend_refs_for_view, in_app_body_context, leading_spaces, position_at_line_start,
    simple_edit_action,
};

pub fn route_guard_code_actions(
    source: &str,
    uri: &Url,
    position: Position,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = (position.line as usize).min(lines.len().saturating_sub(1));
    let line = lines.get(line_idx).copied().unwrap_or("");
    let trimmed = line.trim_start();

    if trimmed.starts_with("view ") {
        if let Some(action) = build_scaffold_view_guard_action(source, uri, line_idx) {
            actions.push(action.into());
        }
    }

    if trimmed.starts_with("on_unauthenticated redirect ")
        || trimmed.starts_with("on_unauthorized redirect ")
    {
        if let Some(action) = build_promote_redirect_default_action(source, uri, line_idx) {
            actions.push(action.into());
        }
    }

    if in_app_body_context(source, position) {
        if has_actor_query(source) && !route_guard_has_default_redirects(source) {
            if let Some(action) = build_scaffold_route_guard_defaults_action(source, uri) {
                actions.push(action.into());
            }
        }
        if app_route_guard_block(source).is_some() && !has_actor_query(source) {
            if let Some(action) = build_insert_actor_query_action(source, uri) {
                actions.push(action.into());
            }
        }
    }

    actions
}

pub(crate) fn build_scaffold_view_guard_action(
    source: &str,
    uri: &Url,
    view_line_idx: usize,
) -> Option<CodeAction> {
    let view = enclosing_view_block(
        source,
        Position {
            line: view_line_idx as u32,
            character: 0,
        },
    )?;
    if view_has_policy(source, &view) {
        return None;
    }
    let policy = hosted_backend_refs_for_view(source, &view)
        .into_iter()
        .find_map(|backend_ref| backend_policy_for_ref(source, &backend_ref))
        .or_else(|| {
            collect_policy_categories_for_feature(source, view.feature_hint.as_deref())
                .into_iter()
                .next()
                .map(|name| format!("@policy.{name}"))
        })
        .unwrap_or_else(|| "@policy.<name>".to_owned());
    let (unauthenticated, unauthorized) = route_guard_default_redirects(source);
    let child_indent = " ".repeat(view.header_indent + 2);
    let redirect_indent = " ".repeat(view.header_indent + 4);
    let new_text = format!(
        "{child_indent}policy {policy}\n{redirect_indent}on_unauthenticated redirect \"{}\"\n{redirect_indent}on_unauthorized redirect \"{}\"\n",
        unauthenticated.unwrap_or_else(|| "/sign-in".to_owned()),
        unauthorized.unwrap_or_else(|| "/403".to_owned()),
    );
    let insertion_line = view_guard_insertion_line(source, &view);
    Some(simple_edit_action(
        uri,
        "Add route guard policy and redirects",
        CodeActionKind::QUICKFIX,
        vec![TextEdit {
            range: Range {
                start: position_at_line_start(insertion_line),
                end: position_at_line_start(insertion_line),
            },
            new_text,
        }],
        true,
    ))
}

pub(crate) fn view_has_policy(source: &str, view: &RouteGuardViewBlock) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    lines
        .iter()
        .take(view.end_line)
        .skip(view.header_line + 1)
        .any(|line| {
            leading_spaces(line) == view.header_indent + 2
                && line.trim_start().starts_with("policy ")
        })
}

pub(crate) fn view_guard_insertion_line(source: &str, view: &RouteGuardViewBlock) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    for (idx, line) in lines
        .iter()
        .enumerate()
        .take(view.end_line)
        .skip(view.header_line + 1)
    {
        if leading_spaces(line) == view.header_indent + 2 && line.trim_start().starts_with("path ")
        {
            return idx + 1;
        }
    }
    view.header_line + 1
}

pub(crate) fn build_promote_redirect_default_action(
    source: &str,
    uri: &Url,
    line_idx: usize,
) -> Option<CodeAction> {
    let line = source.lines().nth(line_idx)?;
    let trimmed = line.trim_start();
    let (source_keyword, default_keyword) = if trimmed.starts_with("on_unauthenticated redirect ") {
        ("on_unauthenticated", "default_unauthenticated_redirect")
    } else if trimmed.starts_with("on_unauthorized redirect ") {
        ("on_unauthorized", "default_unauthorized_redirect")
    } else {
        return None;
    };
    let path = first_quoted_value(trimmed)?;
    if count_view_redirects(source, source_keyword, &path) < 3 {
        return None;
    }
    if app_route_guard_has_default(source, default_keyword) {
        return None;
    }
    let edit = insert_route_guard_default_edit(source, default_keyword, &path)?;
    Some(simple_edit_action(
        uri,
        &format!("Promote `{path}` to app.route_guard default"),
        CodeActionKind::REFACTOR_REWRITE,
        vec![edit],
        false,
    ))
}

pub(crate) fn count_view_redirects(source: &str, keyword: &str, path: &str) -> usize {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with(&format!("{keyword} redirect "))
                && first_quoted_value(trimmed).as_deref() == Some(path)
        })
        .count()
}

pub(crate) fn app_route_guard_has_default(source: &str, default_keyword: &str) -> bool {
    let Some(block) = app_route_guard_block(source) else {
        return false;
    };
    source
        .lines()
        .take(block.end_line)
        .skip(block.header_line + 1)
        .any(|line| line.trim_start().starts_with(default_keyword))
}

pub(crate) fn insert_route_guard_default_edit(
    source: &str,
    default_keyword: &str,
    path: &str,
) -> Option<TextEdit> {
    if let Some(block) = app_route_guard_block(source) {
        let insertion = block.end_line;
        let indent = " ".repeat(block.header_indent + 2);
        return Some(TextEdit {
            range: Range {
                start: position_at_line_start(insertion),
                end: position_at_line_start(insertion),
            },
            new_text: format!("{indent}{default_keyword} \"{path}\"\n"),
        });
    }
    let app_line = app_header_line(source)?;
    Some(TextEdit {
        range: Range {
            start: position_at_line_start(app_line + 1),
            end: position_at_line_start(app_line + 1),
        },
        new_text: format!("  route_guard\n    {default_keyword} \"{path}\"\n"),
    })
}

pub(crate) fn build_scaffold_route_guard_defaults_action(
    source: &str,
    uri: &Url,
) -> Option<CodeAction> {
    let edit = if let Some(block) = app_route_guard_block(source) {
        TextEdit {
            range: Range {
                start: position_at_line_start(block.end_line),
                end: position_at_line_start(block.end_line),
            },
            new_text: "    default_policy @scope.authenticated\n    default_unauthenticated_redirect \"/sign-in\"\n    default_unauthorized_redirect \"/403\"\n".to_owned(),
        }
    } else {
        let actor_line = source
            .lines()
            .position(|line| line.trim_start().starts_with("actor_query "))?;
        TextEdit {
            range: Range {
                start: position_at_line_start(actor_line + 1),
                end: position_at_line_start(actor_line + 1),
            },
            new_text: "  route_guard\n    default_policy @scope.authenticated\n    default_unauthenticated_redirect \"/sign-in\"\n    default_unauthorized_redirect \"/403\"\n".to_owned(),
        }
    };
    Some(simple_edit_action(
        uri,
        "Scaffold app.route_guard defaults",
        CodeActionKind::QUICKFIX,
        vec![edit],
        true,
    ))
}

pub(crate) fn build_insert_actor_query_action(source: &str, uri: &Url) -> Option<CodeAction> {
    let insertion_line = app_route_guard_block(source)
        .map(|block| block.header_line)
        .unwrap_or(app_header_line(source)? + 1);
    let edit = TextEdit {
        range: Range {
            start: position_at_line_start(insertion_line),
            end: position_at_line_start(insertion_line),
        },
        new_text: "  actor_query account.query.me\n".to_owned(),
    };
    Some(simple_edit_action(
        uri,
        "Insert `actor_query account.query.me` stub",
        CodeActionKind::QUICKFIX,
        vec![edit],
        true,
    ))
}

pub(crate) fn has_actor_query(source: &str) -> bool {
    source
        .lines()
        .any(|line| line.trim_start().starts_with("actor_query "))
}

pub(crate) fn route_guard_has_default_redirects(source: &str) -> bool {
    let (unauthenticated, unauthorized) = route_guard_default_redirects(source);
    unauthenticated.is_some() || unauthorized.is_some()
}

pub(crate) fn app_header_line(source: &str) -> Option<usize> {
    source
        .lines()
        .position(|line| leading_spaces(line) == 0 && line.trim_start().starts_with("app "))
}

pub(crate) fn route_guard_default_redirects(source: &str) -> (Option<String>, Option<String>) {
    let Some(block) = app_route_guard_block(source) else {
        return (None, None);
    };
    let mut unauthenticated = None;
    let mut unauthorized = None;
    for line in source
        .lines()
        .take(block.end_line)
        .skip(block.header_line + 1)
    {
        let trimmed = line.trim_start();
        if trimmed.starts_with("default_unauthenticated_redirect ")
            || trimmed.starts_with("on_unauthenticated redirect ")
        {
            unauthenticated = first_quoted_value(trimmed);
        } else if trimmed.starts_with("default_unauthorized_redirect ")
            || trimmed.starts_with("on_unauthorized redirect ")
        {
            unauthorized = first_quoted_value(trimmed);
        }
    }
    (unauthenticated, unauthorized)
}
