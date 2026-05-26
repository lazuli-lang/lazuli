//! Tests for the `encryption` block inside `app.lzi` — `@key.<scope>`
//! bindings with algorithm, rotation, optional rotation_profile, and
//! template axes derived from the `source env.<NAME>` literal.
//! Lives alongside `manifest.rs`.

#![cfg(test)]

use super::parse_app_manifest;

// Encryption bucket cycle — parses an `encryption` block with one
// binding per `@key.<scope>`. Indent-2 `encryption` opens the
// block; indent-4 `key @key.<scope>` opens a binding; indent-6
// `source` / `algorithm` / `rotation` populates the binding.
// See `docs/proposals/encryption-vocab.md` §Lowering.
#[test]
fn parses_encryption_block_with_one_tenant_binding() {
    use lazuli_ir::{EncryptionAlgorithm, EncryptionRotation, EncryptionTemplateAxis};

    let source = r#"
app AcmeCRM
  title "Acme CRM"
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
      rotation manual
"#;

    let manifest = parse_app_manifest(source).unwrap();
    assert_eq!(manifest.encryption_bindings.len(), 1);
    let binding = &manifest.encryption_bindings[0];
    assert_eq!(binding.scope, "@key.tenant");
    assert_eq!(binding.algorithm, EncryptionAlgorithm::Aes256Gcm);
    assert_eq!(binding.rotation, EncryptionRotation::Manual);
    let template = binding.source.template();
    assert_eq!(template.literal, "CRYPT_KEY_TENANT_{tenant_id}");
    assert_eq!(template.axes, vec![EncryptionTemplateAxis::TenantId]);
}

#[test]
fn parses_encryption_block_with_multiple_bindings() {
    let source = r#"
app AcmeCRM
  encryption
    key @key.app
      source env.CRYPT_KEY_APP
      algorithm aes_256_gcm
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
"#;

    let manifest = parse_app_manifest(source).unwrap();
    assert_eq!(manifest.encryption_bindings.len(), 2);
    assert_eq!(manifest.encryption_bindings[0].scope, "@key.app");
    assert_eq!(manifest.encryption_bindings[1].scope, "@key.tenant");
    assert!(
        manifest.encryption_bindings[0]
            .source
            .template()
            .axes
            .is_empty()
    );
    assert_eq!(
        manifest.encryption_bindings[1].source.template().literal,
        "CRYPT_KEY_TENANT_{tenant_id}"
    );
}

#[test]
fn encryption_block_absent_yields_empty_catalog() {
    let source = r#"
app AcmeCRM
  title "Acme CRM"
"#;
    let manifest = parse_app_manifest(source).unwrap();
    assert!(manifest.encryption_bindings.is_empty());
}

#[test]
fn encryption_block_rejects_non_at_key_scope() {
    let source = r#"
app AcmeCRM
  encryption
    key tenant
      source env.CRYPT_KEY_TENANT
      algorithm aes_256_gcm
"#;
    let manifest = parse_app_manifest(source).unwrap();
    // Header without `@key.` prefix is silently dropped; doctor
    // surfaces this as a separate diagnostic. The block parser
    // only records well-shaped bindings.
    assert!(manifest.encryption_bindings.is_empty());
}

#[test]
fn parses_app_encryption_key_with_rotation_profile() {
    let source = r#"
app AcmeCRM
  encryption
    key @key.tenant
      source env.CRYPT_KEY_TENANT_{tenant_id}
      algorithm aes_256_gcm
      rotation manual
      rotation_profile default
"#;
    let manifest = parse_app_manifest(source).unwrap();
    assert_eq!(manifest.encryption_bindings.len(), 1);
    assert_eq!(
        manifest.encryption_bindings[0].rotation_profile.as_deref(),
        Some("default")
    );
}
