//! VOCAB-CAP-MISSING-001 — PII-shaped fields without `@cap.*` capability tier.
//!
//! Fires when a resource field has a PII-suggesting name (`tax_id`, `ssn`,
//! `annual_revenue`, etc.) but its declared type carries no capability
//! tier. The vocabulary already names this: `@cap.PII` / `@cap.Encrypted` /
//! `@cap.Token` / `@cap.Hashed`. The fix is to declare the appropriate
//! tier — doctor does not propose a specific one (different fields want
//! different tiers).
//!
//! Severity: `warning` (strict-profile), `error` (production-profile).
//! Reference: docs/proposals/doctor-vocabulary-lints.md (catalog v2 entry).

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Field, Resource, TypeRef};

const PII_NAMES: &[&str] = &[
    "ssn",
    "cpf",
    "cnpj",
    "tax_id",
    "tax_number",
    "vat",
    "vat_id",
    "national_id",
    "passport",
    "passport_number",
    "credit_card",
    "card_number",
    "iban",
    "bank_account",
    "annual_revenue",
    "salary",
    "income",
    "date_of_birth",
    "birth_date",
    "dob",
    "drivers_license",
    "license_number",
    "email",
    "phone",
    "phone_number",
    "mobile",
    "address_line",
    "street_address",
    "ip_address",
];

// ── output ────────────────────────────────────────────────────────────────────

/// One VOCAB-CAP-MISSING-001 finding: a PII-shaped field with no `@cap.*`
/// capability tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// Field name flagged as PII-shaped.
    pub field: String,
    /// PII catalog token matched by exact or `_token` suffix comparison.
    pub matched_pii_token: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-CAP-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "field `{}.{}` matches PII-name `{}` but declares no `@cap.*` capability tier \
             — add `@cap.PII`, `@cap.Encrypted`, `@cap.Token`, or `@cap.Hashed` as appropriate. \
             Capability tiers give compliance + crypto tooling a typed contract.",
            self.resource, self.field, self.matched_pii_token
        )
    }
}

// ── detection ─────────────────────────────────────────────────────────────────

/// Run VOCAB-CAP-MISSING-001 over one feature's resources.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .flat_map(|r| check_resource(r, path))
        .collect()
}

// ── internals ─────────────────────────────────────────────────────────────────

fn check_resource(resource: &Resource, path: &Path) -> Vec<Finding> {
    resource
        .fields
        .iter()
        .filter_map(|field| {
            if has_capability_tier(field) || has_semantic_pii_tier(field) {
                return None;
            }
            matched_pii_token(&field.name).map(|token| Finding {
                path: path.to_path_buf(),
                resource: resource.name.clone(),
                field: field.name.clone(),
                matched_pii_token: token.to_owned(),
            })
        })
        .collect()
}

fn matched_pii_token(field_name: &str) -> Option<&'static str> {
    let lower = field_name.to_ascii_lowercase();
    PII_NAMES
        .iter()
        .copied()
        .find(|token| lower == *token || lower.ends_with(&format!("_{token}")))
}

fn has_capability_tier(field: &Field) -> bool {
    matches!(&field.type_ref, TypeRef::Capability(_))
}

