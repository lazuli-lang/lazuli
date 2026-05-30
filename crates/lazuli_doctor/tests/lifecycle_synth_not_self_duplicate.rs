//! Bug B regression — a `lifecycle` auto-synthesizes its discriminator
//! enum + field into the feature/resource. The duplicate-detection rules
//! `LIFECYCLE-ENUM-DUPLICATE` and `LIFECYCLE-FIELD-DOUBLE-DECLARED` must
//! NOT flag those synthesized entries as duplicates of their own
//! synthesized origin; only a genuine AUTHOR-declared duplicate should
//! fire. End-to-end: parse `.lzi` -> lower (runs synthesis) -> run the
//! two rules over the lowered IR.

use std::path::Path;

use lazuli_analyzer::lower_feature_skeleton;
use lazuli_doctor::lifecycle::{enum_duplicate, field_double_declared};
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
