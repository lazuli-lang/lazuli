//! Cell LSP-1 — completion, hover, and code-action coverage for the IR
//! Error-Vocab surface. See `docs/proposals/ir-error-messages-vocab.md`
//! §7 for the spec and §11 Cell LSP-1 for the implementation scope.
//!
//! These tests drive the public LSP-side surface directly (the same way
//! `auth.rs` exercises the auth bucket) so they pin the contract:
//! - keyword descriptions + rich hovers for `when_denied`, `errors`,
//!   `message_key`, and the 8 closed-catalog error codes.
//! - resolved-text hover for a code, walking the same-feature
//!   `translation` chain.
//! - completion firing at the 6 trigger positions (proposal §7.1).
//! - the 3 code actions (proposal §7.4).

use lazuli_lsp::{
    ERROR_VOCAB_CODES, ERROR_VOCAB_DEFAULT_VALUES, ERROR_VOCAB_EXPOSE_4XX_FIELDS,
    ERROR_VOCAB_EXPOSE_5XX_FIELDS, collect_translation_keys_for_feature,
    enclosing_feature_name, error_vocab_code_actions, error_vocab_code_builtin_en_us,
    error_vocab_code_detail, error_vocab_code_resolved_hover, error_vocab_completions,
    error_vocab_resolved_text, keyword_description, rich_keyword_hover,
};
use tower_lsp::lsp_types::{CodeActionOrCommand, CompletionItemKind, Position, Url};

// ── closed catalog & catalog-detail coverage ────────────────────────────────

#[test]
fn closed_catalog_codes_match_proposal() {
    // Proposal §2.C + DB-INTEGRITY-CATALOG-EXT (2026-05-19) — the
    // exact 12 codes the runtime ships in `error.go:142-156`.
    // Adding/removing one requires a proposal (Rule Zero). The order
    // here mirrors the canonical-semantics example.
    let expected = [
        "policy_denied",
        "validation_failed",
        "tenant_mismatch",
        "not_found",
        "rate_limited",
        "bad_request",
        "method_not_allowed",
        "integration_error",
        "unique_violation",
        "foreign_key_violation",
        "not_null_violation",
        "check_violation",
    ];
    assert_eq!(ERROR_VOCAB_CODES, &expected);
    for code in expected {
        assert!(
            error_vocab_code_detail(code).is_some(),
            "missing one-liner detail for `{code}`"
        );
        assert!(
            error_vocab_code_builtin_en_us(code).is_some(),
            "missing built-in en-US fallback for `{code}`"
        );
        assert!(
            keyword_description(code).is_some(),
            "missing keyword_description for `{code}` — needed for completion list detail"
        );
    }
}

#[test]
fn closed_catalog_4xx_fields_include_message_key() {
    // Proposal §2.G — `message_key` is the new opt-in field added on
    // top of the pre-existing 4xx catalog. 5xx deliberately excludes
    // `message`.
    assert!(ERROR_VOCAB_EXPOSE_4XX_FIELDS.contains(&"message"));
    assert!(ERROR_VOCAB_EXPOSE_4XX_FIELDS.contains(&"code"));
    assert!(ERROR_VOCAB_EXPOSE_4XX_FIELDS.contains(&"data"));
    assert!(ERROR_VOCAB_EXPOSE_4XX_FIELDS.contains(&"message_key"));
    assert!(ERROR_VOCAB_EXPOSE_5XX_FIELDS.contains(&"code"));
    assert!(ERROR_VOCAB_EXPOSE_5XX_FIELDS.contains(&"data"));
    assert!(
        !ERROR_VOCAB_EXPOSE_5XX_FIELDS.contains(&"message"),
        "`message` must NOT appear in the 5xx exposure catalog — see §2.C"
    );
}

#[test]
fn closed_catalog_default_values_are_hide_expose() {
    assert_eq!(ERROR_VOCAB_DEFAULT_VALUES, &["hide", "expose"]);
}

// ── hover coverage ──────────────────────────────────────────────────────────

