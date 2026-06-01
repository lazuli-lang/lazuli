//! 0004 defaults-hoist — `defaults rate_limit "<spec>"` + `defaults audit
//! default` parse into the feature `defaults` block AST.
//!
//! These exercise the surface (parser → AST) only. The inheritance /
//! override / `audit none` opt-out semantics are proven against the IR in
//! the analyzer + codegen test suites (`cargo test -p lazuli_codegen_go
//! defaults`). Here we assert the two new keys land on
//! `FeatureDefaults` and that malformed forms are rejected so an LLM
//! cannot author a silent typo.

use lazuli_syntax::{DefaultsAudit, DefaultsTenancy, parse_feature_skeletons};

#[test]
fn defaults_rate_limit_audit_parse() {
    let source = r#"
feature billing
  defaults
    tenancy org
    rate_limit "60 per minute per actor"
    audit default
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let defaults = features[0].defaults.as_ref().expect("defaults block");
    assert!(matches!(defaults.tenancy, Some(DefaultsTenancy::Org)));
    assert_eq!(
        defaults.rate_limit.as_deref(),
        Some("60 per minute per actor")
    );
    assert_eq!(defaults.audit, Some(DefaultsAudit::Default));
}

#[test]
fn defaults_rate_limit_only_parses() {
    let source = r#"
feature billing
  defaults
    rate_limit "10 per 10 minutes per ip"
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let defaults = features[0].defaults.as_ref().expect("defaults block");
    assert_eq!(
        defaults.rate_limit.as_deref(),
        Some("10 per 10 minutes per ip")
    );
    assert_eq!(defaults.audit, None);
}

#[test]
fn defaults_audit_only_parses() {
    let source = r#"
feature billing
  defaults
    audit default
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let defaults = features[0].defaults.as_ref().expect("defaults block");
    assert_eq!(defaults.audit, Some(DefaultsAudit::Default));
    assert_eq!(defaults.rate_limit, None);
}

#[test]
fn defaults_rate_limit_duplicate_errors() {
    let source = r#"
feature billing
  defaults
    rate_limit "60 per minute per actor"
    rate_limit "10 per minute per actor"
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    let message = format!("{err}");
    assert!(
        message.contains("at most once"),
        "duplicate defaults rate_limit should error: {message}"
    );
}

#[test]
fn defaults_rate_limit_empty_errors() {
    let source = r#"
feature billing
  defaults
    rate_limit ""
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    let message = format!("{err}");
    assert!(
        message.contains("requires a spec"),
        "empty defaults rate_limit should error: {message}"
    );
}

#[test]
fn defaults_audit_unknown_mode_errors() {
    let source = r#"
feature billing
  defaults
    audit everything
"#;
    let err = parse_feature_skeletons(source).unwrap_err();
    let message = format!("{err}");
    assert!(
        message.contains("only `default`"),
        "unknown defaults audit mode should error: {message}"
    );
}
