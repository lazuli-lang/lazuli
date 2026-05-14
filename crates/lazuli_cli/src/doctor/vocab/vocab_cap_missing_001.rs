//! VOCAB-CAP-MISSING-001 — PII-shaped field name without an `@cap.*` tier.
//!
//! Fires when a resource field satisfies all three conditions:
//!   1. Type is `Text` or `@semantic.Email` (plain text-carrying type).
//!   2. Field name (lowercased, word-boundary split) contains a token from
//!      the closed PII catalog (identifiers / financial / health / government).
//!   3. The field carries no typed `@cap.*` capability tier
//!      (`@cap.Hashed`, `@cap.Encrypted`, `@cap.Token`, etc.).
//!
//! Severity: `warning` (strict-profile), `error` (production-profile).
//! Mirrors the profile graduation of VOCAB-AUDIT-001.
//! Reference: docs/proposals/doctor-vocabulary-lints.md §"Diagnostic shape"

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, TypeRef};

// ── PII token catalog (closed — additions require a new cell + proposal grade) ─

/// Single-word PII tokens. Matched against each underscore-delimited segment of
/// the lowercased field name to avoid substring false-positives (e.g. `passion`
/// must not match `ssn`).
const SINGLE_WORD_TOKENS: &[&str] = &[
    // Identifiers
    "ssn",
    "cpf",
    "cnpj",
    "passport",
    // Financial
    "salary",
    "income",
    "cvv",
    "iban",
    "swift",
    // Health / government
    "diagnosis",
    "prescription",
];

/// Multi-word (underscore-joined) PII tokens. Matched as a contiguous substring
/// of the lowercased field name — underscore delimiters make containment exact.
const MULTI_WORD_TOKENS: &[&str] = &[
    // Identifiers
    "tax_id",
    "national_id",
    "driver_license",
    // Financial
    "annual_revenue",
    "credit_card",
    "card_number",
    "routing_number",
    "account_number",
    // Health / government
    "medical_record",
    "voter_id",
    "tax_filing",
];

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-CAP-MISSING-001 finding: a PII-shaped field with no `@cap.*` tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource that owns the field.
    pub resource: String,
    /// Name of the offending field.
    pub field: String,
    /// The PII token that triggered the match.
    pub matched_token: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-CAP-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "field `{}.{}` name contains PII-sensitive token `{}` but carries no \
             `@cap.*` capability tier — add `@cap.Encrypted(key:@key.<scope>)`, \
             `@cap.Hashed(algorithm:argon2id)`, `@cap.Token(...)`, or another \
             explicit `@cap.*` tier. Unclassified PII fields drift from compliance \
             tooling contracts. See docs/invariants.md §\"@cap.*\".",
            self.resource, self.field, self.matched_token,
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-CAP-MISSING-001 for all resources in one feature.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.  The caller (doctor walker) maps each `Finding` into a
/// `DoctorDiagnostic`.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .flat_map(|r| {
            r.fields.iter().filter_map(|f| {
                if !is_unprotected_text_type(&f.type_ref) {
                    return None;
                }
                pii_token_match(&f.name).map(|token| Finding {
                    path: path.to_path_buf(),
                    resource: r.name.clone(),
                    field: f.name.clone(),
                    matched_token: token.to_owned(),
                })
            })
        })
        .collect()
}

// ── internals ─────────────────────────────────────────────────────────────────

/// Returns `true` when the field type is plain text-like AND carries no typed
/// `@cap.*` capability tier.
///
/// `TypeRef::Capability(_)` covers the typed tiers (`@cap.Hashed`,
/// `@cap.Encrypted`, `@cap.Token`, `@cap.File`).  `BuiltinType::CapSecret`
/// covers the legacy `@cap.Secret` spelling (doctor already warns on that).
/// Any future `@cap.Pii` / `@cap.Confidential` variants will be
/// `CapabilityRef` members once their bucket cycle lands, and this check
/// catches them automatically via the `Capability(_)` arm.
fn is_unprotected_text_type(type_ref: &TypeRef) -> bool {
    matches!(
        type_ref,
        TypeRef::Builtin(BuiltinType::Text | BuiltinType::SemanticEmail)
    )
    // TypeRef::Capability(_) → has @cap.* → not plain text → returns false
    // TypeRef::Builtin(CapSecret) → legacy @cap.Secret → not plain text → false
    // TypeRef::Builtin(Decimal | Integer | ...) → not text-carrying → false
}

