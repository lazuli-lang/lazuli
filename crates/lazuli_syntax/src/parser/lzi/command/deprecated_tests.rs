//! `deprecated` block / inline / bare parser tests for both `command`
//! and `api` headers. Co-located with `command/mod.rs` as a sibling
//! because the inline test block alone pushed the parent past the
//! 500-LOC ceiling.

#![cfg(test)]

use super::super::parse_feature_skeletons;

#[test]
fn command_deprecated_block_parses() {
    let source = r#"feature customer
  command legacy_update
    policy @policy.update
    creates Customer
    deprecated
      since "2026-03-01"
      replacement command.update_v2
      sunset "2026-12-31"
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let dep = features[0].commands[0].deprecated.as_ref().unwrap();
    assert_eq!(dep.since.as_deref(), Some("2026-03-01"));
    assert_eq!(dep.replacement.as_deref(), Some("command.update_v2"));
    assert_eq!(dep.sunset.as_deref(), Some("2026-12-31"));
}

#[test]
fn command_deprecated_inline_parses() {
    let source = r#"feature customer
  command legacy_update
    policy @policy.update
    deprecated since "2026-03-01" replacement command.update_v2 sunset "2026-12-31"
    creates Customer
"#;
    let features = parse_feature_skeletons(source).unwrap();
    assert_eq!(
        features[0].commands[0]
            .deprecated
            .as_ref()
            .unwrap()
            .replacement
            .as_deref(),
        Some("command.update_v2")
    );
}

#[test]
fn command_deprecated_bare_parses() {
    let source = "feature customer\n  command legacy_update\n    policy @policy.update\n    deprecated\n    creates Customer\n";
    let features = parse_feature_skeletons(source).unwrap();
    let dep = features[0].commands[0].deprecated.as_ref().unwrap();
    assert!(dep.since.is_none());
    assert!(dep.replacement.is_none());
    assert!(dep.sunset.is_none());
}

#[test]
fn api_deprecated_block_parses() {
    let source = r#"feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    policy @policy.read
    deprecated
      since "2026-04-01"
      replacement api.export_v2
      sunset "2026-09-30"
"#;
    let features = parse_feature_skeletons(source).unwrap();
    let dep = features[0].apis[0].deprecated.as_ref().unwrap();
    assert_eq!(dep.since.as_deref(), Some("2026-04-01"));
    assert_eq!(dep.replacement.as_deref(), Some("api.export_v2"));
    assert_eq!(dep.sunset.as_deref(), Some("2026-09-30"));
}

#[test]
fn api_deprecated_inline_parses() {
    let source = r#"feature customer
  api legacy_export
    method GET
    path "/api/customers/export-v1"
    output [Customer]
    deprecated since "2026-04-01" replacement api.export_v2 sunset "2026-09-30"
"#;
    let features = parse_feature_skeletons(source).unwrap();
    assert_eq!(
        features[0].apis[0]
            .deprecated
            .as_ref()
            .unwrap()
            .replacement
            .as_deref(),
        Some("api.export_v2")
    );
}

#[test]
fn api_deprecated_bare_parses() {
    let source = "feature customer\n  api legacy_export\n    method GET\n    path \"/api/customers/export-v1\"\n    output [Customer]\n    deprecated\n";
    let features = parse_feature_skeletons(source).unwrap();
    let dep = features[0].apis[0].deprecated.as_ref().unwrap();
    assert!(dep.since.is_none());
    assert!(dep.replacement.is_none());
    assert!(dep.sunset.is_none());
}