#[test]
fn rich_hover_describes_when_denied() {
    let hover = rich_keyword_hover("when_denied").expect("when_denied hover present");
    // Proposal §7.2 — should explain the two attachment sites and the
    // resolution chain.
    assert!(
        hover.contains("per-command"),
        "expected per-command attachment site mentioned: {hover}"
    );
    assert!(
        hover.contains("per-policy"),
        "expected per-policy attachment site mentioned: {hover}"
    );
    assert!(
        hover.contains("@translation."),
        "expected @translation.<key> reference mentioned: {hover}"
    );
    // Doctor references signpost: at least one of the catalog codes.
    assert!(
        hover.contains("ERR-VOCAB"),
        "expected doctor diagnostic codes referenced: {hover}"
    );
}

#[test]
fn rich_hover_describes_errors_block() {
    let hover = rich_keyword_hover("errors").expect("errors hover present");
    // Should cover both exposure rules and per-code overrides.
    assert!(hover.contains("default hide"), "{hover}");
    assert!(hover.contains("expose client 4xx"), "{hover}");
    assert!(hover.contains("expose client 5xx"), "{hover}");
    for code in ERROR_VOCAB_CODES {
        assert!(
            hover.contains(code),
            "expected closed-catalog code `{code}` in `errors` hover: {hover}"
        );
    }
}

#[test]
fn rich_hover_describes_message_key() {
    let hover =
        rich_keyword_hover("message_key").expect("message_key hover present");
    assert!(hover.contains("4xx"), "{hover}");
    assert!(
        hover.contains("offline"),
        "expected offline-catalog rationale: {hover}"
    );
    assert!(
        hover.contains("expose client 4xx"),
        "{hover}"
    );
}

#[test]
fn each_closed_code_has_keyword_description() {
    // The completion list surfaces `keyword_description` as the `detail`
    // field — every code must therefore have a description so the user
    // sees an inline hint in the dropdown.
    for code in ERROR_VOCAB_CODES {
        let detail = keyword_description(code).unwrap_or_else(|| {
            panic!("missing keyword_description for `{code}` (needed by completion list)")
        });
        assert!(
            !detail.trim().is_empty(),
            "empty description for `{code}`"
        );
    }
}

#[test]
fn resolved_hover_reads_feature_level_override() {
    // Proposal §7.2 — when an `errors` block declares
    // `<code> message @translation.<key>`, the hover shows the
    // **resolved** text from the same-feature `translation` block.
    let source = sample_feature_with_errors_override();
    let position = position_at(&source, "policy_denied      message");
    let hover = error_vocab_code_resolved_hover(&source, position, "policy_denied")
        .expect("expected resolved hover when cursor sits on `policy_denied` inside errors block");

    // Resolved text comes from the `account_signin_required` key's en-US
    // variant in the fixture below.
    assert!(
        hover.contains("Please sign in to choose your role."),
        "hover did not include the resolved en-US text: {hover}"
    );
    // Source label points at the feature-level layer.
    assert!(
        hover.contains("account.errors.policy_denied")
            || hover.contains("feature.account"),
        "hover should label the resolution source as the feature.errors layer: {hover}"
    );
}

#[test]
fn resolved_hover_falls_back_to_builtin_when_no_override() {
    // When the feature has no override for the cursor's code, the hover
    // falls back to the shipped en-US catalog string (proposal §2.D). We
    // construct a fixture where `errors` lists a non-overriding sibling
    // (so the cursor lands inside the block) and the cursor sits on the
    // un-overridden `not_found` token.
    let source = "\
feature account
  errors
    default hide
    expose client 4xx code
    # cursor on `not_found` below — no @translation override for it.
    not_found
";
    let position = position_at(source, "not_found");
    let hover = error_vocab_code_resolved_hover(source, position, "not_found")
        .expect("expected resolved hover for `not_found`");
    assert!(
        hover.contains("We couldn't find the requested item."),
        "hover should fall back to the runtime en-US catalog: {hover}"
    );
    assert!(
        hover.contains("built-in"),
        "hover should signal the fallback source: {hover}"
    );
}

#[test]
fn resolved_hover_only_fires_inside_errors_block() {
    let source = "feature account\n  command list\n    policy @policy.read\n";
    let hover = error_vocab_code_resolved_hover(
        source,
        Position {
            line: 1,
            character: 0,
        },
        "policy_denied",
    );
    assert!(
        hover.is_none(),
        "resolved hover should NOT fire when the cursor is outside an `errors` block"
    );
}

