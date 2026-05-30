//! Tests for the `rate_limit none` security opt-out (a mutating command may
//! decline a rate limit but must justify it with a `reason` child). It lowers
//! to the same no-throttle spec as `rate_limit "unlimited"`; the deeper-indented
//! `reason "..."` child is consumed at the parse layer so it does not trip the
//! four-space command-child indentation check.
//!
//! Co-located with `command/mod.rs` as a sibling per the ≤500-LOC rule.

#![cfg(test)]

use super::super::parse_feature_skeletons;

/// `rate_limit none` + a `reason` child parses; the spec lowers to the
/// `unlimited` no-throttle sentinel and the command body continues cleanly.
#[test]
fn rate_limit_none_with_reason_parses_as_no_throttle() {
    let source = r#"feature billing
  command record_payment
    policy @policy.author
    rate_limit none
      reason "internal system command, invoked only from the payment webhook"
    creates Payment from input
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let cmd = &features[0].commands[0];
    let spec = cmd
        .rate_limit
        .as_ref()
        .expect("`rate_limit none` yields a rate-limit spec (satisfies has-rate-limit)");
    assert_eq!(
        spec.default.as_deref(),
        Some("unlimited"),
        "opt-out lowers to the unlimited/no-throttle sentinel"
    );
    assert!(spec.by_env.is_empty());
    // The `reason` child was consumed, not mis-read as a command child — the
    // `creates` effect after it still parsed.
    assert!(
        cmd.effect.is_some(),
        "command body continued past the reason child"
    );
}

/// The `reason` child is optional to the parser (the LSP enforces it); bare
/// `rate_limit none` still parses.
#[test]
fn rate_limit_none_without_reason_still_parses() {
    let source = r#"feature billing
  command record_payment
    policy @policy.author
    rate_limit none
    creates Payment from input
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let cmd = &features[0].commands[0];
    assert!(cmd.rate_limit.is_some());
    assert!(cmd.effect.is_some());
}

/// Regression: the new `none` branch must not disturb the normal quoted-spec
/// form (it is matched only on the exact `rate_limit none` line).
#[test]
fn rate_limit_quoted_spec_still_parses() {
    let source = r#"feature billing
  command record_payment
    policy @policy.author
    rate_limit "600 per minute per ip"
    creates Payment from input
"#;
    let features = parse_feature_skeletons(source).unwrap();
    assert_eq!(
        features[0].commands[0]
            .rate_limit
            .as_ref()
            .unwrap()
            .default
            .as_deref(),
        Some("600 per minute per ip")
    );
}
