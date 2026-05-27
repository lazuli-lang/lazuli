//! Integration tests for the `lifecycle <field>` block as a child of
//! `resource`. The line walker dispatches to `lzi::lifecycle` —
//! these tests pin the resource-side wiring (validation, double-block
//! rejection, multi-`from` parsing). Lives as a sibling because the
//! resource/mod.rs file would otherwise exceed 500 LOC including its
//! own integration tests.

#![cfg(test)]

use super::super::{LifecycleInvariantForm, parse_feature_skeletons};

#[test]
fn parses_minimal_lifecycle_block() {
    let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let lifecycle = features[0].resources[0]
        .lifecycle
        .as_ref()
        .expect("lifecycle");

    assert_eq!(lifecycle.discriminator_field, "status");
    assert_eq!(lifecycle.states.len(), 2);
    assert_eq!(lifecycle.states[0].name, "scheduled");
    assert_eq!(lifecycle.states[1].name, "published");
    assert_eq!(lifecycle.transitions.len(), 1);
    assert_eq!(lifecycle.transitions[0].name, "publish");
    assert_eq!(lifecycle.transitions[0].from, vec!["scheduled"]);
    assert_eq!(lifecycle.transitions[0].to, "published");
}

#[test]
fn parses_lifecycle_with_terminal_states_and_invariants() {
    let source = r#"
feature publication
  domain
    resource Publication
      workspace: Workspace required
      scheduled_at: DateTime required
      publishing_at: DateTime
      published_at: DateTime
      failed_at: DateTime
      cancelled_at: DateTime
      error_reason: Text

      lifecycle status
        state scheduled initial
        state publishing
        state published terminal
        state failed terminal
        state cancelled terminal

        transition begin_publishing
          from scheduled
          to publishing
          policy @policy.publisher_or_admin
          audit default
          timestamps publishing_at

        transition mark_published
          from publishing
          to published
          audit default
          timestamps published_at
          emits publication_published

        transition mark_failed
          from publishing
          to failed
          audit error_reason
          timestamps failed_at
          emits publication_failed payload error_reason

        transition cancel
          from scheduled, publishing
          to cancelled
          audit default
          timestamps cancelled_at
          emits publication_cancelled

        invariant terminal_immutable
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let lifecycle = features[0].resources[0]
        .lifecycle
        .as_ref()
        .expect("lifecycle");

    assert_eq!(lifecycle.states[0].kind_keyword.as_deref(), Some("initial"));
    assert_eq!(
        lifecycle.states[2].kind_keyword.as_deref(),
        Some("terminal")
    );
    assert_eq!(lifecycle.invariants.len(), 1);
    assert_eq!(
        lifecycle.invariants[0].form,
        LifecycleInvariantForm::TerminalImmutable
    );
}

#[test]
fn lifecycle_rejects_fewer_than_two_states() {
    let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        transition publish
          from scheduled
          to published
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    let message = format!("{err}");

    assert!(
        message.contains("at least 2"),
        "error should require at least 2 states: {message}"
    );
}

#[test]
fn lifecycle_rejects_unknown_state_modifier() {
    let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled foo
        state published
        transition publish
          from scheduled
          to published
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    let message = format!("{err}");

    assert!(
        message.contains("initial") && message.contains("terminal"),
        "error should list valid state modifiers: {message}"
    );
}

#[test]
fn lifecycle_double_block_rejects() {
    let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
      lifecycle other_status
        state draft
        state archived
        transition archive
          from draft
          to archived
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    let message = format!("{err}");

    assert!(
        message.contains("at most one"),
        "error should reject duplicate lifecycle blocks: {message}"
    );
}

#[test]
fn lifecycle_rejects_unknown_invariant_form_at_parse_time() {
    // Closed-catalog discipline (`docs/proposals/lifecycle-vocab.md` §3.4):
    // `invariant <head>` must name a catalog form; ad-hoc identifiers like
    // `my_custom_rule` are rejected at parse time, NOT silently coerced
    // into `TerminalImmutable` downstream. This locks Cell B's parser
    // wiring — if the parser stops calling `parse_invariant_form`, the
    // analyzer's old silent fallback resurfaces and this test fires.
    let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        state published terminal
        transition publish
          from scheduled
          to published
        invariant my_custom_rule
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    let message = format!("{err}");

    assert!(
        message.contains("closed catalog"),
        "error should reject unknown invariant form via closed-catalog message: {message}"
    );
    assert!(
        message.contains("my_custom_rule"),
        "error should name the offending form: {message}"
    );
}

#[test]
fn lifecycle_rejects_predicate_style_invariant_at_parse_time() {
    // Per §3.4, no `where` clause is allowed — invariants are catalog-only,
    // no predicate sublanguage. Verifies the parser refuses to swallow a
    // workflow-style predicate expression.
    let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
        invariant single gold where item_id = parent.id
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    let message = format!("{err}");
    assert!(
        message.contains("closed catalog"),
        "error should reject predicate-style invariants: {message}"
    );
}

#[test]
fn lifecycle_parses_single_state_per_scope_invariant() {
    // Positive cell B test: `single <state> per <scope_field>` parses
    // into the typed `SingleStatePerScope` AST variant (no raw text).
    let source = r#"
feature pleiades
  domain
    resource ItemVersion
      lifecycle status
        state draft
        state gold
        transition approve
          from draft
          to gold
        invariant single gold per item_id
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let lifecycle = features[0].resources[0]
        .lifecycle
        .as_ref()
        .expect("lifecycle");
    match &lifecycle.invariants[0].form {
        LifecycleInvariantForm::SingleStatePerScope { state, scope_field } => {
            assert_eq!(state, "gold");
            assert_eq!(scope_field, "item_id");
        }
        other => panic!("expected SingleStatePerScope, got {:?}", other),
    }
}

#[test]
fn transition_multi_from_parsed() {
    let source = r#"
feature publication
  domain
    resource Publication
      lifecycle status
        state scheduled
        state publishing
        state cancelled
        transition cancel
          from scheduled, publishing
          to cancelled
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let lifecycle = features[0].resources[0]
        .lifecycle
        .as_ref()
        .expect("lifecycle");

    assert_eq!(
        lifecycle.transitions[0].from,
        vec!["scheduled", "publishing"]
    );
}
