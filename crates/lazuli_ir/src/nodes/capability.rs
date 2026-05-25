//! Typed `@cap.*` capability decorators — the IR shape behind every
//! `@cap.File(...)`, `@cap.PII(...)`, `@cap.Hashed(...)`, `@cap.Encrypted(...)`,
//! `@cap.E2ee(...)`, and `@cap.Token(...)` annotation on a resource field
//! or API output.
//!
//! Each variant of [`CapabilityRef`] is the IR-side image of a surface
//! `@cap.<Name>(<args>)` decorator. The lowering (surface → IR) lives in
//! `lazuli_analyzer::type_ref_from_syntax`; the doctor cross-checks (e.g.
//! `@cap.File` ↔ `object_storage` capability symmetry, signed-TTL
//! mutual-exclusion with `private` visibility) live in the existing
//! text-pattern doctor pipeline until the storage bucket cycle migrates
//! them onto the typed shape.
//!
//! ## When to add a new variant
//!
//! A new `@cap.<Name>(...)` decorator only earns a [`CapabilityRef`]
//! variant once its bucket-cycle proposal lands. The discriminated union
//! shape is deliberate: each variant gets a hand-shaped struct (no
//! free-form key/value bag) so codegen, doctor, and LSP all share a
//! statically-named payload.
//!
//! ## Visibility + signed URLs
//!
//! [`FileCapability::visibility`] defaults to `private` on a resource
//! field but is **required** on an API output (doctor warns on omission).
//! When `Signed`, [`FileCapability::signed_ttl`] must be present; the
//! mutual-exclusion against `Private` is enforced by doctor — the
//! language records what the author wrote.
//!
//! ## File-size literals
//!
//! [`FileSizeLiteral`] uses **binary** prefixes (1 KB = 1024 bytes,
//! 1 MB = 1024² bytes, …) to match the LSP literal catalog
//! (`is_file_size_literal`). [`FileSize`] carries both the parsed
//! byte count and the source-text literal so `lazuli inspect` round-trips
//! the author's notation cleanly.
//!
//! ## See also
//!
//! - `docs/proposals/encryption-vocab.md` — [`crate::E2eeCapability`]
//!   (lives in `crate::encryption` — sibling capability that the server
//!   stores ciphertext for but never reads)
//! - `docs/proposals/fileref-jsonb-fr3-design.md` — `@cap.File` →
//!   auto-photo command synthesis (FR-3a)

use serde::{Deserialize, Serialize};

use crate::E2eeCapability;

/// Discriminated union for typed capability decorators. New variants land
/// as the bucket cycles type each `@cap.*` family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CapabilityRef {
    File(FileCapability),
    /// `@cap.PII(class:<X>, retention:<dur>, log_redact:<bool>)` —
    /// marks fields as regulated personal data. Drives audit
    /// data_subject inference + auto-redaction at log emission.
    PII(PiiCapability),
    /// Phase L Tier 4 follow-up — `@cap.Hashed(algorithm:<X>)`.
    /// Closed catalog: `argon2id` canonical, `bcrypt` for legacy
    /// migration only.
    Hashed(HashedCapability),
    /// Phase L Tier 4 follow-up — `@cap.Encrypted(key:@key.<scope>)`.
    Encrypted(EncryptedCapability),
    /// Encryption bucket cycle — `@cap.E2ee(key:@key.<scope>)`.
    /// Sibling of `Encrypted`: the server stores ciphertext but
    /// never reads it. See `docs/proposals/encryption-vocab.md`.
    E2ee(E2eeCapability),
    /// Phase L Tier 4 follow-up — `@cap.Token(ttl:<duration>,
    /// single_use:<bool>,store:<storage>)`. `store` is `hashed` in v0.
    Token(TokenCapability),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PiiCapability {
    /// Free-form class name (`identity`, `contact`, `financial`,
    /// `medical`, `biometric`). Adapters/doctor cross-check against a
    /// registry-level catalog if declared.
    pub class: String,
    /// Optional retention period (duration literal, e.g. `"90d"`,
    /// `"7y"`). When set, doctor flags resources without a
    /// retention-cleanup job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<String>,
    /// When true, observability redaction must mask this field's
    /// values in log output. Default true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_redact: Option<bool>,
}

/// Phase L Tier 4 follow-up — typed `@cap.Hashed(algorithm:<X>)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashedCapability {
    pub algorithm: HashAlgorithm,
}

/// Phase L Tier 4 follow-up — closed catalog of `@cap.Hashed` algorithms.
/// `Argon2id` is canonical; `Bcrypt` is kept only for legacy migration
/// (doctor warns on new uses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HashAlgorithm {
    Argon2id,
    Bcrypt,
}

