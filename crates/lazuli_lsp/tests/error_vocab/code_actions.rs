//! Code action coverage — the 3 actions from proposal §7.4.

use lazuli_lsp::{ERROR_VOCAB_CODES, error_vocab_code_actions};
use tower_lsp::lsp_types::{CodeActionOrCommand, Position, Url};

#[test]
fn code_action_scaffold_errors_block_inserts_all_codes() {
    // Proposal §7.4 row 2 — cursor on a feature header without an
    // `errors` block surfaces "Scaffold `errors` block with all <n> codes".
    // DB-INTEGRITY-CATALOG-EXT (2026-05-19): the catalog grew to 12; the
    // iteration over `ERROR_VOCAB_CODES` covers whatever the canon ships.
    let source = "\
feature account
  command list
    policy @policy.read
    returns Customer
";
    let position = Position {
        line: 0,
        character: 0,
    };
    let uri = Url::parse("file:///account.lzi").unwrap();
    let actions = error_vocab_code_actions(source, &uri, position);
    let scaffold = actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca)
                if ca.title.contains("Scaffold `errors` block") =>
            {
                Some(ca)
            }
            _ => None,
        })
        .expect("expected scaffold-errors code action when feature has no `errors` block");
    let edit = scaffold.edit.as_ref().expect("scaffold action must carry an edit");
    let changes = edit
        .changes
        .as_ref()
        .expect("scaffold action must use `changes`");
    let edits = changes.get(&uri).expect("scaffold action must target the active URI");
    assert_eq!(edits.len(), 1, "expected one edit, got {edits:?}");
    let inserted = &edits[0].new_text;
    for code in ERROR_VOCAB_CODES {
        assert!(
            inserted.contains(code),
            "inserted scaffold must include `{code}`: {inserted}"
        );
    }
    // The inserted text must contain the exposure rules and a
    // translation block.
    assert!(inserted.contains("default hide"), "{inserted}");
    assert!(inserted.contains("expose client 4xx message, code"), "{inserted}");
    assert!(inserted.contains("expose client 5xx code"), "{inserted}");
    assert!(inserted.contains("translation"), "{inserted}");
}

#[test]
fn code_action_scaffold_omits_when_errors_block_already_present() {
    let source = "\
feature account
  errors
    default hide
  command list
    policy @policy.read
    returns Customer
";
    let position = Position {
        line: 0,
        character: 0,
    };
    let uri = Url::parse("file:///account.lzi").unwrap();
    let actions = error_vocab_code_actions(source, &uri, position);
    assert!(
        !actions.iter().any(|a| match a {
            CodeActionOrCommand::CodeAction(ca) =>
                ca.title.contains("Scaffold `errors` block"),
            _ => false,
        }),
        "scaffold action should NOT fire when feature already has an `errors` block"
    );
}

#[test]
fn code_action_add_when_denied_on_policy_entry() {
    // Proposal §7.4 row 1 — cursor on a `policies.<category>:` line
    // without a `when_denied` child should surface the add-action.
    let source = "\
feature account
  policies
    authenticated: @scope.authenticated
";
    // Cursor on `authenticated: @scope.authenticated`.
    let position = Position {
        line: 2,
        character: 6,
    };
    let uri = Url::parse("file:///account.lzi").unwrap();
    let actions = error_vocab_code_actions(source, &uri, position);
    let add_action = actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca)
                if ca.title.contains("per-policy default") =>
            {
                Some(ca)
            }
            _ => None,
        })
        .expect("expected per-policy add-when_denied code action");
    let edit = add_action.edit.as_ref().expect("must carry an edit");
    let changes = edit.changes.as_ref().expect("`changes` field required");
    let edits = changes.get(&uri).expect("must target the active URI");
    assert_eq!(edits.len(), 1);
    let inserted = &edits[0].new_text;
    assert!(
        inserted.contains("when_denied @translation."),
        "inserted line must declare a typed translation reference: {inserted}"
    );
    // Inserted at indent +2 from the parent (parent indent is 4, child
    // is 6).
    assert!(
        inserted.starts_with("      when_denied"),
        "inserted line must be indented by 6 spaces: {inserted:?}"
    );
}

#[test]
fn code_action_add_when_denied_on_command_policy_line() {
    // Proposal §7.4 row 3 — cursor on a `command.policy @policy.<name>`
    // line without a `when_denied` child surfaces the add-action.
    let source = "\
feature account
  command choose_role
    policy @policy.authenticated
    returns Customer
";
    let position = Position {
        line: 2,
        character: 4,
    };
    let uri = Url::parse("file:///account.lzi").unwrap();
    let actions = error_vocab_code_actions(source, &uri, position);
    let add_action = actions
        .iter()
        .find_map(|a| match a {
            CodeActionOrCommand::CodeAction(ca)
                if ca.title.contains("per-command override") =>
            {
                Some(ca)
            }
            _ => None,
        })
        .expect("expected per-command add-when_denied code action");
    let edit = add_action.edit.as_ref().expect("must carry an edit");
    let changes = edit.changes.as_ref().expect("`changes` field required");
    let edits = changes.get(&uri).expect("must target the active URI");
    let inserted = &edits[0].new_text;
    // The stub key includes the command name.
    assert!(
        inserted.contains("choose_role"),
        "stub key should include command name: {inserted}"
    );
    // Child indent = parent indent (4) + 2 = 6.
    assert!(
        inserted.starts_with("      when_denied"),
        "expected 6-space indent on child line: {inserted:?}"
    );
}

#[test]
fn code_action_skipped_when_when_denied_already_present() {
    let source = "\
feature account
  policies
    authenticated: @scope.authenticated
      when_denied @translation.must_be_signed_in
";
    let position = Position {
        line: 2,
        character: 6,
    };
    let uri = Url::parse("file:///account.lzi").unwrap();
    let actions = error_vocab_code_actions(source, &uri, position);
    assert!(
        !actions.iter().any(|a| match a {
            CodeActionOrCommand::CodeAction(ca) =>
                ca.title.contains("per-policy default")
                    || ca.title.contains("per-command override"),
            _ => false,
        }),
        "add-when_denied action should NOT fire when a `when_denied` child already exists"
    );
}
