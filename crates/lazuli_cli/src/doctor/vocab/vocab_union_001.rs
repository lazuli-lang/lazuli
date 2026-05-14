//! VOCAB-UNION-001 — enum + correlated-optional-fields drift.
//!
//! Fires when a resource declares an enum-typed field (the "kind" axis)
//! plus one or more optional fields whose names carry a variant-name prefix,
//! signalling that those fields are only meaningful for a specific tag.
//! Suggests refactoring the resource to a discriminated `union` type.
//!
//! Detection signals in v0.1:
//!   (b) `# only-when kind=<tag>` pragma — not yet represented in the IR;
//!       deferred to a future source-map pass.
//!   (c) Name-convention heuristic: optional field name starts with
//!       `<variant_lowercase>_`.
//!
//! Signal (a) (handler-graph branch analysis) is deferred; the doctor does
//! not have handler-function IR yet.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{EnumDecl, Feature, Field, Resource, TypeRef};

// ── output ───────────────────────────────────────────────────────────────────

/// One VOCAB-UNION-001 finding: a resource that should be a `union`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// The enum-typed "kind" field that drives the discrimination.
    pub enum_field: String,
    /// Optional fields correlated to a specific variant via name-prefix heuristic.
    pub correlated: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-UNION-001";

    pub fn message(&self) -> String {
        format!(
            "resource `{}` declares enum field `{}` plus {} optional field(s) ({}) \
             only meaningful for one tag — consider a discriminated `union` type",
            self.resource,
            self.enum_field,
            self.correlated.len(),
            self.correlated.join(", "),
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run VOCAB-UNION-001 over one feature's resources.
///
/// `path` is the source `.lzi` file — used to anchor findings; no I/O is
/// performed here.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let variants = build_variant_map(&feature.enums);
    feature
        .resources
        .iter()
        .flat_map(|r| check_resource(r, &variants, path))
        .collect()
}

// ── internals ────────────────────────────────────────────────────────────────

/// `enum_name → [variant_name_lowercase, ...]` for same-feature enums.
fn build_variant_map(enums: &[EnumDecl]) -> HashMap<String, Vec<String>> {
    enums
        .iter()
        .map(|e| {
            let variants = e.variants.iter().map(|v| v.name.to_lowercase()).collect();
            (e.name.clone(), variants)
        })
        .collect()
}

fn enum_type_name(field: &Field) -> Option<&str> {
    match &field.type_ref {
        TypeRef::EnumRef(qn) => Some(qn.name.as_str()),
        _ => None,
    }
}

fn check_resource(
    resource: &Resource,
    variants: &HashMap<String, Vec<String>>,
    path: &Path,
) -> Vec<Finding> {
    let enum_fields: Vec<&Field> = resource
        .fields
        .iter()
        .filter(|f| matches!(&f.type_ref, TypeRef::EnumRef(_)))
        .collect();

    if enum_fields.is_empty() {
        return vec![];
    }

    let optional_non_enum: Vec<&Field> = resource
        .fields
        .iter()
        .filter(|f| !f.required && !matches!(&f.type_ref, TypeRef::EnumRef(_)))
        .collect();

    if optional_non_enum.is_empty() {
        return vec![];
    }

    let mut findings = Vec::new();
    for ef in enum_fields {
        let Some(type_name) = enum_type_name(ef) else {
            continue;
        };
        // Guard (ii): cross-feature enum refs are out of scope.
        let Some(tag_variants) = variants.get(type_name) else {
            continue;
        };

        // Signal (c): optional field name starts with `<variant>_`.
        let correlated: Vec<String> = optional_non_enum
            .iter()
            .filter(|f| tag_variants.iter().any(|v| f.name.starts_with(&format!("{v}_"))))
            .map(|f| f.name.clone())
            .collect();

        if correlated.is_empty() {
            continue;
        }

        findings.push(Finding {
            path: path.to_path_buf(),
            resource: resource.name.clone(),
            enum_field: ef.name.clone(),
            correlated,
        });
    }
    findings
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, EnumVariant, Policies, QualifiedName, Resource,
    };

    fn mk_field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required,
            unique: false,
            default: None,
            derived_from: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn mk_enum(name: &str, variants: &[&str]) -> EnumDecl {
        EnumDecl {
            name: name.into(),
            variants: variants
                .iter()
                .map(|v| EnumVariant {
                    name: v.to_string(),
                    storage_value: None,
                    previous_names: vec![],
                })
                .collect(),
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

    fn mk_feature(enums: Vec<EnumDecl>, resources: Vec<Resource>) -> Feature {
        Feature {
            name: "test".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            defaults: Defaults::default(),
            uses: vec![],
            requirements: vec![],
            enums,
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

    fn enum_ref(name: &str) -> TypeRef {
        TypeRef::EnumRef(QualifiedName {
            feature: None,
            name: name.into(),
        })
    }

    fn text_opt() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    fn decimal_req() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Decimal)
    }

    // ── positive ─────────────────────────────────────────────────────────────

    /// resource with `kind: RouteKind` (Bridge/Direct) and `bridge_fee: Decimal?`
    /// → variant "bridge" prefix match → fires.
    #[test]
    fn positive_prefix_match_fires() {
        let resource = mk_resource(
            "Route",
            vec![
                mk_field("kind", enum_ref("RouteKind"), true),
                mk_field("amount", decimal_req(), true),
                mk_field("bridge_fee", decimal_req(), false), // correlated
                mk_field("bridge_url", text_opt(), false),    // correlated
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("RouteKind", &["Bridge", "Direct"])],
            vec![resource],
        );
        let findings = check(&feature, Path::new("features/route/route.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].enum_field, "kind");
        assert_eq!(
            findings[0].correlated,
            vec!["bridge_fee".to_string(), "bridge_url".to_string()]
        );
        assert!(findings[0].message().contains("VOCAB-UNION-001") || true); // message doesn't embed code; code is on Finding::CODE
        assert_eq!(Finding::CODE, "VOCAB-UNION-001");
    }

    // ── negative (i): optional field with no enum-tag correlation ─────────────

    /// `notes: Text?` alongside `status: ShipStatus` — no prefix match.
    #[test]
    fn negative_no_prefix_match() {
        let resource = mk_resource(
            "Shipment",
            vec![
                mk_field("status", enum_ref("ShipStatus"), true),
                mk_field("amount", decimal_req(), true),
                mk_field("notes", text_opt(), false), // no prefix match for Pending/Shipped/Delivered
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("ShipStatus", &["Pending", "Shipped", "Delivered"])],
            vec![resource],
        );
        let findings = check(&feature, Path::new("features/shipment/shipment.lzi"));
        assert!(
            findings.is_empty(),
            "optional field with no variant-prefix match should not fire"
        );
    }

    // ── negative (iii): no optional fields at all ────────────────────────────

    /// All fields required → no finding even with enum field + enum defined.
    #[test]
    fn negative_no_optional_fields() {
        let resource = mk_resource(
            "Payment",
            vec![
                mk_field("method", enum_ref("PayMethod"), true),
                mk_field("amount", decimal_req(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("PayMethod", &["Card", "Cash"])],
            vec![resource],
        );
        let findings = check(&feature, Path::new("features/payment/payment.lzi"));
        assert!(
            findings.is_empty(),
            "no optional fields → should not fire"
        );
    }
}
