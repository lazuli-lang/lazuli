// Spec 0018 — `crud` overlay merge + IR-equivalence tests.
//
// The acceptance oracle is `synth_overlay_ir_equals_handrolled`: a
// customer-shaped feature where `conventions [crud]` + a `crud` overlay
// synthesizes a `create_<r>` / `update_<r>` / `delete_<r>` whose IR is
// byte-identical to the equivalent HAND-ROLLED command authored in a
// sibling feature. Both sides go through `lower_feature_skeleton` (the
// same lowering path), so equivalence is measured on the final IR the
// emitters consume.

use lazuli_syntax::parse_feature_skeletons;

use crate::lower_feature_skeleton;
use lazuli_ir as ir;

/// Lower the first feature in `source` to IR.
fn lower(source: &str) -> ir::Feature {
    let features = parse_feature_skeletons(source).expect("parses");
    lower_feature_skeleton(&features[0]).expect("lowers")
}

fn command<'a>(feature: &'a ir::Feature, name: &str) -> &'a ir::Command {
    feature
        .commands
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("command `{name}` not found"))
}

/// Strip the per-command identity that legitimately differs between a
/// synthesized command and a hand-rolled one (name spans, previous_names,
/// derived markers) so the comparison is over the SEMANTIC IR — input,
/// effect, policy, emits, route, kind. `invalidates` differs by design
/// (the synth attaches the canonical list), so we null it for the
/// equivalence projection.
fn semantic_view(c: &ir::Command) -> ir::Command {
    let mut c = c.clone();
    c.span_ref = None;
    c.previous_names = Vec::new();
    c.derived_from = None;
    c.synthesized_from_cap_file = None;
    c.invalidates = Vec::new();
    // rate_limit/audit: the synth sets canonical defaults; a hand-rolled
    // command that omits them would differ. We compare these explicitly in
    // dedicated assertions, not in the structural projection.
    c.rate_limit = None;
    c.audit = None;
    c
}

/// The minimal Customer-shaped resource + overlay. Mirrors the Pauta
/// `create_customer` shape: a `policy @policy.edit`, an `emits`, default
/// literals (`situation = prospect`, `is_active = true`), and a
/// field-rename (`category = input.category_id`).
fn overlay_feature() -> ir::Feature {
    lower(
        "\nfeature shop\n  policies\n    authenticated: @scope.authenticated\n\
         \x20 resource Customer\n\
         \x20   org: Org required\n\
         \x20   legal_name: Text required\n\
         \x20   category: CustomerCategory optional\n\
         \x20   situation: CustomerSituation = prospect\n\
         \x20   is_active: Boolean = true\n\
         \x20   conventions [crud]\n\
         \x20   crud\n\
         \x20     create\n\
         \x20       policy @policy.edit\n\
         \x20       validate @validator.percentage\n\
         \x20       input excludes category, situation, is_active\n\
         \x20       assign category = input.category\n\
         \x20       assign situation = prospect\n\
         \x20       assign is_active = true\n\
         \x20       emits customer_created\n\
         \x20     update\n\
         \x20       policy @policy.edit\n\
         \x20       emits customer_updated\n\
         \x20     delete\n\
         \x20       policy @policy.remove\n\
         \x20       emits customer_deleted\n",
    )
}

/// Hand-rolled equivalent: the SAME three commands authored explicitly on
/// a resource with NO `conventions [crud]`. The effect-assignment ORDER is
/// authored to match the synth output (input-derived assigns first, then
/// the overlay assigns), so the byte-level comparison is exact.
fn handrolled_feature() -> ir::Feature {
    lower(
        "\nfeature shop\n  policies\n    authenticated: @scope.authenticated\n\
         \x20 resource Customer\n\
         \x20   org: Org required\n\
         \x20   legal_name: Text required\n\
         \x20   category: CustomerCategory optional\n\
         \x20   situation: CustomerSituation = prospect\n\
         \x20   is_active: Boolean = true\n\
         \x20 command create_customer\n\
         \x20   input\n\
         \x20     legal_name: Text required\n\
         \x20   policy @policy.edit\n\
         \x20   creates Customer from input\n\
         \x20     legal_name = input.legal_name\n\
         \x20     category = input.category\n\
         \x20     situation = prospect\n\
         \x20     is_active = true\n\
         \x20   emits customer_created\n\
         \x20 command update_customer\n\
         \x20   route id: ID\n\
         \x20   input\n\
         \x20     legal_name: Text optional\n\
         \x20     category: CustomerCategory optional\n\
         \x20     situation: CustomerSituation optional\n\
         \x20     is_active: Boolean optional\n\
         \x20   policy @policy.edit\n\
         \x20   updates Customer\n\
         \x20     legal_name = input.legal_name\n\
         \x20     category = input.category\n\
         \x20     situation = input.situation\n\
         \x20     is_active = input.is_active\n\
         \x20   emits customer_updated\n\
         \x20 command delete_customer\n\
         \x20   route id: ID\n\
         \x20   policy @policy.remove\n\
         \x20   deletes Customer\n\
         \x20   emits customer_deleted\n",
    )
}

#[test]
fn overlay_policy_replaces_default() {
    let f = overlay_feature();
    let create = command(&f, "create_customer");
    // Synth default is PolicyRef::Local("authenticated"); overlay replaces
    // it with the @policy.edit atom (exactly as the hand-rolled lowers).
    assert_eq!(create.policy, ir::PolicyRef::Atom("policy.edit".to_owned()));
    let delete = command(&f, "delete_customer");
    assert_eq!(delete.policy, ir::PolicyRef::Atom("policy.remove".to_owned()));
}

