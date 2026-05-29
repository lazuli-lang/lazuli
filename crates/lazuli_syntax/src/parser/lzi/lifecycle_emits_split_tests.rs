//! Regression tests for bug F1-emits-multi-event on the resource
//! LIFECYCLE side — `transition … emits a, b`.
//!
//! This is the path the production-grade `billing` example actually
//! exercises (`examples/production-grade/features/billing/billing.lzi:44`
//! `emits subscription_activated, subscription_status_changed`): a
//! multi-name `emits` inside a resource `lifecycle` transition. The
//! transition's `Vec<String>` emits flow 1:1 into IR via the analyzer
//! (`lazuli_analyzer/src/lifecycle/mod.rs:215`) and into the synthesized
//! command's `Emits:` block. Before the fix the comma-string survived as
//! one bogus event name; it must now split into one event per name.
//!
//! Sibling of `lifecycle.rs` to keep the parent under the ≤500-LOC ceiling.

#![cfg(test)]

use super::parse_feature_skeletons;

/// Walk `feature → resource → lifecycle → transition[name]` and return its
/// `emits` list. Keeps each test focused on the assertion, not navigation.
fn transition_emits<'a>(source: &str, transition: &str) -> Vec<String> {
    let features = parse_feature_skeletons(source).unwrap();
    let lifecycle = features[0].resources[0]
        .lifecycle
        .as_ref()
        .expect("resource must carry a lifecycle block");
    lifecycle
        .transitions
        .iter()
        .find(|t| t.name == transition)
        .unwrap_or_else(|| panic!("transition `{transition}` not found"))
        .emits
        .clone()
}

#[test]
fn transition_emits_two_names_splits_into_two_events() {
    // Mirrors examples/production-grade/.../billing.lzi:44.
    let source = r#"feature billing
  resource Subscription
    status: SubscriptionStatus = trialing

    lifecycle status
      state trialing initial
      state active
      transition activate
        from trialing
        to active
        emits subscription_activated, subscription_status_changed
"#;
    assert_eq!(
        transition_emits(source, "activate"),
        vec![
            "subscription_activated".to_owned(),
            "subscription_status_changed".to_owned(),
        ],
        "`transition … emits a, b` must split into two event names, not one comma-string"
    );
}

#[test]
fn transition_single_emit_still_parses() {
    let source = r#"feature billing
  resource Subscription
    status: SubscriptionStatus = trialing

    lifecycle status
      state trialing initial
      state active
      transition activate
        from trialing
        to active
        emits subscription_activated
"#;
    assert_eq!(
        transition_emits(source, "activate"),
        vec!["subscription_activated".to_owned()]
    );
}

#[test]
fn transition_repeated_emits_lines_accumulate_with_split() {
    // `emits` may repeat on a transition; a later multi-name line appends
    // its split names after an earlier single-name line.
    let source = r#"feature billing
  resource Subscription
    status: SubscriptionStatus = trialing

    lifecycle status
      state trialing initial
      state active
      transition activate
        from trialing
        to active
        emits a
        emits b, c
"#;
    assert_eq!(
        transition_emits(source, "activate"),
        vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
    );
}

#[test]
fn transition_emits_only_commas_is_a_parse_error() {
    let source = r#"feature billing
  resource Subscription
    status: SubscriptionStatus = trialing

    lifecycle status
      state trialing initial
      state active
      transition activate
        from trialing
        to active
        emits ,
"#;
    assert!(parse_feature_skeletons(source).is_err());
}
