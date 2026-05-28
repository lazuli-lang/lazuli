//! W4 command-side primitive parser tests — GAP-06 `approval chain` +
//! GAP-REORDER-01 `reorder <Resource> by <field>`. Co-located with
//! `command/mod.rs` as a sibling so the parent stays under the LOC ceiling.

#![cfg(test)]

use super::super::parse_feature_skeletons;

#[test]
fn approval_single_by_lifts_to_one_element_chain() {
    // Back-compat: the single-approver `by` form lifts to a 1-element chain
    // with `by == chain[0]` and `sequential == false`.
    let source = r#"feature customer
  command archive
    policy @policy.delete
    approval
      by @role.admin
      timeout "24h"
      then deny
    deletes Customer
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let approval = features[0].commands[0].approval.as_ref().unwrap();
    assert_eq!(approval.by, "@role.admin");
    assert_eq!(approval.chain, vec!["@role.admin".to_owned()]);
    assert!(!approval.sequential);
}

#[test]
fn approval_chain_sequential_parses() {
    // W4 GAP-06 — `chain [@role.a, @role.b] sequential`.
    let source = r#"feature customer
  command archive
    policy @policy.delete
    approval
      chain [@role.manager, @role.admin] sequential
      timeout "24h"
      then deny
    deletes Customer
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let approval = features[0].commands[0].approval.as_ref().unwrap();
    assert_eq!(
        approval.chain,
        vec!["@role.manager".to_owned(), "@role.admin".to_owned()]
    );
    assert_eq!(approval.by, "@role.manager"); // first approver
    assert!(approval.sequential);
}

#[test]
fn approval_by_and_chain_together_is_a_parse_error() {
    let source = r#"feature customer
  command archive
    policy @policy.delete
    approval
      by @role.admin
      chain [@role.manager, @role.admin]
      then deny
    deletes Customer
"#;
    assert!(
        parse_feature_skeletons(source).is_err(),
        "`approval` must not accept both `by` and `chain`"
    );
}

#[test]
fn reorder_command_parses() {
    // W4 GAP-REORDER-01 — `reorder <Resource> by <position_field>`.
    let source = r#"feature jobs
  command reorder_steps
    policy @policy.update
    reorder JobStep by position
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let reorder = features[0].commands[0].reorder.as_ref().unwrap();
    assert_eq!(reorder.resource, "JobStep");
    assert_eq!(reorder.position_field, "position");
}

#[test]
fn reorder_without_by_is_a_parse_error() {
    let source = r#"feature jobs
  command reorder_steps
    policy @policy.update
    reorder JobStep
"#;
    assert!(
        parse_feature_skeletons(source).is_err(),
        "`reorder <Resource>` without `by <field>` must be a parse error"
    );
}
