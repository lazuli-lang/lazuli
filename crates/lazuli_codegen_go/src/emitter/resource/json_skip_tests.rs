//! Secret-bearing capability fields must not appear in JSON output.
//! A leaking `password_hash` or token in a generic `json.Marshal`
//! response would be a credential disclosure; the codegen carves
//! these out with `json:"-"` regardless of required/optional axis.
//!
//! This sibling exists so the security contract has a single, stable
//! home — anyone touching the resource emitter can scan one file to
//! see exactly which capability shapes must stay non-marshallable.

#![cfg(test)]

use super::test_support::{
    base_feature, e2ee_field, emit, encrypted_field, hashed_field, simple_field, simple_resource,
    token_field,
};
use lazuli_ir::{BuiltinType, Field, TypeRef};

#[test]
fn hashed_field_emits_json_skip_sentinel() {
    let mut feature = base_feature("account");
    let resource = simple_resource("user", vec![hashed_field("password_hash", true)]);
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("json:\"-\""),
        "expected json:\"-\" carve-out for @cap.Hashed field, got:\n{out}"
    );
    assert!(
        !out.contains("json:\"password_hash\""),
        "hashed field MUST NOT leak via JSON name, got:\n{out}"
    );
}

#[test]
fn encrypted_field_emits_json_skip_sentinel() {
    let mut feature = base_feature("customer");
    let resource = simple_resource("customer", vec![encrypted_field("external_id", true)]);
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("json:\"-\""),
        "expected json:\"-\" carve-out for @cap.Encrypted field, got:\n{out}"
    );
    assert!(!out.contains("json:\"external_id\""));
}

#[test]
fn e2ee_field_emits_json_skip_sentinel() {
    let mut feature = base_feature("customer");
    let resource = simple_resource("customer", vec![e2ee_field("private_note", true)]);
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("json:\"-\""),
        "expected json:\"-\" carve-out for @cap.E2ee field, got:\n{out}"
    );
    assert!(!out.contains("json:\"private_note\""));
}

#[test]
fn token_field_emits_json_skip_sentinel() {
    let mut feature = base_feature("auth");
    let resource = simple_resource("session", vec![token_field("refresh_token", true)]);
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("json:\"-\""),
        "expected json:\"-\" carve-out for @cap.Token field, got:\n{out}"
    );
    assert!(!out.contains("json:\"refresh_token\""));
}

#[test]
fn legacy_cap_secret_emits_json_skip_sentinel() {
    let mut feature = base_feature("auth");
    let resource = simple_resource(
        "credential",
        vec![simple_field("api_key", BuiltinType::CapSecret, true)],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("json:\"-\""),
        "expected json:\"-\" carve-out for @cap.Secret (legacy) field, got:\n{out}"
    );
}

#[test]
fn optional_hashed_field_still_skips_json() {
    // `json:"-"` must beat `,omitempty` — an optional hashed value
    // is still a secret if present.
    let mut feature = base_feature("account");
    let resource = simple_resource("user", vec![hashed_field("password_hash", false)]);
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(out.contains("json:\"-\""));
    assert!(!out.contains("password_hash,omitempty"));
}

#[test]
fn cap_file_field_keeps_json_tag() {
    // `@cap.File` is a public handle to storage, not a secret —
    // the JSON tag must remain so clients can address uploads.
    let mut feature = base_feature("docs");
    let resource = simple_resource(
        "document",
        vec![Field {
            name: "blob".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::CapFile),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(out.contains("json:\"blob\""));
    assert!(!out.contains("json:\"-\""));
}