fn has_semantic_pii_tier(field: &Field) -> bool {
    match &field.type_ref {
        TypeRef::UserDefined(qn) if qn.feature.is_none() => {
            let lower = qn.name.to_ascii_lowercase();
            lower == "@semantic.email" || lower == "@semantic.phone"
        }
        _ => false,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, CapabilityRef, Defaults, Feature, Field, HashAlgorithm, HashedCapability,
        Policies, QualifiedName, Resource, TypeRef,
    };

    // ── helpers ──────────────────────────────────────────────────────────────

    fn qn(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required,
            unique: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
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
            lifecycle: None,
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
            pollers: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn text_type() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    fn int_type() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Integer)
    }

    fn user_type() -> TypeRef {
        TypeRef::UserDefined(qn("User"))
    }

    fn semantic_email_type() -> TypeRef {
        TypeRef::UserDefined(qn("@semantic.Email"))
    }

    fn cap_pii_type() -> TypeRef {
        TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
            algorithm: HashAlgorithm::Argon2id,
        }))
    }

    // ── positive ─────────────────────────────────────────────────────────────

    /// `tax_id: Text` with no `@cap.*` tier — MUST fire.
    #[test]
    fn positive_tax_id_text_no_cap_fires() {
        let resource = mk_resource("Customer", vec![mk_field("tax_id", text_type(), true)]);
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("features/customer/customer.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Customer");
        assert_eq!(findings[0].field, "tax_id");
        assert_eq!(findings[0].matched_pii_token, "tax_id");
        assert_eq!(Finding::CODE, "VOCAB-CAP-MISSING-001");
        assert!(findings[0].message().contains("@cap.*"));
    }

    /// `annual_revenue: Int` with no `@cap.*` tier — MUST fire.
    #[test]
    fn positive_annual_revenue_int_no_cap_fires() {
        let resource = mk_resource(
            "Customer",
            vec![mk_field("annual_revenue", int_type(), false)],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("features/customer/customer.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "annual_revenue");
        assert_eq!(findings[0].matched_pii_token, "annual_revenue");
    }

    /// `customer_phone: Text` suffix-matches PII token `phone` — MUST fire.
    #[test]
    fn positive_customer_phone_prefix_match_fires() {
        let resource = mk_resource(
            "Customer",
            vec![mk_field("customer_phone", text_type(), false)],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("features/customer/customer.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "customer_phone");
        assert_eq!(findings[0].matched_pii_token, "phone");
    }

    // ── negative (i): `@cap.*` tier — must NOT fire ──────────────────────────

    /// `email: @cap.PII` shape represented by `TypeRef::Capability(_)` — no finding.
    #[test]
    fn negative_email_with_cap_pii_does_not_fire() {
        let resource = mk_resource("User", vec![mk_field("email", cap_pii_type(), true)]);
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("features/user/user.lzi")).is_empty(),
            "field with TypeRef::Capability(_) must not trigger VOCAB-CAP-MISSING-001"
        );
    }

    // ── negative (ii): user-FK shape — must NOT fire ─────────────────────────

    /// `user: User required` is an FK; `user` is not a PII catalog token.
    #[test]
    fn negative_user_fk_does_not_fire() {
        let resource = mk_resource("Post", vec![mk_field("user", user_type(), true)]);
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("features/post/post.lzi")).is_empty(),
            "plain user FK must not trigger VOCAB-CAP-MISSING-001"
        );
    }

    // ── negative (iii): normal field — must NOT fire ─────────────────────────

    /// `title: Text` carries no PII-shaped token.
    #[test]
    fn negative_normal_field_does_not_fire() {
        let resource = mk_resource("Post", vec![mk_field("title", text_type(), true)]);
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("features/post/post.lzi")).is_empty(),
            "normal field name must not trigger VOCAB-CAP-MISSING-001"
        );
    }

    // ── negative (iv): semantic PII type — must NOT fire ─────────────────────

    /// `email: @semantic.Email` is already tier-aware at the semantic layer.
    #[test]
    fn negative_email_with_semantic_type_does_not_fire() {
        let resource = mk_resource("User", vec![mk_field("email", semantic_email_type(), true)]);
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("features/user/user.lzi")).is_empty(),
            "@semantic.Email field must not trigger VOCAB-CAP-MISSING-001"
        );
    }

    // ── additional coverage ───────────────────────────────────────────────────

    /// Mixed bag: guarded PII, unguarded PII, and normal fields — only the
    /// unguarded PII-shaped fields fire.
    #[test]
    fn mixed_only_unguarded_fires() {
        let resource = mk_resource(
            "Customer",
            vec![
                mk_field("email", cap_pii_type(), true),
                mk_field("customer_phone", text_type(), false),
                mk_field("title", text_type(), true),
                mk_field("billing_address_line", text_type(), false),
                mk_field("user", user_type(), true),
            ],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("features/customer/customer.lzi"));
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings
                .iter()
                .map(|f| f.field.as_str())
                .collect::<Vec<_>>(),
            vec!["customer_phone", "billing_address_line"]
        );
    }
}
