use lazuli_ir as ir;

use lazuli_syntax::{parse_feature_skeletons, parse_lzx_document};

use crate::auth::lower_auth_identity;
use crate::query::parse_query_filter_line;
use crate::resource::lower_validate_line;
use crate::{
    AnalyzeError, lower_audit_block, lower_feature_skeleton, lower_lzx_document,
    lower_policy_atom_with_args, parse_cap_file_type, resolve_invalidates_targets,
    type_ref_from_syntax,
};

// -------------------------------------------------------------------------
// IR Error-Vocab (Cell PARSE-1) — analyzer lowering round-trip tests
// for the three new IR slots populated by this cell:
//   * `Command.policy_when_denied` ← `command.policy.when_denied`
//   * `PolicyCategory.when_denied` ← `policies.<cat>.when_denied`
//   * `Feature.errors` ← `errors` block (default + 4xx/5xx + messages)
// -------------------------------------------------------------------------

// -------------------------------------------------------------------------
// W4-3 (INLINE-POLICY-ATOM-LIST-MISPARSE) — an inline comma-separated atom
// list on a command/query (`policy @role.ADMIN, @role.MANAGER`) must lower to
// an OR of two role atoms, NOT one bogus atom. Before the parser fix the whole
// comma string fell through to the raw single-atom path and the
// command/query was PERMANENTLY denied for everyone.
// -------------------------------------------------------------------------

#[test]
fn inline_command_policy_atom_list_lowers_to_or() {
    let source = r#"
feature account
  command grant
    policy @role.ADMIN, @role.MANAGER
    input
      target_id: ID required
    returns User
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let command = feature
        .commands
        .iter()
        .find(|c| c.name == "grant")
        .expect("grant command");
    let expr = command
        .policy_expr
        .as_ref()
        .expect("inline atom list must lower to a structured policy_expr (OR), not a raw single atom");
    match expr {
        ir::PolicyExpr::Or(terms) => {
            assert_eq!(terms.len(), 2, "expected an OR of exactly two role atoms");
            match (&terms[0], &terms[1]) {
                (ir::PolicyExpr::Atom(a), ir::PolicyExpr::Atom(b)) => {
                    assert_eq!((a.namespace.as_str(), a.name.as_str()), ("role", "ADMIN"));
                    assert_eq!((b.namespace.as_str(), b.name.as_str()), ("role", "MANAGER"));
                }
                other => panic!("expected two Atom terms (ADMIN, MANAGER), got {other:?}"),
            }
        }
        other => panic!("expected PolicyExpr::Or, got {other:?}"),
    }
}

#[test]
fn inline_query_list_policy_atom_list_lowers_to_or() {
    let source = r#"
feature account
  query.list users_in_tenant
    policy @role.ADMIN, @role.MANAGER
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let query = feature
        .queries
        .iter()
        .find(|q| q.name() == "users_in_tenant")
        .expect("users_in_tenant query");
    let ir::Query::List(list) = query else {
        panic!("expected query.list, got {query:?}");
    };
    let expr = list
        .policy_expr
        .as_ref()
        .expect("inline atom list on query.list must lower to PolicyExpr::Or");
    match expr {
        ir::PolicyExpr::Or(terms) => assert_eq!(terms.len(), 2),
        other => panic!("expected PolicyExpr::Or, got {other:?}"),
    }
}

#[test]
fn lower_command_policy_when_denied_populates_typed_ref() {
    let source = r#"
feature account
  command choose_role
    policy @policy.authenticated
      when_denied @translation.choose_role_signin_required
    input
      role_id: ID required
    returns User
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let command = feature
        .commands
        .iter()
        .find(|c| c.name == "choose_role")
        .expect("choose_role command");
    let key = command
        .policy_when_denied
        .as_ref()
        .expect("policy_when_denied lowered");
    assert_eq!(key.key, "choose_role_signin_required");
}

#[test]
fn lower_policy_category_when_denied_populates_typed_ref() {
    let source = r#"
feature account
  policies
    authenticated: @scope.authenticated
      when_denied @translation.must_be_signed_in
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let authenticated = feature
        .policies
        .categories
        .iter()
        .find(|c| c.name == "authenticated")
        .expect("authenticated category");
    let key = authenticated
        .when_denied
        .as_ref()
        .expect("when_denied lowered");
    assert_eq!(key.key, "must_be_signed_in");
}

#[test]
fn lower_feature_errors_populates_typed_block() {
    let source = r#"
feature account
  errors
    default hide
    expose client 4xx message, code
    expose client 5xx code

    policy_denied message @translation.account_signin_required
    validation_failed message @translation.account_invalid_input
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let errors = feature.errors.as_ref().expect("errors block lowered");
    assert_eq!(errors.default, Some(ir::ErrorExposureDefault::Hide));
    assert_eq!(errors.exposure_4xx, vec!["message", "code"]);
    assert_eq!(errors.exposure_5xx, vec!["code"]);
    assert_eq!(errors.messages.len(), 2);
    let policy_denied = errors
        .messages
        .iter()
        .find(|m| m.code == "policy_denied")
        .expect("policy_denied row");
    assert_eq!(policy_denied.message.key, "account_signin_required");
    let validation = errors
        .messages
        .iter()
        .find(|m| m.code == "validation_failed")
        .expect("validation_failed row");
    assert_eq!(validation.message.key, "account_invalid_input");
    // v1 leaves field_messages empty (reserved slot — proposal §3.4).
    assert!(errors.field_messages.is_empty());
}

#[test]
fn lower_feature_without_errors_block_keeps_field_none() {
    let source = r#"
feature account
  command choose_role
    input
      role_id: ID required
    returns User
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    assert!(
        feature.errors.is_none(),
        "feature without `errors` block keeps `errors: None`"
    );
}

#[test]
fn lower_feature_errors_default_expose_lowers_correctly() {
    let source = r#"
feature account
  errors
    default expose
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let errors = feature.errors.as_ref().expect("errors block lowered");
    assert_eq!(errors.default, Some(ir::ErrorExposureDefault::Expose));
    assert!(errors.exposure_4xx.is_empty());
    assert!(errors.exposure_5xx.is_empty());
    assert!(errors.messages.is_empty());
}

#[test]
fn lower_feature_errors_redact_patterns_lowers() {
    let source = r#"
feature account
  errors
    error_redact "[0-9]{11}"
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let errors = feature.errors.as_ref().expect("errors block lowered");
    assert_eq!(errors.redact_patterns, vec!["[0-9]{11}".to_owned()]);
}

#[test]
fn lower_feature_errors_audience_exposure_lowers() {
    let source = r#"
feature account
  errors
    expose to @audience operator message, code
"#;
    let features = parse_feature_skeletons(source).expect("parses");
    let feature = lower_feature_skeleton(&features[0]).expect("lowers");
    let errors = feature.errors.as_ref().expect("errors block lowered");
    let rule = errors.audience_exposure.first().expect("audience exposure");
    assert_eq!(rule.audience.as_deref(), Some("operator"));
    assert_eq!(rule.fields, vec!["message".to_owned(), "code".to_owned()]);
}
