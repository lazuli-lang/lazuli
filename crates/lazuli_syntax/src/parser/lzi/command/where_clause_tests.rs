//! BUG #18 — `where <col> = <expr>` inside an `updates`/`deletes` block
//! is parsed as the row-scoping WHERE clause, NOT as a SET assignment
//! with a phantom column literally named `"where id"`.
//!
//! These tests pin the parser-level AST shape the analyzer + codegen
//! consume: `CommandEffectDecl.where_clause` carries the scoping
//! bindings; `CommandEffectDecl.assignments` carries only the real SET
//! columns.

#![cfg(test)]

use super::super::parse_feature_skeletons;

/// `updates User` with `where id = ctx.actor.id` — the canonical "update
/// my own row" shape that 400'd in pauta. The `where` line MUST land in
/// `where_clause` (id ← ctx.actor.id), and the SET `assignments` MUST
/// contain only the real columns, with NO `"where id"` phantom.
#[test]
fn updates_where_actor_id_routes_to_where_clause_not_assignments() {
    let source = r#"feature account
  command complete_profile
    input
      full_name: Text required
    policy @policy.authenticated
    updates User
      where id = ctx.actor.id
      full_name = input.full_name
      updated_at = ctx.now
    emits account_profile_completed
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let effect = features[0].commands[0]
        .effect
        .as_ref()
        .expect("updates effect parsed");

    // The where line is recognised as a WHERE binding.
    assert_eq!(effect.where_clause.len(), 1, "exactly one `where` binding");
    assert_eq!(effect.where_clause[0].field, "id");
    assert_eq!(effect.where_clause[0].value, "ctx.actor.id");

    // The SET assignments carry only the real columns — NO `"where id"`.
    assert_eq!(effect.assignments.len(), 2, "two SET columns");
    assert!(
        effect.assignments.iter().all(|a| a.field != "where id"),
        "no phantom `where id` SET column: {:?}",
        effect.assignments
    );
    assert_eq!(effect.assignments[0].field, "full_name");
    assert_eq!(effect.assignments[0].value, "input.full_name");
    assert_eq!(effect.assignments[1].field, "updated_at");
    assert_eq!(effect.assignments[1].value, "ctx.now");
}

/// Route-keyed update — `where id = route.id` — must also land in
/// `where_clause` (don't regress the route path).
#[test]
fn updates_where_route_id_routes_to_where_clause() {
    let source = r#"feature agency
  command rename_agency
    route id: ID
    input
      name: Text required
    policy @policy.authenticated
    updates Agency
      where id = route.id
      name = input.name
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let effect = features[0].commands[0].effect.as_ref().unwrap();
    assert_eq!(effect.where_clause.len(), 1);
    assert_eq!(effect.where_clause[0].field, "id");
    assert_eq!(effect.where_clause[0].value, "route.id");
    assert_eq!(effect.assignments.len(), 1);
    assert_eq!(effect.assignments[0].field, "name");
}

/// `deletes` with an explicit `where` also routes to `where_clause`.
#[test]
fn deletes_where_routes_to_where_clause() {
    let source = r#"feature agency
  command remove_dept
    route id: ID
    policy @policy.authenticated
    deletes Department
      where id = route.id
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let effect = features[0].commands[0].effect.as_ref().unwrap();
    assert_eq!(effect.where_clause.len(), 1);
    assert_eq!(effect.where_clause[0].field, "id");
    assert_eq!(effect.where_clause[0].value, "route.id");
    assert!(effect.assignments.is_empty());
}

/// An `updates` block WITHOUT a `where` leaves `where_clause` empty (the
/// legacy id-key codegen fallback then applies). Regression guard so the
/// new branch never fires spuriously.
#[test]
fn updates_without_where_leaves_where_clause_empty() {
    let source = r#"feature catalog
  command set_tier
    route id: ID
    input
      tier: Text required
    policy @policy.authenticated
    updates Customer
      tier = input.tier
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let effect = features[0].commands[0].effect.as_ref().unwrap();
    assert!(effect.where_clause.is_empty());
    assert_eq!(effect.assignments.len(), 1);
    assert_eq!(effect.assignments[0].field, "tier");
}

/// A column whose name merely STARTS WITH `where` (no trailing space
/// separator) is a normal SET assignment, not a where clause.
#[test]
fn column_named_like_where_is_not_a_where_clause() {
    let source = r#"feature ops
  command tick
    route id: ID
    input
      whereabouts: Text required
    policy @policy.authenticated
    updates Asset
      whereabouts = input.whereabouts
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let effect = features[0].commands[0].effect.as_ref().unwrap();
    assert!(effect.where_clause.is_empty(), "`whereabouts` is a SET col");
    assert_eq!(effect.assignments.len(), 1);
    assert_eq!(effect.assignments[0].field, "whereabouts");
}
