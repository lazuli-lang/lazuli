//! Cell LSP-1 coverage for auth.sessions refresh-token rotation.

use lazuli_lsp::{auth_refresh_code_actions, auth_refresh_completions, keyword_description};
use tower_lsp::lsp_types::{CodeActionOrCommand, CompletionItemKind, Position, Url};

#[test]
fn completion_after_access_ttl_offers_access_duration_literals() {
    let source = concat!(
        "feature account\n",
        "  auth\n",
        "    sessions\n",
        "      resource UserSession\n",
        "      access_ttl \n",
    );
    let position = cursor_after(source, "access_ttl ");
    let items =
        auth_refresh_completions(source, position).expect("expected completion after access_ttl");
    let labels = labels(&items);
    assert!(labels.contains(&"\"15 minutes\"".to_owned()), "{labels:?}");
    assert!(labels.contains(&"\"1 hour\"".to_owned()), "{labels:?}");
}

#[test]
fn completion_after_rotation_line_offers_scaffold_snippet() {
    let source = concat!(
        "feature account\n",
        "  auth\n",
        "    sessions\n",
        "      resource UserSession\n",
        "      rotation\n",
    );
    let position = cursor_after(source, "rotation");
    let items = auth_refresh_completions(source, position)
        .expect("expected completion after bare rotation line");
    let scaffold = items
        .iter()
        .find(|item| item.label == "scaffold rotation block")
        .expect("expected scaffold snippet");
    assert_eq!(scaffold.kind, Some(CompletionItemKind::SNIPPET));
    let insert_text = scaffold.insert_text.as_deref().unwrap_or("");
    assert!(
        insert_text.contains("refresh_ttl \"30 days\""),
        "{insert_text}"
    );
    assert!(
        insert_text.contains("grace \"30 seconds\""),
        "{insert_text}"
    );
    assert!(
        insert_text.contains("theft_detection_action revoke_session_family"),
        "{insert_text}"
    );
}

#[test]
fn rotation_block_completions_cover_inner_clauses_durations_and_theft_actions() {
    let source = concat!(
        "feature account\n",
        "  auth\n",
        "    sessions\n",
        "      resource UserSession\n",
        "      rotation\n",
        "        \n",
        "        refresh_ttl \n",
        "        grace \n",
        "        theft_detection_action \n",
    );

    let clause_items = auth_refresh_completions(
        source,
        Position {
            line: 5,
            character: 8,
        },
    )
    .expect("expected inner rotation clause completions");
    let clause_labels = labels(&clause_items);
    assert!(
        clause_labels
            .iter()
            .any(|label| label.starts_with("refresh_ttl")),
        "{clause_labels:?}"
    );
    assert!(
        clause_labels.iter().any(|label| label.starts_with("grace")),
        "{clause_labels:?}"
    );
    assert!(
        clause_labels
            .iter()
            .any(|label| label.starts_with("theft_detection_action")),
        "{clause_labels:?}"
    );

    let refresh_items = auth_refresh_completions(source, cursor_after(source, "refresh_ttl "))
        .expect("expected refresh_ttl duration completions");
    assert!(
        labels(&refresh_items).contains(&"\"30 days\"".to_owned()),
        "{refresh_items:?}"
    );

    let grace_items = auth_refresh_completions(source, cursor_after(source, "grace "))
        .expect("expected grace duration completions");
    assert!(
        labels(&grace_items).contains(&"\"30 seconds\"".to_owned()),
        "{grace_items:?}"
    );

    let theft_items =
        auth_refresh_completions(source, cursor_after(source, "theft_detection_action "))
            .expect("expected theft action completions");
    let theft_labels = labels(&theft_items);
    assert!(
        theft_labels.contains(&"revoke_session_family".to_owned()),
        "{theft_labels:?}"
    );
    assert!(
        theft_labels.contains(&"revoke_user".to_owned()),
        "{theft_labels:?}"
    );
    for item in &theft_items {
        assert_eq!(item.kind, Some(CompletionItemKind::ENUM_MEMBER));
    }
}