/// Returns the first matching PII catalog token (owned) for `field_name`, or
/// `None` if no token matches.
///
/// Single-word tokens are checked with word-boundary semantics: the token must
/// equal a whole underscore-delimited segment of the lowercased name.
/// Multi-word tokens (already underscore-joined) are checked as contiguous
/// substrings — underscore delimiters make the containment unambiguous.
fn pii_token_match(field_name: &str) -> Option<&'static str> {
    let lower = field_name.to_lowercase();

    // Single-word: exact segment match prevents false positives from embedded
    // subsequences (e.g. `ssn` in a hypothetical `dissension_count`).
    for &token in SINGLE_WORD_TOKENS {
        if lower.split('_').any(|seg| seg == token) {
            return Some(token);
        }
    }

    // Multi-word: substring containment is unambiguous because tokens contain
    // underscores — `tag_id` cannot match `tax_id`.
    for &token in MULTI_WORD_TOKENS {
        if lower.contains(token) {
            return Some(token);
        }
    }

    None
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, CapabilityRef, Defaults, EncryptedCapability, Feature, Field, HashedCapability,
        HashAlgorithm, Policies, Resource, TypeRef,
    };

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required: true,
            unique: false,
            default: None,
            derived_from: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.into(),
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields,
            constraints: vec![],
            validate: None,
            validates: vec![],
            retention: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            workflows: vec![],
            jobs: vec![],
            webhooks: vec![],
            notifications: vec![],
            event_groups: vec![],
            tenant_migrations: vec![],
            translation: None,
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    // ── positive ──────────────────────────────────────────────────────────────

    /// `tax_id: Text required` — PII name with no @cap.* tier MUST fire.
    #[test]
    fn positive_tax_id_text_fires() {
        let field = mk_field("tax_id", TypeRef::Builtin(BuiltinType::Text));
        let resource = mk_resource("Employer", vec![field]);
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("features/employer/employer.lzi"));
        assert_eq!(findings.len(), 1, "expected one finding for tax_id: Text");
        assert_eq!(findings[0].field, "tax_id");
        assert_eq!(findings[0].resource, "Employer");
        assert_eq!(Finding::CODE, "VOCAB-CAP-MISSING-001");
        assert!(
            findings[0].message().contains("tax_id"),
            "message must name the field"
        );
        assert!(
            findings[0].message().contains("tax_id"),
            "matched_token must appear in message"
        );
    }

    // ── negative (i): @cap.Encrypted — must NOT fire ──────────────────────────

    /// `tax_id: @cap.Encrypted(key:@key.tenant) required` — protected, must not fire.
    #[test]
    fn negative_i_cap_encrypted_does_not_fire() {
        let cap = TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
            key: "@key.tenant".into(),
        }));
        let field = mk_field("tax_id", cap);
        let resource = mk_resource("Employer", vec![field]);
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "tax_id with @cap.Encrypted must not trigger VOCAB-CAP-MISSING-001"
        );
    }

    // ── negative (ii): tag_id — similar substring, different word boundary ────

    /// `tag_id: Text required` — `tag_id` shares no catalog token;
    /// must not fire even though it resembles `tax_id`.
    #[test]
    fn negative_ii_tag_id_does_not_fire() {
        let field = mk_field("tag_id", TypeRef::Builtin(BuiltinType::Text));
        let resource = mk_resource("Post", vec![field]);
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "tag_id must not match any PII catalog token"
        );
    }

    // ── negative (iii): @semantic.Email + @cap.Hashed — must NOT fire ─────────

    /// A PII-shaped email field that is explicitly hashed — the @cap.* tier
    /// removes this from the finding set even though the name matches.
    ///
    /// In the IR, `@semantic.Email @cap.Hashed(algorithm:argon2id)` surfaces as
    /// `TypeRef::Capability(CapabilityRef::Hashed(...))` — the @cap tier is
    /// the authoritative type_ref once applied.
    #[test]
    fn negative_iii_pii_name_with_cap_hashed_does_not_fire() {
        let cap = TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
            algorithm: HashAlgorithm::Argon2id,
        }));
        // Use `ssn` — a single-word PII token — to exercise the segment-match path.
        let field = mk_field("ssn", cap);
        let resource = mk_resource("Identity", vec![field]);
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "PII-shaped name with @cap.Hashed must not trigger VOCAB-CAP-MISSING-001"
        );
    }

    // ── word-boundary edge cases ───────────────────────────────────────────────

    /// Single-word token `ssn` must match when it IS a standalone segment.
    #[test]
    fn positive_ssn_segment_fires() {
        let field = mk_field("user_ssn", TypeRef::Builtin(BuiltinType::Text));
        let resource = mk_resource("Identity", vec![field]);
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1, "user_ssn must fire on ssn token");
        assert_eq!(findings[0].matched_token, "ssn");
    }

    /// Multi-word token `annual_revenue` must match across segment boundaries.
    #[test]
    fn positive_annual_revenue_fires() {
        let field = mk_field(
            "reported_annual_revenue",
            TypeRef::Builtin(BuiltinType::Text),
        );
        let resource = mk_resource("Company", vec![field]);
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(
            findings.len(),
            1,
            "reported_annual_revenue must fire on annual_revenue token"
        );
        assert_eq!(findings[0].matched_token, "annual_revenue");
    }
}
