//! Completion coverage — the 6 trigger positions from proposal §7.1.

use lazuli_lsp::{ERROR_VOCAB_CODES, error_vocab_completions};
use tower_lsp::lsp_types::{CompletionItemKind, Position};

use super::cursor_after;

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
    let source = concat!("feature account\n", "  errors\n", "    \n", "\n",);
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
    let source = concat!("feature account\n", "  errors\n", "    default \n", "\n",);
    let position = cursor_after(source, "default ");
    let items = error_vocab_completions(source, position)
        .expect("expected completion items after `default ` inside `errors`");
    let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
    assert!(labels.contains(&"hide".to_string()), "{labels:?}");
    assert!(labels.contains(&"expose".to_string()), "{labels:?}");
}
