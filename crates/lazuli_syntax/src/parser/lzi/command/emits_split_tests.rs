//! Regression tests for bug F1-emits-multi-event — `emits a, b` must
//! split into one [`CommandEmit`] per name at the PARSE layer.
//!
//! Before the fix, `parse_command_emit` never split on `,`: the whole
//! `"subscription_activated, subscription_status_changed"` string became
//! one `CommandEmit.name`, which the analyzer carried 1:1 into
//! `ir::Command.emits[0]` and both Go emitters rendered as a single
//! bogus `{Name: "a, b", …}` EventEmit matching no declared event. The
//! split now happens before the AST is built, so every downstream layer
//! (analyzer, emitters, doctor VOCAB-EVENT-PAYLOAD-001) sees N names.
//!
//! Co-located with `command/mod.rs` as a sibling per the ≤500-LOC rule.

#![cfg(test)]

use super::super::parse_feature_skeletons;
use crate::ast::CommandEffectKindDecl;

/// The headline regression: a two-name `emits` line yields two separate
/// `CommandEmit`s, each carrying exactly one trimmed name.
#[test]
fn emits_two_names_splits_into_two_command_emits() {
    let source = r#"feature billing
  command activate
    policy @policy.update
    updates Subscription
    emits subscription_activated, subscription_status_changed
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let emits = &features[0].commands[0].emits;
    assert_eq!(emits.len(), 2, "`emits a, b` must produce two CommandEmits");
    assert_eq!(emits[0].name, "subscription_activated");
    assert_eq!(emits[1].name, "subscription_status_changed");
    // No stray comma or whitespace leaked into either name.
    assert!(emits.iter().all(|e| !e.name.contains(',')));
}

/// Extra interior whitespace around the comma is trimmed; three names work.
#[test]
fn emits_three_names_with_loose_whitespace_trims_each() {
    let source = r#"feature billing
  command activate
    policy @policy.update
    updates Subscription
    emits a ,  b,c
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let names: Vec<&str> = features[0].commands[0]
        .emits
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(names, vec!["a", "b", "c"]);
}

/// A trailing ` from <kind>` axis binds to *every* split name.
#[test]
fn emits_from_axis_applies_to_all_split_names() {
    let source = r#"feature billing
  command activate
    policy @policy.update
    creates Subscription
    emits created, status_changed from creates
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let emits = &features[0].commands[0].emits;
    assert_eq!(emits.len(), 2);
    assert_eq!(emits[0].name, "created");
    assert_eq!(emits[1].name, "status_changed");
    assert!(
        emits
            .iter()
            .all(|e| e.from == Some(CommandEffectKindDecl::Creates)),
        "the `from creates` axis must apply to every name on the line"
    );
}

/// The single-name form is unchanged — still one `CommandEmit`, and its
/// optional payload child block still attaches.
#[test]
fn single_emit_with_payload_child_still_parses() {
    let source = r#"feature billing
  command reassign
    policy @policy.update
    updates Subscription
    emits subscription_reassigned
      to_owner_id = input.owner_id
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let emits = &features[0].commands[0].emits;
    assert_eq!(emits.len(), 1);
    assert_eq!(emits[0].name, "subscription_reassigned");
    assert_eq!(emits[0].fields.len(), 1);
    assert_eq!(emits[0].fields[0].field, "to_owner_id");
}

/// Edge case from the bug report: a multi-name `emits` carrying a payload
/// child block is ambiguous (which event gets the binds?). The simplest
/// correct rule rejects it — require single-name emits for bound payloads.
#[test]
fn multi_name_emit_with_payload_child_is_a_parse_error() {
    let source = r#"feature billing
  command activate
    policy @policy.update
    updates Subscription
    emits a, b
      to_owner_id = input.owner_id
"#;
    let err = parse_feature_skeletons(source).expect_err(
        "multi-name `emits` with a payload child block must be rejected, not silently guessed",
    );
    assert!(
        format!("{err}").contains("multi-name `emits`"),
        "error should explain the multi-name + payload-block conflict, got: {err}"
    );
}

/// An `emits` line with only commas / whitespace is still rejected — the
/// split must not turn `emits ,` into zero events that silently parse.
#[test]
fn emits_only_commas_is_a_parse_error() {
    let source = r#"feature billing
  command activate
    policy @policy.update
    updates Subscription
    emits ,
"#;
    assert!(parse_feature_skeletons(source).is_err());
}