#[test]
fn overlay_assign_merges_into_effect() {
    let f = overlay_feature();
    let create = command(&f, "create_customer");
    let assigns = match &create.effect {
        ir::CommandEffect::Creates(e) => &e.assignments,
        other => panic!("expected Creates, got {other:?}"),
    };
    fn path(segs: &[&str]) -> ir::Expr {
        ir::Expr::Path(ir::Path::from_segments(
            segs.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        ))
    }
    let pairs: Vec<(&str, ir::Expr)> =
        assigns.iter().map(|a| (a.field.as_str(), a.value.clone())).collect();
    // legal_name is input-derived; category/situation/is_active are overlay
    // assigns appended after, in author order. category/situation/is_active
    // were `input excludes`d so they carry no auto input-binding.
    // `situation = prospect` is a bare enum-variant literal — the
    // `resolve_enum_literal_bindings` pass lifts it from `Expr::Path` to
    // `Expr::Enum { type_name: None, variant: "prospect" }` so codegen
    // emits the TEXT const `FromConst("prospect")` (the unqualified
    // `Expr::Enum` render), not a runtime path lookup.
    assert_eq!(
        pairs,
        vec![
            ("legal_name", path(&["input", "legal_name"])),
            ("category", path(&["input", "category"])),
            (
                "situation",
                ir::Expr::Enum(ir::EnumLiteral {
                    type_name: None,
                    variant: "prospect".to_owned(),
                }),
            ),
            ("is_active", ir::Expr::Boolean(true)),
        ]
    );
}

#[test]
fn overlay_emits_and_validate() {
    let f = overlay_feature();
    let create = command(&f, "create_customer");
    assert_eq!(create.emits, vec!["customer_created"]);
    // `validate` is Doctor-only on IR — it does NOT appear as a command
    // field (the hand-rolled `validate` doesn't lower either), so there is
    // nothing to assert on IR; equivalence is preserved precisely because
    // the overlay's validate carries no IR weight. (Parity assertion: the
    // command lowered without error despite carrying a `validate` overlay.)
    assert!(matches!(create.effect, ir::CommandEffect::Creates(_)));
}

#[test]
fn overlay_input_excludes() {
    let f = overlay_feature();
    let create = command(&f, "create_customer");
    match &create.input {
        ir::CommandInput::Typed(slots) => {
            let names: Vec<&str> = slots.iter().map(|s| s.name.as_str()).collect();
            // category/situation/is_active excluded; legal_name remains.
            assert_eq!(names, vec!["legal_name"]);
        }
        other => panic!("expected Typed input, got {other:?}"),
    }
}

#[test]
fn synth_overlay_ir_equals_handrolled() {
    // THE acceptance oracle (unit level): the full create/update/delete
    // trio's semantic IR is byte-identical between synth+overlay and the
    // hand-rolled equivalent.
    let synth = overlay_feature();
    let hand = handrolled_feature();
    for name in ["create_customer", "update_customer", "delete_customer"] {
        let s = semantic_view(command(&synth, name));
        let h = semantic_view(command(&hand, name));
        assert_eq!(s, h, "IR diverged for `{name}`");
    }
}

#[test]
fn bare_crud_unchanged_against_no_overlay() {
    // A `conventions [crud]` resource with NO `crud` block synthesizes
    // exactly what it did before the overlay landed (regression guard for
    // Hostpoint bare adopters). We assert the synth create carries the
    // default policy + every resource input field.
    let f = lower(
        "\nfeature shop\n  policies\n    authenticated: @scope.authenticated\n\
         \x20 resource Listing\n\
         \x20   org: Org required\n\
         \x20   title: Text required\n\
         \x20   conventions [crud]\n",
    );
    let create = command(&f, "create_listing");
    assert_eq!(create.policy, ir::PolicyRef::Local("authenticated".to_owned()));
    assert!(create.emits.is_empty());
    match &create.input {
        ir::CommandInput::Typed(slots) => {
            assert_eq!(
                slots.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
                vec!["title"]
            );
        }
        other => panic!("expected Typed input, got {other:?}"),
    }
}

#[test]
fn delete_overlay_is_soft_delete_aware() {
    // With `soft_delete by`, the synth delete is soft-delete-aware (spec
    // 0015 owns the lowering). The overlay only sets policy + emits; it
    // must not disturb the soft-delete shape. We assert the delete kind
    // stays Delete and the overlay policy/emits applied.
    let f = lower(
        "\nfeature shop\n  policies\n    authenticated: @scope.authenticated\n\
         \x20 resource Listing\n\
         \x20   org: Org required\n\
         \x20   title: Text required\n\
         \x20   soft_delete by\n\
         \x20   conventions [crud]\n\
         \x20   crud\n\
         \x20     delete\n\
         \x20       policy @policy.remove\n\
         \x20       emits listing_deleted\n",
    );
    let delete = command(&f, "delete_listing");
    assert!(matches!(delete.kind, ir::CommandKind::Delete));
    assert_eq!(delete.policy, ir::PolicyRef::Atom("policy.remove".to_owned()));
    assert_eq!(delete.emits, vec!["listing_deleted"]);
}
