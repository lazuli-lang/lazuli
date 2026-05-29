//! Regression tests for bug F1-emits-multi-event on the JOB side.
//!
//! The job parser carries its OWN inline `emits` handling in `parse_job`
//! (it does NOT call `command::parse_command_emit`), so it needs its own
//! comma-split coverage: before the fix, `emits a, b` on a job lowered to
//! a single `"a, b"` event name; it must now split into two names. Kept
//! as a sibling of `job/mod.rs` so the parent stays under the ≤500-LOC
//! ceiling.

#![cfg(test)]

use super::super::parse_feature_skeletons;

#[test]
fn job_emits_two_names_splits_into_two_event_names() {
    let source = r#"feature customer
  job sync_subscription
    trigger event billing.subscription_changed
    emits subscription_activated, subscription_status_changed
    handler "./jobs/sync_subscription.go"
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let job = &features[0].jobs[0];
    assert_eq!(
        job.emits,
        vec![
            "subscription_activated".to_owned(),
            "subscription_status_changed".to_owned(),
        ],
        "`emits a, b` on a job must lower to two event names, not one comma-string"
    );
}

#[test]
fn job_emits_from_axis_splits_all_names() {
    let source = r#"feature customer
  job sync_subscription
    trigger event billing.subscription_changed
    emits created, status_changed from creates
    handler "./jobs/sync_subscription.go"
"#;
    let features = parse_feature_skeletons(source).unwrap();
    assert_eq!(
        features[0].jobs[0].emits,
        vec!["created".to_owned(), "status_changed".to_owned()]
    );
}

#[test]
fn job_multi_name_emit_with_payload_child_is_a_parse_error() {
    let source = r#"feature customer
  job sync_subscription
    trigger event billing.subscription_changed
    emits a, b
      to_owner_id = payload.owner_id
    handler "./jobs/sync_subscription.go"
"#;
    let err = parse_feature_skeletons(source)
        .expect_err("multi-name job `emits` with a payload child block must be rejected");
    assert!(format!("{err}").contains("multi-name `emits`"), "got: {err}");
}