/// Phase L Tier 4 follow-up — typed `@cap.Encrypted(key:@key.<scope>)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptedCapability {
    /// `@key.<scope>` reference, stored verbatim with the `@key.`
    /// prefix preserved so cold-readers see the namespace.
    pub key: String,
}

/// Phase L Tier 4 follow-up — typed `@cap.Token(...)`. All three
/// dimensions (ttl/single_use/store) are mandatory in canonical v0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCapability {
    /// `ttl:<integer><s|m|h|d>` — duration literal preserved verbatim
    /// (adapter parses).
    pub ttl: String,
    /// `single_use:true|false`.
    pub single_use: bool,
    /// `store:hashed` — closed catalog `{Hashed}` in v0.
    pub store: TokenStore,
}

/// Phase L Tier 4 follow-up — closed catalog of token storage modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenStore {
    Hashed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCapability {
    pub max_size: FileSize,
    /// At least one MIME entry. The `|`-separated source form is
    /// normalised into a flat vector; a `*/*` wildcard authoring is
    /// represented as `family: "*", subtype: "*"`.
    pub accept: Vec<MimeType>,
    /// `None` is the parse-time default (`private` on a resource
    /// field, **required** on an api output — doctor warns on
    /// omission). The bucket-storage cycle proposal carries the
    /// closed catalog `{public, private, signed}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<FileVisibility>,
    /// `Some` only when `visibility == Signed`. Mutually exclusive
    /// with `visibility == Private` (doctor enforces); language
    /// records what the author wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signed_ttl: Option<String>,
    /// `auto_photo_policy: @policy.<name>` — explicit policy attached
    /// to the 4 commands synthesized from this `@cap.File` site
    /// (FR-3a). `None` defers to the analyzer's heuristic
    /// (resource_singular + "_only").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_photo_policy: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSize {
    pub bytes: u64,
    /// Source-text literal preserved for inspect round-trip.
    pub literal: FileSizeLiteral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", content = "amount")]
pub enum FileSizeLiteral {
    Kb(u32),
    Mb(u32),
    Gb(u32),
}

impl FileSizeLiteral {
    /// Convert the literal into a byte count. `kb` = 1024, `mb` = 1024*1024,
    /// `gb` = 1024*1024*1024 (binary prefixes, matching the LSP literal
    /// catalog `is_file_size_literal`).
    pub fn bytes(self) -> u64 {
        match self {
            Self::Kb(n) => n as u64 * 1024,
            Self::Mb(n) => n as u64 * 1024 * 1024,
            Self::Gb(n) => n as u64 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MimeType {
    /// IANA top-level family (`text`, `image`, `application`, `audio`,
    /// `video`, `font`) or wildcard `*`.
    pub family: String,
    /// Subtype, e.g. `csv`, `vnd.ms-excel`, `*`.
    pub subtype: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileVisibility {
    Public,
    Private,
    Signed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_size_literal_bytes_uses_binary_prefixes() {
        assert_eq!(FileSizeLiteral::Kb(1).bytes(), 1024);
        assert_eq!(FileSizeLiteral::Mb(1).bytes(), 1024 * 1024);
        assert_eq!(FileSizeLiteral::Gb(1).bytes(), 1024 * 1024 * 1024);
        assert_eq!(FileSizeLiteral::Mb(10).bytes(), 10 * 1024 * 1024);
    }

    #[test]
    fn file_visibility_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&FileVisibility::Public).unwrap(),
            "\"public\""
        );
        assert_eq!(
            serde_json::to_string(&FileVisibility::Private).unwrap(),
            "\"private\""
        );
        assert_eq!(
            serde_json::to_string(&FileVisibility::Signed).unwrap(),
            "\"signed\""
        );
    }

    #[test]
    fn hash_algorithm_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&HashAlgorithm::Argon2id).unwrap(),
            "\"argon2id\""
        );
        assert_eq!(
            serde_json::to_string(&HashAlgorithm::Bcrypt).unwrap(),
            "\"bcrypt\""
        );
    }

    #[test]
    fn file_size_literal_round_trips_with_unit_amount_envelope() {
        let lit = FileSizeLiteral::Mb(5);
        let json = serde_json::to_string(&lit).unwrap();
        let back: FileSizeLiteral = serde_json::from_str(&json).unwrap();
        assert_eq!(lit, back);
        assert!(json.contains("\"unit\":\"Mb\""));
        assert!(json.contains("\"amount\":5"));
    }
}
