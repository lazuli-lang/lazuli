//! Encryption bucket cycle IR types.
//!
//! Lowered from `app.lzi` `encryption` block (Candidate B in
//! `docs/proposals/encryption-vocab.md` §Design). One `EncryptionBinding`
//! per `@key.<scope>` referenced by any `@cap.Encrypted` / `@cap.E2ee`
//! field site in the capsule.
//!
//! The capability-side surface (`@cap.Encrypted(key:@key.<scope>)`)
//! continues to live on `CapabilityRef` in the main IR module; this file
//! covers only the app-level binding catalog + the new `@cap.E2ee`
//! sibling.
//!
//! See `docs/proposals/encryption-vocab.md` §Lowering.

use serde::{Deserialize, Serialize};

use crate::SpanRef;

/// Phase L Tier 5 — typed `@cap.E2ee(key:@key.<scope>)`. Mirror of
/// `EncryptedCapability` for end-to-end-encrypted fields the server
/// stores but never reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct E2eeCapability {
    /// `@key.<scope>` reference, stored verbatim with the `@key.`
    /// prefix preserved so cold-readers see the namespace.
    pub key: String,
}

/// Phase L Tier 5 — app-level encryption key binding. One entry per
/// `@key.<scope>` referenced by `@cap.Encrypted` / `@cap.E2ee` field
/// sites in the capsule. Lives on `AppManifest.encryption_bindings`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionBinding {
    /// `@key.<scope>` reference, verbatim with `@key.` prefix.
    pub scope: String,
    /// Where the key bytes live. Closed catalog `{Env, Secrets}`.
    pub source: EncryptionSource,
    /// Symmetric algorithm. v0: `Aes256Gcm`.
    pub algorithm: EncryptionAlgorithm,
    /// v0: `Manual`. `KmsManaged` deferred.
    pub rotation: EncryptionRotation,
    /// CL.C.5 — optional binding to a `registry.secret_rotation <name>`
    /// profile. Doctor's `secret-rotation-binding-unknown` flags
    /// references to undeclared profiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Phase L Tier 5 — where an `EncryptionBinding` resolves its key
/// bytes. `Env` reads via the env-var schema; `Secrets` reads via the
/// pending `@adapter.secrets` (Wave 2). The template literal preserves
/// the authored axes so doctor + runtime can substitute them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum EncryptionSource {
    /// `env.<UPPER>{...}` — resolves via the env var schema.
    Env(EncryptionTemplate),
    /// `secrets.<lower>{...}` — resolves via `@adapter.secrets`.
    Secrets(EncryptionTemplate),
}

impl EncryptionSource {
    /// Borrow the underlying template, regardless of the source kind.
    pub fn template(&self) -> &EncryptionTemplate {
        match self {
            Self::Env(t) | Self::Secrets(t) => t,
        }
    }
}

/// Phase L Tier 5 — parsed source template. Literal preserves the
/// authored `{tenant_id}` / `{user_id}` / `{record_id}` braces verbatim
/// so the runtime can substitute them; `axes` enumerates which axes
/// the literal contains.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionTemplate {
    /// Literal name with template axes preserved verbatim,
    /// e.g. `"CRYPT_KEY_TENANT_{tenant_id}"`.
    pub literal: String,
    /// Parsed template axes; closed catalog
    /// `{tenant_id, user_id, record_id}`.
    pub axes: Vec<EncryptionTemplateAxis>,
}

impl EncryptionTemplate {
    /// Parse the authored literal into a typed template. Axes inside
    /// `{...}` braces are matched against the closed catalog; unknown
    /// names are skipped (doctor surfaces them as `ENC-TEMPLATE-AXIS-001`).
    pub fn parse(literal: &str) -> Self {
        let mut axes = Vec::new();
        let bytes = literal.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'{' {
                if let Some(end) = literal[i + 1..].find('}') {
                    let axis_name = &literal[i + 1..i + 1 + end];
                    if let Some(axis) = EncryptionTemplateAxis::parse(axis_name) {
                        if !axes.contains(&axis) {
                            axes.push(axis);
                        }
                    }
                    i += end + 2;
                    continue;
                }
            }
            i += 1;
        }
        Self {
            literal: literal.to_owned(),
            axes,
        }
    }
}

