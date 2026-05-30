//! Tests for the `Encrypt<Resource>` / `Decrypt<Resource>` helper
//! generation triggered by `@cap.Encrypted` / `@cap.E2ee` fields. The
//! helpers wire `encryption.ForCtx(...)` + the pattern annotations
//! the lint contract requires (proposal `encryption-vocab.md`
//! §Codegen).
//!
//! Sibling exists so the codegen contract for ciphered access lives
//! in one place — when the helper signature drifts the regression
//! surface is contained.

#![cfg(test)]

use super::test_support::{
    base_feature, emit, encrypted_field as encrypted_capability_field, simple_field,
    simple_resource,
};
use lazuli_ir::{BuiltinType, CapabilityRef, E2eeCapability, Field, TypeRef};

#[test]
fn encrypted_field_emits_encrypt_and_decrypt_helpers_with_runtime_import() {
    let mut feature = base_feature("customer");
    let resource = simple_resource(
        "customer",
        vec![encrypted_capability_field("external_id", true)],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");

    // Import wired only when the resource carries encrypted fields.
    assert!(
        out.contains("\"lazuli.dev/runtime/lazuli/encryption\""),
        "expected encryption import:\n{out}"
    );
    // Helper signatures.
    assert!(
        out.contains("func EncryptCustomer(ctx *lazuli.Ctx, row *Customer) error {"),
        "expected EncryptCustomer signature:\n{out}"
    );
    assert!(
        out.contains("func DecryptCustomer(ctx *lazuli.Ctx, row *Customer) error {"),
        "expected DecryptCustomer signature:\n{out}"
    );
    // Pattern annotations honour the lint contract.
    assert!(out.contains("//lazuli:pattern resource_encrypt v1"));
    assert!(out.contains("//lazuli:pattern resource_decrypt v1"));
    // Cipher resolution lifts the `@key.<scope>` reference verbatim.
    assert!(
        out.contains("encryption.ForCtx(ctx, \"@key.tenant\", \"\")"),
        "expected ForCtx call with @key.tenant scope:\n{out}"
    );
    // Required field — no nil-pointer guard, direct len check.
    assert!(out.contains("if len(row.ExternalID) > 0 {"));
    assert!(out.contains("cipher.Encrypt(row.ExternalID)"));
    assert!(out.contains("cipher.Decrypt(row.ExternalID)"));
}

#[test]
fn optional_encrypted_field_guards_nil_and_dereferences() {
    let mut feature = base_feature("customer");
    let resource = simple_resource(
        "customer",
        vec![encrypted_capability_field("external_id", false)],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(
        out.contains("if row.ExternalID != nil && len(*row.ExternalID) > 0 {"),
        "expected pointer-nil guard:\n{out}"
    );
    assert!(out.contains("cipher.Encrypt((*row.ExternalID))"));
    assert!(out.contains("cipher.Decrypt((*row.ExternalID))"));
}

#[test]
fn e2ee_field_skipped_on_decrypt_path() {
    let mut feature = base_feature("customer");
    let resource = simple_resource(
        "customer",
        vec![Field {
            name: "private_note".to_owned(),
            type_ref: TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
                key: "@key.user".to_owned(),
            })),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    // Encrypt covers E2ee.
    assert!(out.contains("cipher.Encrypt(row.PrivateNote)"));
    // Decrypt skips E2ee — the helper body is the E2ee-only sentinel.
    assert!(
        out.contains("Every encrypted field on this resource is @cap.E2ee"),
        "expected E2ee-only Decrypt sentinel:\n{out}"
    );
    assert!(!out.contains("cipher.Decrypt(row.PrivateNote)"));
}

#[test]
fn resource_without_encrypted_fields_omits_helpers_and_import() {
    let mut feature = base_feature("customer");
    let resource = simple_resource(
        "customer",
        vec![simple_field("name", BuiltinType::Text, true)],
    );
    feature.resources.push(resource);
    let out = emit(&feature).expect("must emit");
    assert!(!out.contains("lazuli/encryption"));
    assert!(!out.contains("EncryptCustomer"));
    assert!(!out.contains("DecryptCustomer"));
}
