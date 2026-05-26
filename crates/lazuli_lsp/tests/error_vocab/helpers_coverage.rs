//! Public helper functions — `enclosing_feature_name`,
//! `collect_translation_keys_for_feature`, `error_vocab_resolved_text`.

use lazuli_lsp::{
    collect_translation_keys_for_feature, enclosing_feature_name, error_vocab_resolved_text,
};
use tower_lsp::lsp_types::Position;

use super::sample_feature_with_errors_override;

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
