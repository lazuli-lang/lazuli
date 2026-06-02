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

/// G13 false-positive regression (2026-06-02): a lifecycle transition
/// whose inline `tests` block uses the `allows when` / `denies when`
/// predicate form must lower to a NON-EMPTY `TestBlock` (each `when`
/// line becomes a `TestAssertion::Raw`). Before the fix, lifecycle had
/// its own private `lower_test_line` that lacked the `when` → `Raw`
/// fallback, so these lines dropped to nothing, the block lowered to
/// `None`, and `VOCAB-TESTS-MISSING-001` false-fired. Both transition
/// call sites now route through `crate::test_lowering::lower_test_block`.
#[test]
fn transition_when_predicate_tests_lower_to_non_empty_block() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
          tests
            allows when input.note contains "ok"
            denies when input.note is_empty"#,
    ));
    let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
    let tests = lifecycle.transitions[0]
        .tests
        .as_ref()
        .expect("when-predicate transition tests must lower to Some(TestBlock), not None");
    assert_eq!(tests.assertions.len(), 2, "{:?}", tests.assertions);
    assert!(
        tests
            .assertions
            .iter()
            .all(|a| matches!(a, ir::TestAssertion::Raw { .. })),
        "when-form lines lower to the Raw fallback: {:?}",
        tests.assertions
    );
}

/// The typed transition forms (`allows from <state> [as <actor>]`)
/// must still lower to their typed variants after routing through the
/// unified lowering — guards against a regression in the migration.
#[test]
fn transition_typed_from_tests_still_lower_typed() {
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state scheduled
        state published
        transition publish
          from scheduled
          to published
          tests
            allows from scheduled
            denies from published"#,
    ));
    let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
    let tests = lifecycle.transitions[0].tests.as_ref().expect("tests");
    assert_eq!(
        tests.assertions[0],
        ir::TestAssertion::AllowsFrom {
            state: "scheduled".to_owned()
        }
    );
    assert_eq!(
        tests.assertions[1],
        ir::TestAssertion::DeniesFrom {
            state: "published".to_owned()
        }
    );
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
fn invariant_single_state_per_scope_lowers_to_typed_variant() {
    // Cell B contract: the analyzer maps the parser's typed
    // `LifecycleInvariantForm::SingleStatePerScope` directly into the IR
    // — no string sniffing, no silent fallback. If the parser ever loses
    // the typed form (regressing to raw text), this test still pins the
    // analyzer's typed-only signature; if the analyzer regrows the old
    // string-based `lower_invariant`, this assertion stops matching the
    // scope_field tail.
    let feature = lower(&minimal_source(
        r#"      lifecycle status
        state draft
        state gold
        transition approve
          from draft
          to gold
        invariant single gold per item_id"#,
    ));
    let lifecycle = feature.resources[0].lifecycle.as_ref().expect("lifecycle");
    assert_eq!(
        lifecycle.invariants,
        vec![ir::LifecycleInvariant::SingleStatePerScope {
            state: "gold".to_owned(),
            scope_field: "item_id".to_owned(),
        }]
    );
}

#[test]
fn invariant_unknown_form_never_silently_coerces() {
    // Rule Zero pin: the architect audit found the analyzer used to
    // silently coerce ANY unknown invariant form to `TerminalImmutable`.
    // Cell B closed the gap by rejecting unknown forms at parse time —
    // so a `.lzi` with `invariant my_custom_rule` must fail BEFORE
    // reaching the analyzer. If it ever parses, the lifecycle will NOT
    // carry a `TerminalImmutable` invariant for the bogus input.
    let source = minimal_source(
        r#"      lifecycle status
        state draft
        state published
        transition publish
          from draft
          to published
        invariant my_custom_rule"#,
    );
    let result = lazuli_syntax::parse_feature_skeletons(&source);
    let err = result.expect_err("unknown invariant form must reject at parse time");
    let message = format!("{err}");
    assert!(
        message.contains("closed catalog"),
        "parser must reject via closed-catalog message: {message}"
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
