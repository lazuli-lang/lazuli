//! GAP-AUDIT-01 — `audit ... materialize @feature.<f>.<R>` parser tests.
//! Co-located with `command/mod.rs` as a sibling so the parent stays
//! under the LOC ceiling.

#![cfg(test)]

use super::super::parse_feature_skeletons;

#[test]
fn audit_materialize_parses_feature_and_resource() {
    let source = r#"feature jobs
  command create_job
    policy @policy.create
    audit actor, target.id
      emit_to @event_group.job_audit
      before
      after
      materialize @feature.operation_audit_log.OperationLog
    creates Job
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let audit = features[0].commands[0].audit.as_ref().unwrap();
    let m = audit.materialize.as_ref().expect("materialize parsed");
    assert_eq!(m.feature, "operation_audit_log");
    assert_eq!(m.resource, "OperationLog");
    // Sibling audit children still parse alongside materialize.
    assert_eq!(audit.emit_to.as_deref(), Some("@event_group.job_audit"));
    assert!(audit.record_before);
    assert!(audit.record_after);
}

#[test]
fn audit_without_materialize_leaves_none() {
    let source = r#"feature jobs
  command create_job
    policy @policy.create
    audit actor, target.id
      emit_to @event_group.job_audit
    creates Job
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let audit = features[0].commands[0].audit.as_ref().unwrap();
    assert!(audit.materialize.is_none());
}

#[test]
fn audit_materialize_requires_feature_prefix() {
    // Missing the `@feature.` prefix is a parse error.
    let source = r#"feature jobs
  command create_job
    policy @policy.create
    audit actor
      materialize OperationLog
    creates Job
"#;
    assert!(
        parse_feature_skeletons(source).is_err(),
        "`audit materialize` requires `@feature.<feature>.<Resource>`"
    );
}

#[test]
fn audit_materialize_requires_two_segments() {
    // `@feature.audit` (only one segment) is a parse error.
    let source = r#"feature jobs
  command create_job
    policy @policy.create
    audit actor
      materialize @feature.audit
    creates Job
"#;
    assert!(
        parse_feature_skeletons(source).is_err(),
        "`audit materialize` requires both a feature and a resource segment"
    );
}

#[test]
fn audit_materialize_declared_twice_is_error() {
    let source = r#"feature jobs
  command create_job
    policy @policy.create
    audit actor
      materialize @feature.audit.OperationLog
      materialize @feature.audit.OperationLog
    creates Job
"#;
    assert!(
        parse_feature_skeletons(source).is_err(),
        "`audit materialize` may be declared at most once"
    );
}
