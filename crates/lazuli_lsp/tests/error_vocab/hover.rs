//! Hover coverage — rich keyword hovers + resolved code hovers.

use lazuli_lsp::{
    ERROR_VOCAB_CODES, error_vocab_code_resolved_hover, rich_keyword_hover,
};
use tower_lsp::lsp_types::Position;

use super::{position_at, sample_feature_with_errors_override};

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
