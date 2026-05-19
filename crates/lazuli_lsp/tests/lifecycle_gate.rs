//! Cell LSP-1 — lifecycle route-gate completion, hover, and code-action coverage.

use lazuli_lsp::{
    keyword_description, lifecycle_gate_code_actions, lifecycle_gate_completions,
    lifecycle_gate_hover,
};
use tower_lsp::lsp_types::{CodeActionOrCommand, Position, Url};

fn cursor_after(source: &str, needle: &str) -> Position {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("needle `{needle}` not found"))
        + needle.len();
    position_for_offset(source, offset)
}

fn cursor_after_last(source: &str, needle: &str) -> Position {
    let offset = source
        .rfind(needle)
        .unwrap_or_else(|| panic!("needle `{needle}` not found"))
        + needle.len();
    position_for_offset(source, offset)
}

fn position_for_offset(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut character = 0u32;
    for ch in source[..offset].chars() {
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }
    Position { line, character }
}

fn labels_at(source: &str, needle: &str) -> Vec<String> {
    lifecycle_gate_completions(source, cursor_after(source, needle))
        .unwrap_or_else(|| panic!("expected lifecycle completions after `{needle}`"))
        .into_iter()
        .map(|item| item.label)
        .collect()
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

fn fixture() -> String {
    [
        "feature account",
        "  domain",
        "    resource Account",
        "      lifecycle status",
        "        state invited",
        "        state active",
        "        transition activate",
        "          from invited",
        "          to active",
        "  query.lookup account_profile by id: ID",
        "    returns Account",
        "",
        "experience account",
        "  view account_home",
        "    path /account",
        "  resume account_onboarding",
        "    source query.lookup account_profile",
        "    none → view account_home",
        "    invited → view account_home",
        "    active → view account_home",
        "",
        "feature host",
        "  uses account",
        "  policies",
        "    host_only: @scope.authenticated, @role.host",
        "  domain",
        "    resource Host",
        "      lifecycle status",
        "        state intermediation_terms_pending",
        "        state basic_details_pending",
        "        state address_pending",
        "        state languages_pending",
        "        state complete",
        "        transition save_basic",
        "          from intermediation_terms_pending",
        "          to basic_details_pending",
        "  query.lookup my_host by actor_id: ID",
        "    policy @policy.host_only",
        "    returns Host",
        "  command save_host",
        "    updates Host",
        "",
        "feature billing",
        "  domain",
        "    resource Invoice",
        "      lifecycle status",
        "        state draft",
        "        state paid",
        "        transition pay",
        "          from draft",
        "          to paid",
        "  query.lookup invoice by id: ID",
        "    returns Invoice",
        "",
        "experience host",
        "  uses account",
        "  view host_home",
        "    path /host",
        "    policy @policy.host_only",
        "    source host.query.my_host",
        "    requires_lifecycle Host = complete",
        "    on_lifecycle_pending @resume host_onboarding",
        "",
        "  view host_onboarding_intermediation",
        "    path /onboarding/host/intermediation",
        "    policy @policy.host_only",
        "",
        "  view host_onboarding_basic_details",
        "    path /onboarding/host/basic-details",
        "    policy @policy.host_only",
        "    submit host.command.save_host",
        "    ",
        "",
        "  view host_onboarding_address",
        "    path /onboarding/host/address",
        "    policy @policy.host_only",
        "",
        "  resume host_onboarding",
        "    source query.lookup my_host",
        "    none → view host_onboarding_intermediation",
        "    intermediation_terms_pending → view host_onboarding_intermediation",
        "    ghost_pending → view host_onboarding_address",
        "",
    ]
    .join("\n")
}

#[test]
fn completion_covers_all_nine_lifecycle_gate_positions() {
    let source = fixture();

    let mut resource_source = source.clone();
    resource_source = resource_source.replace(
        "    requires_lifecycle Host = complete",
        "    requires_lifecycle ",
    );
    assert_eq!(
        sorted(labels_at(&resource_source, "requires_lifecycle ")),
        vec!["Account".to_owned(), "Host".to_owned()]
    );

    let mut state_source = source.clone();
    state_source = state_source.replace(
        "    requires_lifecycle Host = complete",
        "    requires_lifecycle Host = ",
    );
    assert_eq!(
        sorted(labels_at(&state_source, "requires_lifecycle Host = ")),
        vec![
            "address_pending",
            "basic_details_pending",
            "complete",
            "intermediation_terms_pending",
            "languages_pending",
        ]
    );

    let mut resume_ref_source = source.clone();
    resume_ref_source = resume_ref_source.replace(
        "    on_lifecycle_pending @resume host_onboarding",
        "    on_lifecycle_pending @resume ",
    );
    assert_eq!(
        sorted(labels_at(
            &resume_ref_source,
            "on_lifecycle_pending @resume "
        )),
        vec![
            "account.account_onboarding".to_owned(),
            "host_onboarding".to_owned()
        ]
    );

    let arm_labels = labels_at(&source, "source query.lookup my_host\n");
    for expected in [
        "none",
        "basic_details_pending",
        "address_pending",
        "languages_pending",
        "complete",
        "*",
    ] {
        assert!(
            arm_labels.contains(&expected.to_owned()),
            "missing `{expected}` in {arm_labels:?}"
        );
    }

    for arm_prefix in [
        "    address_pending ",
        "    address_pending ->",
        "    address_pending →",
    ] {
        let source = format!("{}\n{arm_prefix}", fixture());
        assert_eq!(labels_at(&source, arm_prefix), vec!["view ".to_owned()]);
    }

    let source_with_target = format!("{}\n    address_pending → view ", fixture());
    let view_labels = labels_at(&source_with_target, "address_pending → view ");
    assert!(
        view_labels.contains(&"host_home".to_owned()),
        "{view_labels:?}"
    );
    assert!(
        view_labels.contains(&"host_onboarding_address".to_owned()),
        "{view_labels:?}"
    );
    assert!(
        !view_labels.contains(&"account_home".to_owned()),
        "{view_labels:?}"
    );

    let header_source = "experience host\n  resume host_onboarding";
    assert_eq!(
        labels_at(header_source, "resume host_onboarding"),
        vec!["source query.lookup <q>".to_owned()]
    );

    let lookup_source = source.replace("source query.lookup my_host", "source query.lookup ");
    let lookup_labels = lifecycle_gate_completions(
        &lookup_source,
        cursor_after_last(&lookup_source, "source query.lookup "),
    )
    .expect("lookup query completions")
    .into_iter()
    .map(|item| item.label)
    .collect();
    assert_eq!(
        sorted(lookup_labels),
        vec!["account.account_profile".to_owned(), "my_host".to_owned()]
    );

    let view_slot_labels = labels_at(&source, "    submit host.command.save_host\n    ");
    assert_eq!(
        view_slot_labels,
        vec![
            "requires_lifecycle ".to_owned(),
            "on_lifecycle_pending @resume ".to_owned()
        ]
    );
}

#[test]
fn hover_catalog_covers_lifecycle_keywords_and_resume_tokens() {
    assert_eq!(
        keyword_description("requires_lifecycle"),
        Some(
            "Gate this view on the actor's `<Resource>.lifecycle_state`. Codegen emits a TanStack `beforeLoad` that fetches the source query and redirects via `@resume` on mismatch."
        )
    );
    assert_eq!(
        keyword_description("on_lifecycle_pending"),
        Some(
            "Name of the `resume <name>` block to redirect through when `requires_lifecycle` doesn't match."
        )
    );
    assert_eq!(
        keyword_description("resume"),
        Some(
            "Block declaring how to route a user whose lifecycle state of a particular resource is mid-flow."
        )
    );

    let source = fixture();
    let source_hover = lifecycle_gate_hover(
        &source,
        cursor_after_last(&source, "source"),
        Some("source"),
    )
    .expect("source query.lookup hover");
    assert!(
        source_hover.contains("Must return a single record OR not-found (404)"),
        "{source_hover}"
    );

    let none_hover =
        lifecycle_gate_hover(&source, cursor_after(&source, "none"), Some("none")).unwrap();
    assert!(
        none_hover.contains("source query returns 404"),
        "{none_hover}"
    );

    let star_source = format!("{}\n    * → view host_home", fixture());
    let star_hover = lifecycle_gate_hover(&star_source, cursor_after(&star_source, "*"), None)
        .expect("wildcard hover");
    assert!(star_hover.contains("Catch-all arm"), "{star_hover}");

    let ascii_arrow_source = format!("{}\n    address_pending -> view host_home", fixture());
    let ascii_arrow_hover = lifecycle_gate_hover(
        &ascii_arrow_source,
        cursor_after(&ascii_arrow_source, "->"),
        None,
    )
    .expect("ascii arrow hover");
    assert!(
        ascii_arrow_hover.contains("Both Unicode `→` and ASCII `->` accepted"),
        "{ascii_arrow_hover}"
    );

    let unicode_arrow_hover =
        lifecycle_gate_hover(&source, cursor_after(&source, "→"), None).expect("unicode arrow");
    assert!(
        unicode_arrow_hover.contains("Arrow token mapping a lifecycle state arm"),
        "{unicode_arrow_hover}"
    );
}

#[test]
fn code_actions_cover_lifecycle_gate_quick_fixes() {
    let source = fixture();
    let uri = Url::parse("file:///host.lzx").unwrap();

    let resume_actions = lifecycle_gate_code_actions(
        &source,
        &uri,
        cursor_after(&source, "ghost_pending → view host_onboarding_address"),
    );
    let titles: Vec<_> = resume_actions
        .iter()
        .filter_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) => Some(action.title.clone()),
            _ => None,
        })
        .collect();
    for expected in [
        "Add missing state arms",
        "Remove stale arm",
        "Convert to wildcard",
    ] {
        assert!(titles.contains(&expected.to_owned()), "{titles:?}");
    }

    let add_missing = code_action(&resume_actions, "Add missing state arms");
    let inserted = only_inserted_text(add_missing, &uri);
    assert!(
        inserted.contains("basic_details_pending → view <TODO>"),
        "{inserted}"
    );
    assert!(
        inserted.contains("address_pending → view <TODO>"),
        "{inserted}"
    );

    let gate_actions = lifecycle_gate_code_actions(
        &source,
        &uri,
        cursor_after(&source, "view host_onboarding_basic_details"),
    );
    let add_gate = code_action(&gate_actions, "Add lifecycle gate");
    let inserted = only_inserted_text(add_gate, &uri);
    assert!(
        inserted.contains("requires_lifecycle Host = basic_details_pending"),
        "{inserted}"
    );
    assert!(
        inserted.contains("on_lifecycle_pending @resume host_onboarding"),
        "{inserted}"
    );
}

fn code_action<'a>(
    actions: &'a [CodeActionOrCommand],
    title: &str,
) -> &'a tower_lsp::lsp_types::CodeAction {
    actions
        .iter()
        .find_map(|action| match action {
            CodeActionOrCommand::CodeAction(action) if action.title == title => Some(action),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing code action `{title}`"))
}

fn only_inserted_text(action: &tower_lsp::lsp_types::CodeAction, uri: &Url) -> String {
    action
        .edit
        .as_ref()
        .and_then(|edit| edit.changes.as_ref())
        .and_then(|changes| changes.get(uri))
        .expect("expected workspace edit")
        .iter()
        .map(|edit| edit.new_text.as_str())
        .collect::<String>()
}
