//! Back-compat round-trip — features serialized before error-vocab fields
//! existed must still deserialize cleanly with defaults of `None`.

use lazuli_ir::{Command, Feature, PolicyCategory};

use super::empty_feature;

#[test]
fn feature_without_error_vocab_fields_omits_them_from_json() {
    let feature = empty_feature();
    let json = serde_json::to_string(&feature).expect("serialize bare Feature");
    assert!(
        !json.contains("\"errors\""),
        "Feature.errors should skip when None: {json}"
    );
    // Round-trip the bare shape too.
    let back: Feature = serde_json::from_str(&json).expect("deserialize bare Feature");
    assert_eq!(feature, back);
}

#[test]
fn pre_vocab_feature_json_deserializes_with_none_errors() {
    // A pre-vocab JSON fixture has no `errors` field; deserialization
    // must populate `Feature.errors = None` via serde defaults.
    let json = r#"{
        "name": "legacy",
        "purpose": null,
        "defaults": { "tenancy": null, "timestamps": false, "policy": null },
        "uses": [],
        "enums": [],
        "resources": [],
        "events": [],
        "rules": [],
        "policies": { "categories": [], "fields": [] },
        "commands": [],
        "queries": [],
        "workflows": [],
        "jobs": [],
        "webhooks": [],
        "surfaces": [],
        "extensions": [],
        "escape_routes": []
    }"#;
    let feature: Feature = serde_json::from_str(json).expect("deserialize pre-vocab Feature");
    assert!(feature.errors.is_none());
}

#[test]
fn pre_vocab_policy_category_json_deserializes_with_none_when_denied() {
    let json = r#"{ "name": "create", "atoms": ["@role.admin"] }"#;
    let cat: PolicyCategory = serde_json::from_str(json).expect("deserialize pre-vocab category");
    assert!(cat.when_denied.is_none());
    assert_eq!(cat.atoms, vec!["@role.admin".to_owned()]);
}

#[test]
fn pre_vocab_command_json_deserializes_with_none_policy_when_denied() {
    let json = r#"{
        "name": "noop",
        "kind": "Returns",
        "input": { "kind": "Empty" },
        "effect": { "kind": "None" },
        "policy": { "kind": "None" }
    }"#;
    let cmd: Command = serde_json::from_str(json).expect("deserialize pre-vocab command");
    assert!(cmd.policy_when_denied.is_none());
}
