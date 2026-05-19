//! Cell LSP-1 — route-guard completion, hover, and code-action coverage.

use lazuli_lsp::{route_guard_code_actions, route_guard_completions, route_guard_hover};
use tower_lsp::lsp_types::{CodeActionOrCommand, CompletionItemKind, Position, Url};

fn cursor_after(source: &str, needle: &str) -> Position {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("needle `{needle}` not found"))
        + needle.len();
    let mut line = 0u32;
    let mut character = 0u32;
    for ch in source[..offset].chars() {
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    Position { line, character }
}

fn cursor_after_last(source: &str, needle: &str) -> Position {
    let offset = source
        .rfind(needle)
        .unwrap_or_else(|| panic!("needle `{needle}` not found"))
        + needle.len();
    let mut line = 0u32;
    let mut character = 0u32;
    for ch in source[..offset].chars() {
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    Position { line, character }
}

fn labels(source: &str, position: Position) -> Vec<String> {
    route_guard_completions(source, position)
        .expect("expected route-guard completions")
        .into_iter()
        .map(|item| item.label)
        .collect()
}

fn fixture() -> String {
    [
        "app HostPoint",
        "  actor_query ",
        "  route_guard",
        "    default_policy @scope.authenticated",
        "    default_unauthenticated_redirect \"/sign-in\"",
        "    default_unauthorized_redirect \"/403\"",
        "",
        "route public_login",
        "  path \"/sign-in\"",
        "  to account.view.login",
        "",
        "route forbidden",
        "  path \"/403\"",
        "  to account.view.forbidden",
        "",
        "feature account",
        "  policies",
        "    host_only: @scope.authenticated, @role.host",
        "    public: @scope.public",
        "",
        "  query.lookup me by id: ID",
        "    policy @policy.host_only",
        "    returns Account",
        "",
        "experience account",
        "  view home",
        "    path \"/host\"",
        "    policy ",
        "    source account.query.me",
        "",
    ]
    .join("\n")
}

#[test]
fn completion_after_view_policy_offers_local_policy_refs() {
    let source = fixture();
    let position = cursor_after_last(&source, "    policy ");
    let labels = labels(&source, position);

    assert!(
        labels.contains(&"@policy.host_only".to_owned()),
        "{labels:?}"
    );
    assert!(labels.contains(&"@policy.public".to_owned()), "{labels:?}");
}

#[test]
fn completion_after_view_redirect_offers_declared_route_paths() {
    let source = format!("{}{}", fixture(), "        on_unauthenticated redirect \n",);
    let position = cursor_after(&source, "on_unauthenticated redirect ");
    let labels = labels(&source, position);

    assert!(labels.contains(&"/sign-in".to_owned()), "{labels:?}");
    assert!(labels.contains(&"/403".to_owned()), "{labels:?}");
}

#[test]
fn completion_after_app_default_redirect_offers_declared_route_paths() {
    let source = fixture().replace(
        "default_unauthenticated_redirect \"/sign-in\"",
        "default_unauthenticated_redirect ",
    );
    let position = cursor_after(&source, "default_unauthenticated_redirect ");
    let labels = labels(&source, position);

    assert!(labels.contains(&"/sign-in".to_owned()), "{labels:?}");
    assert!(labels.contains(&"/403".to_owned()), "{labels:?}");
}

#[test]
fn completion_after_actor_query_offers_feature_query_refs() {
    let source = fixture();
    let position = cursor_after(&source, "actor_query ");
    let labels = labels(&source, position);

    assert!(
        labels.contains(&"account.query.me".to_owned()),
        "{labels:?}"
    );
}

#[test]
fn completion_inside_app_route_guard_offers_three_default_clauses() {
    let source = "app HostPoint\n  route_guard\n    \n";
    let items = route_guard_completions(
        source,
        Position {
            line: 2,
            character: 4,
        },
    )
    .expect("expected route_guard clause completions");
    let labels: Vec<_> = items.iter().map(|item| item.label.clone()).collect();

    for expected in [
        "default_policy",
        "default_unauthenticated_redirect",
        "default_unauthorized_redirect",
    ] {
        assert!(labels.contains(&expected.to_owned()), "{labels:?}");
    }
    assert!(
        items
            .iter()
            .all(|item| item.kind == Some(CompletionItemKind::SNIPPET))
    );
}

#[test]
fn hover_on_view_policy_ref_shows_atoms_and_backend_alignment() {
    let source = fixture().replace(
        "\n    policy \n    source",
        "\n    policy @policy.host_only\n    source",
    );
    let position = cursor_after_last(&source, "@policy.");
    let hover = route_guard_hover(&source, position, "policy.host_only")
        .expect("expected route guard policy ref hover");

    assert!(hover.contains("@scope.authenticated"), "{hover}");
    assert!(hover.contains("@role.host"), "{hover}");
    assert!(hover.contains("guard matches backend"), "{hover}");
}

#[test]
fn code_action_inserts_actor_query_stub_when_route_guard_has_no_actor_query() {
    let source = "app HostPoint\n  route_guard\n    default_policy @scope.authenticated\n";
    let uri = Url::parse("file:///app.lzi").unwrap();
    let actions = route_guard_code_actions(
        source,
        &uri,
        Position {
            line: 0,
            character: 0,
        },
    );
    let action = actions
        .iter()
        .find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action)
                if action.title.contains("actor_query account.query.me") =>
            {
                Some(action)
            }
            _ => None,
        })
        .expect("expected actor_query stub code action");
    let edits = action
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.as_ref())
        .and_then(|changes| changes.get(&uri))
        .expect("expected workspace edit");

    assert_eq!(edits.len(), 1);
    assert!(edits[0].new_text.contains("actor_query account.query.me"));
}