/// Phase L Tier 5 — closed catalog of template axes recognised inside
/// `EncryptionTemplate` literals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionTemplateAxis {
    TenantId,
    UserId,
    RecordId,
}

impl EncryptionTemplateAxis {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "tenant_id" => Some(Self::TenantId),
            "user_id" => Some(Self::UserId),
            "record_id" => Some(Self::RecordId),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TenantId => "tenant_id",
            Self::UserId => "user_id",
            Self::RecordId => "record_id",
        }
    }
}

/// Phase L Tier 5 — closed catalog of `@cap.Encrypted` algorithms.
/// v0: `Aes256Gcm` (paired 1:1 with `runtime/go/lazuli/encryption/aes_gcm.go`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionAlgorithm {
    Aes256Gcm,
}

impl EncryptionAlgorithm {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "aes_256_gcm" => Some(Self::Aes256Gcm),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "aes_256_gcm",
        }
    }
}

/// Phase L Tier 5 — closed catalog of `@cap.Encrypted` rotation
/// strategies. v0: `Manual`. `KmsManaged` is deferred to Wave 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncryptionRotation {
    Manual,
}

impl EncryptionRotation {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
        }
    }
}

/// Phase L Tier 5 — closed catalog of `@key.<scope>` references. Used
/// by doctor to validate templates against scopes; not stored in IR
/// (the `EncryptionBinding.scope` field preserves the authored
/// `@key.<scope>` string verbatim).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EncryptionKeyScope {
    App,
    Tenant,
    User,
    Record,
}

impl EncryptionKeyScope {
    /// Parse the verbatim `@key.<scope>` reference.
    pub fn parse(reference: &str) -> Option<Self> {
        let name = reference.strip_prefix("@key.")?;
        match name {
            "app" => Some(Self::App),
            "tenant" => Some(Self::Tenant),
            "user" => Some(Self::User),
            "record" => Some(Self::Record),
            _ => None,
        }
    }

    /// Required template axis for this scope. `None` for `App` (the
    /// global key has no per-axis derivation).
    pub fn required_axis(self) -> Option<EncryptionTemplateAxis> {
        match self {
            Self::App => None,
            Self::Tenant => Some(EncryptionTemplateAxis::TenantId),
            Self::User => Some(EncryptionTemplateAxis::UserId),
            Self::Record => Some(EncryptionTemplateAxis::RecordId),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses_single_axis() {
        let t = EncryptionTemplate::parse("CRYPT_KEY_TENANT_{tenant_id}");
        assert_eq!(t.axes, vec![EncryptionTemplateAxis::TenantId]);
    }

    #[test]
    fn template_parses_no_axes() {
        let t = EncryptionTemplate::parse("CRYPT_KEY_APP");
        assert!(t.axes.is_empty());
        assert_eq!(t.literal, "CRYPT_KEY_APP");
    }

    #[test]
    fn template_skips_unknown_axis() {
        let t = EncryptionTemplate::parse("CRYPT_KEY_{tenant_id}_{nope}");
        assert_eq!(t.axes, vec![EncryptionTemplateAxis::TenantId]);
    }

    #[test]
    fn algorithm_round_trip() {
        let a = EncryptionAlgorithm::parse("aes_256_gcm").unwrap();
        assert_eq!(a.as_str(), "aes_256_gcm");
        assert_eq!(a, EncryptionAlgorithm::Aes256Gcm);
    }

    #[test]
    fn rotation_round_trip() {
        let r = EncryptionRotation::parse("manual").unwrap();
        assert_eq!(r.as_str(), "manual");
    }

    #[test]
    fn key_scope_required_axes() {
        assert_eq!(
            EncryptionKeyScope::parse("@key.tenant").unwrap().required_axis(),
            Some(EncryptionTemplateAxis::TenantId)
        );
        assert_eq!(
            EncryptionKeyScope::parse("@key.user").unwrap().required_axis(),
            Some(EncryptionTemplateAxis::UserId)
        );
        assert_eq!(
            EncryptionKeyScope::parse("@key.record").unwrap().required_axis(),
            Some(EncryptionTemplateAxis::RecordId)
        );
        assert_eq!(
            EncryptionKeyScope::parse("@key.app").unwrap().required_axis(),
            None
        );
        assert!(EncryptionKeyScope::parse("@key.bogus").is_none());
    }
}
