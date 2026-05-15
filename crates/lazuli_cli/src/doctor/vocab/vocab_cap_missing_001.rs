//! VOCAB-CAP-MISSING-001 - `@pii.*` field without a crypto/storage capability.
//!
//! Fires when a resource field carries a sensitive `@pii.<class>` marker but
//! the field type is not one of the explicit capability tiers that change
//! storage semantics: `@cap.Hashed`, `@cap.Encrypted`, `@cap.E2ee`, or
//! `@cap.Token`.
//!
//! `Field` does not yet preserve trailing decorator markers in IR, so the
//! rule exposes two entry points:
//! - `check` keeps the closed-catalog module API and becomes active once PII
//!   markers are lifted into `Field`.
//! - `check_source` reads raw `.lzi` source for golden fixture coverage without
//!   changing the IR shape in this cell.
//!
//! Severity: `error` (strict), `warning` (prototype).

use std::path::{Path, PathBuf};

use lazuli_ir::{CapabilityRef, Feature, Field, Resource, TypeRef};

const SENSITIVE_PII_TAGS: &[&str] = &[
    "contact",
    "financial",
    "health",
    "government_id",
    "auth_secret",
    "external",
    // Existing fixtures/docs still use these names. Treat them as aliases for
    // the same sensitive classes instead of silently allowing plaintext.
    "credential",
    "identifier",
];

const PII_CARVE_OUT_TAGS: &[&str] = &["derived", "public"];

// -- output ------------------------------------------------------------------

/// One VOCAB-CAP-MISSING-001 finding: a sensitive `@pii.*` resource field
/// without an explicit storage capability tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// Field name.
    pub field: String,
    /// Sensitive PII tag, without the `@pii.` prefix.
    pub pii_tag: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-CAP-MISSING-001";

    pub fn message(&self) -> String {
        format!(
            "field `{}.{}` carries `@pii.{}` but no `@cap.Hashed/Encrypted/Token` - sensitive data stored in plaintext",
            self.resource, self.field, self.pii_tag
        )
    }
}

// -- detection ---------------------------------------------------------------

/// Run VOCAB-CAP-MISSING-001 over one feature's resources.
///
/// Current IR drops trailing `@pii.*` field decorators during lowering. This
/// function therefore only evaluates fields whose PII tags are available to
/// this module (none today), preserving the vocabulary lint API without adding
/// a new field to `lazuli_ir::Field`.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .flat_map(|resource| {
            resource
                .fields
                .iter()
                .filter_map(|field| check_field(resource, field, &[], path))
        })
        .collect()
}

/// Run VOCAB-CAP-MISSING-001 over raw `.lzi` source text.
///
/// This is the practical entry point until resource-field `@pii.*` markers are
/// lifted into IR. It intentionally only scans resource field lines.
pub fn check_source(source: &str, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut current_resource: Option<(String, usize)> = None;

    for raw_line in source.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim_end();
        let trimmed = line.trim_start();

        if trimmed.is_empty() {
            continue;
        }

        let indent = leading_spaces(line);

        if let Some(name) = resource_name(trimmed) {
            current_resource = Some((name.to_owned(), indent));
            continue;
        }

        let Some((resource, resource_indent)) = current_resource.as_ref() else {
            continue;
        };

        if indent <= *resource_indent {
            current_resource = None;
            continue;
        }

        let Some((field_name, rhs)) = parse_field_line(trimmed) else {
            continue;
        };

        let pii_tags = pii_tags_in(rhs);
        if pii_tags.is_empty() {
            continue;
        }

        if has_capability_text(rhs) || has_derived_marker(rhs) {
            continue;
        }

        if let Some(tag) = first_sensitive_pii_tag(&pii_tags) {
            findings.push(Finding {
                path: path.to_path_buf(),
                resource: resource.clone(),
                field: field_name.to_owned(),
                pii_tag: tag.to_owned(),
            });
        }
    }

    findings
}

// -- internals ---------------------------------------------------------------

fn check_field(
    resource: &Resource,
    field: &Field,
    pii_tags: &[&str],
    path: &Path,
) -> Option<Finding> {
    if pii_tags.is_empty()
        || has_sensitive_capability(&field.type_ref)
        || is_explicitly_derived(field)
    {
        return None;
    }

    first_sensitive_pii_tag(pii_tags).map(|tag| Finding {
        path: path.to_path_buf(),
        resource: resource.name.clone(),
        field: field.name.clone(),
        pii_tag: tag.to_owned(),
    })
}

fn first_sensitive_pii_tag<'a>(pii_tags: &'a [&str]) -> Option<&'a str> {
    pii_tags
        .iter()
        .copied()
        .filter(|tag| !PII_CARVE_OUT_TAGS.contains(tag))
        .find(|tag| SENSITIVE_PII_TAGS.contains(tag))
}

fn has_sensitive_capability(type_ref: &TypeRef) -> bool {
    match type_ref {
        TypeRef::Capability(
            CapabilityRef::Hashed(_)
            | CapabilityRef::Encrypted(_)
            | CapabilityRef::E2ee(_)
            | CapabilityRef::Token(_),
        ) => true,
        TypeRef::Many(inner) => has_sensitive_capability(inner),
        _ => false,
    }
}

fn is_explicitly_derived(field: &Field) -> bool {
    field.derived_from.is_some()
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn resource_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("resource ")?;
    rest.split_whitespace().next()
}

