//! Lifecycle synthesis tests — extracted from `lifecycle/mod.rs`
//! (Rails-style R9 split). Production code stays in `mod.rs`.

use super::*;

fn lower(source: &str) -> ir::Feature {
    let features = lazuli_syntax::parse_feature_skeletons(source).expect("parses");
    crate::lower_feature_skeleton(&features[0]).expect("lowers")
}

fn minimal_source(body: &str) -> String {
    format!(
        r#"
feature publication
  domain
    resource Publication
{}
"#,
        body
    )
}

#[test]
fn lowers_minimal_lifecycle_to_ir() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
    ));
    let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
    assert_eq!(lifecycle.discriminator_field, "status");
    assert_eq!(lifecycle.generated_enum, "PublicationStatus");
    assert_eq!(lifecycle.states.len(), 2);
    assert_eq!(lifecycle.transitions[0].name, "publish");
}

#[test]
fn auto_emits_generated_enum_with_correct_variants() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
    ));
    let en = feature
        .enums
        .iter()
        .find(|en| en.name == "PublicationStatus")
        .expect("enum");
    assert_eq!(en.variants[0].name, "Scheduled");
    assert_eq!(
        en.variants[0].storage_value,
        Some(ir::StorageValue::String("scheduled".to_owned()))
    );
}

#[test]
fn auto_emits_discriminator_field_if_missing() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
    ));
    let status = feature.resources[0]
        .fields
        .iter()
        .find(|f| f.name == "status")
        .expect("status field");
    assert!(status.required);
    assert!(matches!(status.type_ref, ir::TypeRef::EnumRef(_)));
}

#[test]
fn auto_emits_timestamps_field_if_missing() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
          timestamps published_at"#,
    ));
    let field = feature.resources[0]
        .fields
        .iter()
        .find(|f| f.name == "published_at")
        .expect("timestamp field");
    assert_eq!(
        field.type_ref,
        ir::TypeRef::Builtin(ir::BuiltinType::DateTime)
    );
    assert!(!field.required);
}

#[test]
fn existing_field_not_double_emitted() {
    let feature = lower(&minimal_source(
        r#"      status: PublicationStatus required
      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
    ));
    assert_eq!(
        feature.resources[0]
            .fields
            .iter()
            .filter(|f| f.name == "status")
            .count(),
        1
    );
}

#[test]
fn lowers_each_transition_to_command() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state publishing
        state published
        transition begin
          from scheduled
          to publishing
        transition publish
          from publishing
          to published"#,
    ));
    assert_eq!(feature.commands.len(), 2);
    assert!(
        feature
            .commands
            .iter()
            .all(|cmd| cmd.kind == ir::CommandKind::Update)
    );
    assert_eq!(feature.commands[0].name, "begin");
    assert_eq!(feature.commands[1].name, "publish");
}

#[test]
fn multi_source_transition_lowers_correctly() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state publishing
        state cancelled
        transition cancel
          from scheduled, publishing
          to cancelled"#,
    ));
    let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
    assert_eq!(
        lifecycle.transitions[0].from,
        vec!["scheduled".to_owned(), "publishing".to_owned()]
    );
    assert_eq!(feature.commands.len(), 1);
    assert_eq!(feature.commands[0].name, "cancel");
}

#[test]
fn invariant_terminal_immutable_lowers_to_enum_variant() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state published terminal
        transition publish
          from scheduled
          to published
        invariant terminal_immutable"#,
    ));
    let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
    assert_eq!(
        lifecycle.invariants,
        vec![ir::LifecycleInvariant::TerminalImmutable]
    );
}

#[test]
fn state_kind_intermediate_default() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published"#,
    ));
    let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
    assert_eq!(
        lifecycle.states[0].kind,
        ir::LifecycleStateKind::Intermediate
    );
}

#[test]
fn state_kind_initial_terminal_preserved() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled initial
        state published terminal
        transition publish
          from scheduled
          to published"#,
    ));
    let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
    assert_eq!(lifecycle.states[0].kind, ir::LifecycleStateKind::Initial);
    assert_eq!(lifecycle.states[1].kind, ir::LifecycleStateKind::Terminal);
}
