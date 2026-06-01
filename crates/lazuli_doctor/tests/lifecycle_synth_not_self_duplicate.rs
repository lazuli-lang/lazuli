//! Bug B regression — a `lifecycle` auto-synthesizes its discriminator
//! enum + field into the feature/resource. The duplicate-detection rules
//! `LIFECYCLE-ENUM-DUPLICATE` and `LIFECYCLE-FIELD-DOUBLE-DECLARED` must
//! NOT flag those synthesized entries as duplicates of their own
//! synthesized origin; only a genuine AUTHOR-declared duplicate should
//! fire. End-to-end: parse `.lzi` -> lower (runs synthesis) -> run the
//! two rules over the lowered IR.

use std::path::Path;

use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor::lifecycle::{enum_duplicate, field_double_declared, state_set_undeclared_001};
use lazuli_syntax::parse_feature_skeletons;

fn lower(source: &str) -> lazuli_ir::Feature {
    let skeletons = parse_feature_skeletons(source).expect("parse");
    lower_feature_skeleton(&skeletons[0]).expect("lower")
}

/// A single-resource feature whose lifecycle synthesizes `OrderStatus` +
/// `status` (the author declared neither) must produce zero
/// `LIFECYCLE-ENUM-DUPLICATE` / `LIFECYCLE-FIELD-DOUBLE-DECLARED`
/// findings. Before the fix each rule fired once, flagging the synth
/// enum/field against itself.
#[test]
fn lifecycle_synth_enum_and_field_are_not_self_duplicates() {
    let source = r#"
feature shop
  resource Order
    number: Text required
    lifecycle status
      state draft initial
      state paid terminal
      transition pay
        from draft
        to paid
"#;
    let feature = lower(source);

    let enum_findings = enum_duplicate::check(&feature, Path::new("shop.lzi"));
    assert!(
        enum_findings.is_empty(),
        "synthesized lifecycle enum must not be flagged as a duplicate of itself: {enum_findings:?}"
    );

    let field_findings = field_double_declared::check(&feature, Path::new("shop.lzi"));
    assert!(
        field_findings.is_empty(),
        "synthesized discriminator field must not be flagged as double-declared against itself: {field_findings:?}"
    );
}

/// Guard against an over-broad fix: a GENUINE author-declared `enum
/// OrderStatus` (under the feature's `domain` block) colliding with the
/// lifecycle-generated enum of the same name must STILL fire
/// `LIFECYCLE-ENUM-DUPLICATE`. (The synth pass skips emitting its own
/// enum when the author already declared one, so the surviving entry
/// carries the author's distinct span — a real collision.)
#[test]
fn author_declared_enum_collision_still_fires() {
    let source = r#"
feature shop
  domain
    enum OrderStatus
      Draft
      Paid
  resource Order
    number: Text required
    lifecycle status
      state draft initial
      state paid terminal
      transition pay
        from draft
        to paid
"#;
    let feature = lower(source);
    let findings = enum_duplicate::check(&feature, Path::new("shop.lzi"));
    assert_eq!(
        findings.len(),
        1,
        "an author-declared enum colliding with the generated lifecycle enum is a real duplicate: {findings:?}"
    );
    assert_eq!(findings[0].enum_name, "OrderStatus");
}

/// Spec 0017 `synth_origin_no_regression` — binding the closed `state` set
/// into the existing lifecycle synth must not re-trip the synth-origin
/// false-positives the traveler waivers document: a real authored lifecycle
/// (whose states ARE declared) lowers through synthesis and stays silent for
/// `LIFECYCLE-ENUM-DUPLICATE`, `-FIELD-DOUBLE-DECLARED`, AND the new
/// `LIFECYCLE-STATE-SET-UNDECLARED-001` (the declared set exists, so it is
/// not enum-by-command). This is the traveler shape end-to-end.
#[test]
fn synth_origin_no_regression_declared_state_set_is_clean() {
    let source = r#"
feature traveler
  resource Traveler
    name: Text required
    lifecycle lifecycle_state
      state basic_details_pending initial
      state review_pending
      state active terminal
      transition advance_to_review
        from basic_details_pending
        to review_pending
      transition activate
        from review_pending
        to active
"#;
    let feature = lower(source);

    assert!(
        enum_duplicate::check(&feature, Path::new("traveler.lzi")).is_empty(),
        "synth lifecycle enum must not self-duplicate (traveler waiver root cause)"
    );
    assert!(
        field_double_declared::check(&feature, Path::new("traveler.lzi")).is_empty(),
        "synth discriminator field must not self-double-declare (traveler waiver root cause)"
    );
    assert!(
        state_set_undeclared_001::check(&feature, Path::new("traveler.lzi")).is_empty(),
        "a lifecycle with a DECLARED closed state set is not enum-by-command — must stay silent"
    );
}