fn parse_field_line(trimmed: &str) -> Option<(&str, &str)> {
    let (name, rhs) = trimmed.split_once(':')?;
    let name = name.trim();
    if name.is_empty()
        || name.contains(' ')
        || matches!(
            name,
            "route" | "input" | "output" | "policy" | "read" | "write" | "emits" | "audit"
        )
    {
        return None;
    }
    Some((name, rhs.trim()))
}

fn pii_tags_in(text: &str) -> Vec<&str> {
    let mut tags = Vec::new();
    let mut rest = text;

    while let Some(idx) = rest.find("@pii.") {
        let after = &rest[idx + "@pii.".len()..];
        let end = after
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(after.len());
        if end > 0 {
            tags.push(&after[..end]);
        }
        rest = &after[end..];
    }

    tags
}

fn has_capability_text(text: &str) -> bool {
    text.contains("@cap.Hashed")
        || text.contains("@cap.Encrypted")
        || text.contains("@cap.E2ee")
        || text.contains("@cap.Token")
}

fn has_derived_marker(text: &str) -> bool {
    text.contains(" derived from ")
}

// -- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, E2eeCapability, EncryptedCapability, HashAlgorithm,
        HashedCapability, Policies, Resource, TokenCapability, TokenStore,
    };

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required: false,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_derived_field(name: &str, type_ref: TypeRef) -> Field {
        Field {
            derived_from: Some("score > 80".to_owned()),
            ..mk_field(name, type_ref)
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
            invariants: vec![],
        }
    }

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "test".into(),
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
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            auth: None,
            surfaces: vec![],
            extensions: vec![],
            escape_routes: vec![],
            agents: vec![],
            reports: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn text() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    fn hashed() -> TypeRef {
        TypeRef::Capability(CapabilityRef::Hashed(HashedCapability {
            algorithm: HashAlgorithm::Argon2id,
        }))
    }

    fn encrypted() -> TypeRef {
        TypeRef::Capability(CapabilityRef::Encrypted(EncryptedCapability {
            key: "@key.tenant".to_owned(),
        }))
    }

    fn e2ee() -> TypeRef {
        TypeRef::Capability(CapabilityRef::E2ee(E2eeCapability {
            key: "@key.actor".to_owned(),
        }))
    }

    fn token() -> TypeRef {
        TypeRef::Capability(CapabilityRef::Token(TokenCapability {
            ttl: "1h".to_owned(),
            single_use: true,
            store: TokenStore::Hashed,
        }))
    }

    fn many(inner: TypeRef) -> TypeRef {
        TypeRef::Many(Box::new(inner))
    }

    #[test]
    fn positive_sensitive_pii_without_capability_fires() {
        let resource = mk_resource("Customer", vec![mk_field("email", text())]);
        let finding = check_field(
            &resource,
            &resource.fields[0],
            &["contact"],
            Path::new("features/customer/customer.lzi"),
        )
        .expect("sensitive PII without capability must fire");

        assert_eq!(Finding::CODE, "VOCAB-CAP-MISSING-001");
        assert_eq!(finding.resource, "Customer");
        assert_eq!(finding.field, "email");
        assert_eq!(finding.pii_tag, "contact");
        assert_eq!(
            finding.message(),
            "field `Customer.email` carries `@pii.contact` but no `@cap.Hashed/Encrypted/Token` - sensitive data stored in plaintext"
        );
    }

    #[test]
    fn negative_capability_present_does_not_fire() {
        for type_ref in [hashed(), encrypted(), e2ee(), token()] {
            let resource = mk_resource("Customer", vec![mk_field("secret", type_ref)]);
            assert!(
                check_field(
                    &resource,
                    &resource.fields[0],
                    &["auth_secret"],
                    Path::new("features/customer/customer.lzi")
                )
                .is_none(),
                "capability-protected PII must not fire"
            );
        }
    }

    #[test]
    fn carve_out_derived_pii_does_not_fire() {
        let resource = mk_resource("Customer", vec![mk_field("score", text())]);
        assert!(
            check_field(
                &resource,
                &resource.fields[0],
                &["derived"],
                Path::new("features/customer/customer.lzi")
            )
            .is_none(),
            "@pii.derived is computed/classification-only and must not require a capability"
        );

        let resource = mk_resource("Customer", vec![mk_derived_field("risk", text())]);
        assert!(
            check_field(
                &resource,
                &resource.fields[0],
                &["financial"],
                Path::new("features/customer/customer.lzi")
            )
            .is_none(),
            "`derived from` fields are explicit computed fields"
        );
    }

    #[test]
    fn many_of_capability_does_not_fire() {
        let resource = mk_resource("Session", vec![mk_field("tokens", many(token()))]);
        assert!(
            check_field(
                &resource,
                &resource.fields[0],
                &["auth_secret"],
                Path::new("features/session/session.lzi")
            )
            .is_none(),
            "Many<@cap.Token> carries the capability on the inner type"
        );
    }

    #[test]
    fn golden_source_fixture_fires_for_plaintext_pii_field() {
        let source =
            include_str!("../../../../../tests/golden/vocab/cap-missing/pii-contact-plaintext.lzi");

        let findings = check_source(
            source,
            Path::new("tests/golden/vocab/cap-missing/plain.lzi"),
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Customer");
        assert_eq!(findings[0].field, "email");
        assert_eq!(findings[0].pii_tag, "contact");
    }

    #[test]
    fn check_api_remains_noop_until_pii_is_lifted_into_field_ir() {
        let resource = mk_resource("Customer", vec![mk_field("email", text())]);
        let feature = mk_feature(vec![resource]);

        assert!(check(&feature, Path::new("features/customer/customer.lzi")).is_empty());
    }
}