#[test]
fn hovers_document_auth_refresh_keywords() {
    for (keyword, expected) in [
        ("access_ttl", "15 minutes"),
        ("rotation", "parent_session_id"),
        ("refresh_ttl", "30 days"),
        ("grace", "30 seconds"),
        ("theft_detection_action", "revoke_session_family"),
    ] {
        let hover =
            keyword_description(keyword).unwrap_or_else(|| panic!("missing hover for {keyword}"));
        assert!(hover.contains(expected), "{keyword}: {hover}");
        assert!(
            hover.contains("default") || hover.contains("Default"),
            "{keyword}: {hover}"
        );
    }
}

#[test]
fn code_action_promotes_single_token_sessions_to_rotation_defaults() {
    let source = concat!(
        "feature account\n",
        "  auth\n",
        "    sessions\n",
        "      resource UserSession\n",
        "      ttl \"7 days\"\n",
    );
    let uri = Url::parse("file:///account.lzi").unwrap();
    let actions = auth_refresh_code_actions(
        source,
        &uri,
        Position {
            line: 2,
            character: 4,
        },
    );
    let action = action_by_title(&actions, "Promote single-token to rotation");
    let inserted = only_inserted_text(action, &uri);
    assert!(inserted.contains("access_ttl \"15 minutes\""), "{inserted}");
    assert!(inserted.contains("rotation"), "{inserted}");
    assert!(inserted.contains("refresh_ttl \"30 days\""), "{inserted}");
    assert!(inserted.contains("grace \"30 seconds\""), "{inserted}");
    assert!(
        inserted.contains("theft_detection_action revoke_session_family"),
        "{inserted}"
    );
    assert!(inserted.contains("framework default"), "{inserted}");
}

#[test]
fn code_action_scaffolds_empty_rotation_line() {
    let source = concat!(
        "feature account\n",
        "  auth\n",
        "    sessions\n",
        "      resource UserSession\n",
        "      rotation\n",
    );
    let uri = Url::parse("file:///account.lzi").unwrap();
    let actions = auth_refresh_code_actions(
        source,
        &uri,
        Position {
            line: 4,
            character: 6,
        },
    );
    let action = action_by_title(&actions, "Scaffold rotation block");
    let inserted = only_inserted_text(action, &uri);
    assert!(inserted.contains("refresh_ttl \"30 days\""), "{inserted}");
    assert!(inserted.contains("grace \"30 seconds\""), "{inserted}");
    assert!(
        inserted.contains("theft_detection_action revoke_session_family"),
        "{inserted}"
    );
}

fn action_by_title<'a>(
    actions: &'a [CodeActionOrCommand],
    title: &str,
) -> &'a tower_lsp::lsp_types::CodeAction {
    actions
        .iter()
        .find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) if action.title == title => Some(action),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing action `{title}` in {actions:?}"))
}

fn only_inserted_text(action: &tower_lsp::lsp_types::CodeAction, uri: &Url) -> String {
    let edit = action.edit.as_ref().expect("code action must carry edit");
    let changes = edit.changes.as_ref().expect("edit must use changes");
    let edits = changes.get(uri).expect("edit must target active URI");
    assert_eq!(edits.len(), 1, "expected one edit: {edits:?}");
    edits[0].new_text.clone()
}

fn labels(items: &[tower_lsp::lsp_types::CompletionItem]) -> Vec<String> {
    items.iter().map(|item| item.label.clone()).collect()
}

fn cursor_after(source: &str, needle: &str) -> Position {
    let line_idx = source
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("source does not contain `{needle}`:\n{source}"));
    let line = source.lines().nth(line_idx).expect("line exists");
    let column = line.find(needle).expect("needle is on line") + needle.len();
    Position {
        line: line_idx as u32,
        character: column as u32,
    }
}
