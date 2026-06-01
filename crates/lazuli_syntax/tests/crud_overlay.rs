//! Spec 0018 — `crud` overlay block parser tests.
//!
//! Covers the `crud` child block under a `conventions [crud]` resource:
//! the `create`/`update`/`delete` sub-blocks and their
//! `policy` / `validate` / `input excludes` / `assign` / `emits` clauses.
//! The overlay is analyzer-only; these tests pin the AST shape the
//! conventions pass consumes.

use lazuli_syntax::{ResourceDecl, parse_feature_skeletons};

fn first_resource(source: &str) -> ResourceDecl {
    parse_feature_skeletons(source)
        .expect("crud overlay authoring should parse")
        .remove(0)
        .resources
        .remove(0)
}

/// A customer-shaped resource carrying the full overlay trio.
fn customer_with_overlay() -> ResourceDecl {
    first_resource(
        "\nfeature customer_management\n  resource Customer\n\
         \x20   agency_id: ID required\n\
         \x20   situation: CustomerSituation = prospect\n\
         \x20   is_active: Boolean = true\n\
         \x20   conventions [crud]\n\
         \x20   crud\n\
         \x20     create\n\
         \x20       policy @policy.edit\n\
         \x20       validate @validator.percentage\n\
         \x20       input excludes situation, is_active, is_defaulter\n\
         \x20       assign situation = prospect\n\
         \x20       assign is_active = true\n\
         \x20       assign category = input.category_id\n\
         \x20       emits customer_created\n\
         \x20     update\n\
         \x20       policy @policy.edit\n\
         \x20       emits customer_updated\n\
         \x20     delete\n\
         \x20       policy @policy.remove\n\
         \x20       emits customer_deleted\n",
    )
}

#[test]
fn crud_overlay_parses() {
    let r = customer_with_overlay();
    let overlay = r.crud_overlay.expect("crud overlay present");

    let create = overlay.create.expect("create sub-block");
    assert_eq!(create.policy.as_deref(), Some("@policy.edit"));
    assert_eq!(create.validate, vec!["@validator.percentage"]);
    assert_eq!(
        create.input_excludes,
        vec!["situation", "is_active", "is_defaulter"]
    );
    assert_eq!(create.assigns.len(), 3);
    assert_eq!(create.assigns[0].field, "situation");
    assert_eq!(create.assigns[0].value, "prospect");
    assert_eq!(create.assigns[1].field, "is_active");
    assert_eq!(create.assigns[1].value, "true");
    assert_eq!(create.assigns[2].field, "category");
    assert_eq!(create.assigns[2].value, "input.category_id");
    assert_eq!(create.emits, vec!["customer_created"]);

    let update = overlay.update.expect("update sub-block");
    assert_eq!(update.policy.as_deref(), Some("@policy.edit"));
    assert_eq!(update.emits, vec!["customer_updated"]);

    let delete = overlay.delete.expect("delete sub-block");
    assert_eq!(delete.policy.as_deref(), Some("@policy.remove"));
    assert_eq!(delete.emits, vec!["customer_deleted"]);
}

#[test]
fn bare_crud_unchanged() {
    // `conventions [crud]` with NO `crud` block — overlay is None
    // (regression guard for bare adopters; today's synth byte-identical).
    let r = first_resource(
        "\nfeature catalog\n  resource Listing\n    title: Text required\n    conventions [crud]\n",
    );
    assert_eq!(r.conventions.len(), 1);
    assert!(r.crud_overlay.is_none());
}

#[test]
fn crud_overlay_partial_blocks() {
    // Only a `delete` overlay (the minimal soft-delete-policy case).
    let r = first_resource(
        "\nfeature catalog\n  resource Listing\n    title: Text required\n\
         \x20   conventions [crud]\n\
         \x20   crud\n\
         \x20     delete\n\
         \x20       policy @policy.remove\n",
    );
    let overlay = r.crud_overlay.expect("overlay present");
    assert!(overlay.create.is_none());
    assert!(overlay.update.is_none());
    assert_eq!(
        overlay.delete.expect("delete").policy.as_deref(),
        Some("@policy.remove")
    );
}

#[test]
fn assign_rhs_matches_handrolled_effect_grammar() {
    // The `assign` RHS captures verbatim what the hand-rolled
    // `creates`/`updates` assignment block captures — literal, `input.x`,
    // enum-variant, `ctx.x`. The analyzer lowers both through the same
    // `lower_raw_expr`, so identical text must round-trip identically.
    let r = first_resource(
        "\nfeature customer_management\n  resource Customer\n    agency_id: ID required\n\
         \x20   conventions [crud]\n\
         \x20   crud\n\
         \x20     create\n\
         \x20       assign created_at = ctx.now\n\
         \x20       assign situation = prospect\n\
         \x20       assign is_active = true\n\
         \x20       assign category = input.category_id\n",
    );
    let create = r.crud_overlay.unwrap().create.unwrap();
    let pairs: Vec<(&str, &str)> = create
        .assigns
        .iter()
        .map(|a| (a.field.as_str(), a.value.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("created_at", "ctx.now"),
            ("situation", "prospect"),
            ("is_active", "true"),
            ("category", "input.category_id"),
        ]
    );
}

#[test]
fn empty_crud_block_errors() {
    let err = parse_feature_skeletons(
        "\nfeature catalog\n  resource Listing\n    title: Text required\n\
         \x20   conventions [crud]\n\
         \x20   crud\n    title2: Text required\n",
    )
    .expect_err("empty crud block rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("at least one `create`/`update`/`delete`"),
        "expected empty-overlay diagnostic, got: {msg}"
    );
}

#[test]
fn unknown_crud_subblock_errors() {
    let err = parse_feature_skeletons(
        "\nfeature catalog\n  resource Listing\n    title: Text required\n\
         \x20   conventions [crud]\n\
         \x20   crud\n\
         \x20     upsert\n\
         \x20       policy @policy.edit\n",
    )
    .expect_err("unknown sub-block rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("create"),
        "expected sub-block catalog diagnostic, got: {msg}"
    );
}

#[test]
fn duplicate_crud_subblock_errors() {
    let err = parse_feature_skeletons(
        "\nfeature catalog\n  resource Listing\n    title: Text required\n\
         \x20   conventions [crud]\n\
         \x20   crud\n\
         \x20     create\n\
         \x20       policy @policy.edit\n\
         \x20     create\n\
         \x20       policy @policy.member\n",
    )
    .expect_err("duplicate sub-block rejected");
    let msg = format!("{err}");
    assert!(msg.contains("duplicate"), "got: {msg}");
}

#[test]
fn crud_overlay_serde_round_trip() {
    // The AST captures the overlay losslessly (round-trips through serde).
    let r = customer_with_overlay();
    let json = serde_json::to_string(&r).expect("serialize");
    let back: ResourceDecl = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(r, back);
}

#[test]
fn input_excludes_requires_excludes_keyword() {
    let err = parse_feature_skeletons(
        "\nfeature catalog\n  resource Listing\n    title: Text required\n\
         \x20   conventions [crud]\n\
         \x20   crud\n\
         \x20     create\n\
         \x20       input title\n",
    )
    .expect_err("bare input rejected");
    let msg = format!("{err}");
    assert!(msg.contains("input excludes"), "got: {msg}");
}