// ── completion coverage ────────────────────────────────────────────────────

#[test]
fn completion_after_when_denied_offers_translation_keys() {
    // Proposal §7.1 row 1 — after `when_denied `, offer the local
    // feature's translation keys. The trailing space after `when_denied`
    // is real on the line; the test fixture includes it.
    let source = concat!(
        "feature account\n",
        "  policies\n",
        "    authenticated: @scope.authenticated\n",
        "      when_denied \n",
        "\n",
        "  translation\n",
        "    catalog \"./i18n/account.<locale>.json\"\n",
        "    key must_be_signed_in\n",
        "      en-US \"Please sign in first.\"\n",
        "    key host_only_action\n",
        "      en-US \"Host action.\"\n",
    );
    // Cursor sits right after `when_denied ` on the policy child line.
    let position = cursor_after(source, "when_denied ");
    let items = error_vocab_completions(source, position)
        .expect("expected completion items after `when_denied `");
    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    assert!(
        labels.contains(&"@translation.must_be_signed_in".to_owned()),
        "expected `@translation.must_be_signed_in` in completion list; got {labels:?}"
    );
    assert!(
        labels.contains(&"@translation.host_only_action".to_owned()),
        "expected `@translation.host_only_action` in completion list; got {labels:?}"
    );
}

#[test]
fn completion_inside_errors_block_offers_eight_codes() {
    // Proposal §7.1 row 4 — bare indented line inside `errors` offers the
    // 8 closed-catalog codes.
    let source = concat!(
        "feature account\n",
        "  errors\n",
        "    \n",
        "\n",
    );
    // Cursor on the third line, four spaces of indent (blank indented
    // line inside the `errors` block).
    let position = Position {
        line: 2,
        character: 4,
    };
    let items = error_vocab_completions(source, position)
        .expect("expected completion items inside `errors` block");
    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    for code in ERROR_VOCAB_CODES {
        assert!(
            labels.contains(&code.to_string()),
            "expected closed-catalog code `{code}` in completion list; got {labels:?}"
        );
    }
    // Every item should carry the `ENUM_MEMBER` kind tag and have a
    // detail string so the dropdown is informative.
    for item in &items {
        assert_eq!(item.kind, Some(CompletionItemKind::ENUM_MEMBER));
        assert!(item.detail.is_some());
    }
}

#[test]
fn completion_after_code_message_offers_translation_keys() {
    // Proposal §7.1 row 5 — after `<code> message ` inside `errors`,
    // offer `@translation.<key>` items.
    let source = concat!(
        "feature account\n",
        "  errors\n",
        "    policy_denied message \n",
        "\n",
        "  translation\n",
        "    catalog \"./i18n/account.<locale>.json\"\n",
        "    key account_signin_required\n",
        "      en-US \"Please sign in first.\"\n",
    );
    let position = cursor_after(source, "policy_denied message ");
    let items = error_vocab_completions(source, position)
        .expect("expected completion items after `policy_denied message `");
    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    assert!(
        labels.contains(&"@translation.account_signin_required".to_owned()),
        "expected `@translation.account_signin_required` in completion list; got {labels:?}"
    );
}

#[test]
fn completion_after_expose_client_4xx_offers_four_fields() {
    let source = concat!(
        "feature account\n",
        "  errors\n",
        "    expose client 4xx \n",
        "\n",
    );
    let position = cursor_after(source, "expose client 4xx ");
    let items = error_vocab_completions(source, position)
        .expect("expected completion items after `expose client 4xx `");
    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    assert_eq!(
        labels.iter().filter(|l| *l == "message").count(),
        1,
        "4xx must include `message`: {labels:?}"
    );
    assert!(labels.contains(&"message_key".to_string()), "{labels:?}");
    assert!(labels.contains(&"code".to_string()), "{labels:?}");
    assert!(labels.contains(&"data".to_string()), "{labels:?}");
}

