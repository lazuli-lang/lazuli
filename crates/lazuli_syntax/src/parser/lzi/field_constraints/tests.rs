//! Inline field-constraint integration tests. Co-located with
//! `field_constraints/mod.rs` as a sibling because the inline test
//! block alone pushes the parent past the 500-LOC ceiling.

#![cfg(test)]

use super::super::parse_feature_skeletons;

/// `key: Text required min 2 max 80 pattern "^[a-z0-9-]+$"` —
/// the canonical proposal §10 example. Constraints stack with
/// `required` modifier; type_text remains `Text`.
#[test]
fn resource_field_text_min_max_pattern() {
    let source = r#"
feature slug
  domain
    resource Slug
      key: Text required min 2 max 80 pattern "^[a-z0-9-]+$"
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let field = &features[0].resources[0].fields[0];
    assert_eq!(field.name, "key");
    assert_eq!(field.type_text, "Text");
    assert!(field.required);
    assert_eq!(field.constraints.min, Some(2));
    assert_eq!(field.constraints.max, Some(80));
    assert_eq!(field.constraints.pattern.as_deref(), Some("^[a-z0-9-]+$"));
}

/// `between A and B` on Integer parses as a two-tuple.
#[test]
fn resource_field_integer_between() {
    let source = r#"
feature person
  domain
    resource Person
      age: Integer between 0 and 150
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let field = &features[0].resources[0].fields[0];
    assert_eq!(field.name, "age");
    assert_eq!(field.constraints.between, Some((0, 150)));
    assert!(field.constraints.min.is_none());
    assert!(field.constraints.max.is_none());
}

/// `in ["admin", "editor", "viewer"]` on Text parses the
/// list and strips surrounding quotes.
#[test]
fn resource_field_text_in_list() {
    let source = r#"
feature acl
  domain
    resource Member
      role: Text in ["admin", "editor", "viewer"]
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let field = &features[0].resources[0].fields[0];
    assert_eq!(field.name, "role");
    assert_eq!(
        field.constraints.r#in.as_deref(),
        Some(&["admin".to_owned(), "editor".to_owned(), "viewer".to_owned()][..])
    );
}

/// `length N` on Text captures exact length.
#[test]
fn resource_field_text_length() {
    let source = r#"
feature post
  domain
    resource Post
      title: Text length 120
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let field = &features[0].resources[0].fields[0];
    assert_eq!(field.constraints.length, Some(120));
}

/// Constraints before the default literal parse correctly.
#[test]
fn resource_field_constraints_before_default() {
    let source = r#"
feature counter
  domain
    resource Counter
      score: Integer min 0 max 100 = 50
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let field = &features[0].resources[0].fields[0];
    assert_eq!(field.constraints.min, Some(0));
    assert_eq!(field.constraints.max, Some(100));
    assert_eq!(field.default.as_deref(), Some("50"));
}

/// Command input slots pick up the same constraint catalog.
#[test]
fn command_input_slot_min_max_pattern() {
    let source = r#"
feature slug
  command create
    policy @policy.create
    input
      key: Text required min 2 max 80 pattern "^[a-z]+$"
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let cmd = &features[0].commands[0];
    let slots = match &cmd.input {
        crate::CommandInputDecl::Typed(s) => s,
        _ => panic!("expected typed input"),
    };
    assert_eq!(slots[0].name, "key");
    assert_eq!(slots[0].constraints.min, Some(2));
    assert_eq!(slots[0].constraints.max, Some(80));
    assert_eq!(slots[0].constraints.pattern.as_deref(), Some("^[a-z]+$"));
    assert!(slots[0].required);
}

#[test]
fn command_write_window_parses_duration_literal() {
    let source = r#"
feature billing
  command create_invoice
    input customer, issued_at
    write_window by input.issued_at within 30d
    policy @policy.create
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let write_window = features[0].commands[0]
        .write_window
        .as_ref()
        .expect("write_window");
    assert_eq!(write_window.by, "input.issued_at");
    assert_eq!(write_window.within, "30d");
}

#[test]
fn command_write_window_requires_by() {
    let source = r#"
feature billing
  command create_invoice
    write_window input.issued_at within 30d
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    assert!(err.to_string().contains("write_window"));
}

#[test]
fn command_write_window_requires_within() {
    let source = r#"
feature billing
  command create_invoice
    write_window by input.issued_at
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    assert!(err.to_string().contains("within"));
}

#[test]
fn command_triggers_transition_parses_canonical_and_legacy_shapes() {
    let source = r#"
feature order
  command submit
    triggers transition approve
  command fulfill
    triggers transition approve, capture_payment, ship
  command legacy_inline
    triggers approve, capture_payment
  command legacy_block
    triggers
      transition approve
      transition capture_payment, ship
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    assert_eq!(features[0].commands[0].triggers, vec!["approve".to_owned()]);
    assert_eq!(
        features[0].commands[1].triggers,
        vec![
            "approve".to_owned(),
            "capture_payment".to_owned(),
            "ship".to_owned()
        ]
    );
    assert_eq!(
        features[0].commands[2].triggers,
        vec!["approve".to_owned(), "capture_payment".to_owned()]
    );
    assert_eq!(
        features[0].commands[3].triggers,
        vec![
            "approve".to_owned(),
            "capture_payment".to_owned(),
            "ship".to_owned()
        ]
    );

    let trailing = r#"
feature order
  command broken
    triggers transition approve,
"#;
    let err = parse_feature_skeletons(trailing).unwrap_err();
    assert!(err.to_string().contains("empty entry"));
}