#[test]
fn completion_after_expose_client_5xx_excludes_message() {
    let source = concat!(
        "feature account\n",
        "  errors\n",
        "    expose client 5xx \n",
        "\n",
    );
    let position = cursor_after(source, "expose client 5xx ");
    let items = error_vocab_completions(source, position)
        .expect("expected completion items after `expose client 5xx `");
    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    assert!(labels.contains(&"code".to_string()), "{labels:?}");
    assert!(labels.contains(&"data".to_string()), "{labels:?}");
    assert!(
        !labels.contains(&"message".to_string()),
        "5xx exposure must NOT offer `message` (proposal §2.C, §7.1 row 7): {labels:?}"
    );
    assert!(
        !labels.contains(&"message_key".to_string()),
        "5xx exposure must NOT offer `message_key`: {labels:?}"
    );
}

#[test]
fn completion_after_default_inside_errors_offers_hide_expose() {
    let source = concat!(
        "feature account\n",
        "  errors\n",
        "    default \n",
        "\n",
    );
    let position = cursor_after(source, "default ");
    let items = error_vocab_completions(source, position)
        .expect("expected completion items after `default ` inside `errors`");
    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    assert!(labels.contains(&"hide".to_string()), "{labels:?}");
    assert!(labels.contains(&"expose".to_string()), "{labels:?}");
}

// ── code action coverage ───────────────────────────────────────────────────

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

// ── helper coverage ────────────────────────────────────────────────────────

#[test]
fn enclosing_feature_name_walks_backwards() {
    let source = "\
feature account
  policies
    authenticated: @scope.authenticated
      when_denied @translation.must_be_signed_in
";
    // Cursor on the indent-6 child line.
    let feature = enclosing_feature_name(
        source,
        Position {
            line: 3,
            character: 8,
        },
    )
    .expect("expected feature lookup to succeed");
    assert_eq!(feature, "account");
}

#[test]
fn collect_translation_keys_for_feature_returns_local_keys() {
    let source = sample_feature_with_errors_override();
    let keys = collect_translation_keys_for_feature(&source, "account");
    assert!(
        keys.iter().any(|k| k == "account_signin_required"),
        "expected key `account_signin_required` in {keys:?}"
    );
}

#[test]
fn resolved_text_helper_returns_first_locale_variant() {
    let source = sample_feature_with_errors_override();
    let resolved = error_vocab_resolved_text(&source, "account", "policy_denied")
        .expect("expected resolved text for policy_denied");
    assert!(
        resolved.contains("Please sign in to choose your role."),
        "{resolved}"
    );
}

// ── fixtures ───────────────────────────────────────────────────────────────

fn sample_feature_with_errors_override() -> String {
    // Mirrors the proposal §2.E "Resolution example" feature, with one
    // per-code feature-level override (`policy_denied`) and one declared
    // translation key.
    [
        "feature account",
        "  policies",
        "    authenticated: @scope.authenticated",
        "      when_denied @translation.must_be_signed_in",
        "",
        "  errors",
        "    default hide",
        "    expose client 4xx message, code",
        "    expose client 5xx code",
        "    policy_denied      message @translation.account_signin_required",
        "",
        "  translation",
        "    catalog \"./i18n/account.<locale>.json\"",
        "    key account_signin_required",
        "      en-US \"Please sign in to choose your role.\"",
        "      pt-BR \"Para escolher seu papel, entre na sua conta primeiro.\"",
        "    key must_be_signed_in",
        "      en-US \"You need to sign in first.\"",
        "",
    ]
    .join("\n")
}

fn position_at(source: &str, needle: &str) -> Position {
    let line_idx = line_index_containing(source, needle);
    let line = source.lines().nth(line_idx).expect("line should exist");
    let column = line.find(needle).expect("needle should appear on the line");
    Position {
        line: line_idx as u32,
        character: column as u32,
    }
}

/// Return a cursor position one column past `needle` on the line where
/// `needle` first appears. Used to simulate the user having typed the
/// substring and pressed completion at the very end of it.
fn cursor_after(source: &str, needle: &str) -> Position {
    let line_idx = line_index_containing(source, needle);
    let line = source.lines().nth(line_idx).expect("line should exist");
    let column = line.find(needle).expect("needle should appear on the line");
    Position {
        line: line_idx as u32,
        character: (column + needle.len()) as u32,
    }
}

fn line_index_containing(source: &str, needle: &str) -> usize {
    source
        .lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| {
            panic!("source does not contain needle `{needle}`:\n{source}")
        })
}
