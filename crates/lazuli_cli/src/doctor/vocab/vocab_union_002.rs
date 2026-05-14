//! VOCAB-UNION-002 — polymorphic enum-tag + bare-ID FK anti-pattern.
//!
//! Fires when a resource pairs an `enum`-typed discriminator field with a
//! plain `ID`-typed foreign key named `<enum-field>_id`.  The bare `ID` is
//! untyped: the DB has no FK constraint and the codegen cannot emit a typed
//! accessor.  The refactor is a discriminated `union` with one typed FK per
//! variant.
//!
//! Canonical example (`examples/comment.lzi`):
//!
//! ```lzi
//! resource Comment
//!   target:    CommentTarget required   # enum → discriminator
//!   target_id: ID required              # bare ID → FIRES
//! ```
//!
//! Suggested refactor:
//!
//! ```lzi
//! union Comment
//!   Issue
//!     target: Issue required
//!   Customer
//!     target: Customer required
//! ```
//!
//! Correlation rule: an ID field named exactly `<enum-field-name>_id` is
//! considered correlated.  Cross-feature enum refs are out of scope (no
//! local variant data to validate against).

use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Field, Resource, TypeRef, BuiltinType};

// ── output ───────────────────────────────────────────────────────────────────

/// One VOCAB-UNION-002 finding: a resource pairing an enum discriminator
/// with a bare `ID` FK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// The enum-typed discriminator field (e.g. `target`).
    pub enum_field: String,
    /// The bare `ID` field correlated to it (e.g. `target_id`).
    pub untyped_id_field: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-UNION-002";

    pub fn message(&self) -> String {
        format!(
            "resource `{}` pairs enum field `{}` with bare `ID` field `{}` — \
             consider a discriminated `union` type with a typed FK per variant",
            self.resource,
            self.enum_field,
            self.untyped_id_field,
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run VOCAB-UNION-002 over one feature's resources.
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

// ── internals ────────────────────────────────────────────────────────────────

fn is_bare_id(field: &Field) -> bool {
    matches!(&field.type_ref, TypeRef::Builtin(BuiltinType::Id))
}

fn check_resource(resource: &Resource, path: &Path) -> Vec<Finding> {
    let enum_fields: Vec<&Field> = resource
        .fields
        .iter()
        .filter(|f| matches!(&f.type_ref, TypeRef::EnumRef(_)))
        .collect();

    if enum_fields.is_empty() {
        return vec![];
    }

    let id_fields: Vec<&Field> = resource
        .fields
        .iter()
        .filter(|f| is_bare_id(f))
        .collect();

    if id_fields.is_empty() {
        return vec![];
    }

    let mut findings = Vec::new();
    for ef in enum_fields {
        let expected = format!("{}_id", ef.name);
        if let Some(id_field) = id_fields.iter().find(|f| f.name == expected) {
            findings.push(Finding {
                path: path.to_path_buf(),
                resource: resource.name.clone(),
                enum_field: ef.name.clone(),
                untyped_id_field: id_field.name.clone(),
            });
        }
    }
    findings
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, EnumDecl, EnumVariant, Field, Policies, QualifiedName, Resource,
        Feature,
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

    fn user_def(name: &str) -> TypeRef {
        TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: name.into(),
        })
    }

    fn bare_id() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Id)
    }

    fn text_req() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    // ── positive ──────────────────────────────────────────────────────────────

    /// `target: CommentTarget required` + `target_id: ID required` → fires.
    #[test]
    fn positive_enum_plus_bare_id_correlated() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field("target", enum_ref("CommentTarget"), true),
                mk_field("target_id", bare_id(), true),
                mk_field("body", text_req(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );
        let findings = check(&feature, Path::new("features/comment/comment.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Comment");
        assert_eq!(findings[0].enum_field, "target");
        assert_eq!(findings[0].untyped_id_field, "target_id");
        assert_eq!(Finding::CODE, "VOCAB-UNION-002");
        assert!(findings[0].message().contains("target_id"));
    }

    // ── negative (i): typed FK — no enum, resource ref only ───────────────────

    /// `target: User required` is a typed FK (UserDefined), not an enum.
    /// No enum field present → should not fire.
    #[test]
    fn negative_typed_fk_no_enum() {
        let resource = mk_resource(
            "Assignment",
            vec![
                mk_field("target", user_def("User"), true),
                mk_field("target_id", bare_id(), true),
                mk_field("body", text_req(), true),
            ],
        );
        let feature = mk_feature(vec![], vec![resource]);
        let findings = check(&feature, Path::new("features/assignment/assignment.lzi"));
        assert!(
            findings.is_empty(),
            "typed FK with no enum field should not fire"
        );
    }

    // ── negative (ii): standalone bare ID with no enum partner ────────────────

    /// `tag_id: ID required` exists but there is no enum field named `tag`.
    #[test]
    fn negative_bare_id_no_enum_partner() {
        let resource = mk_resource(
            "Attachment",
            vec![
                mk_field("tag_id", bare_id(), true),
                mk_field("body", text_req(), true),
            ],
        );
        let feature = mk_feature(vec![], vec![resource]);
        let findings = check(&feature, Path::new("features/attachment/attachment.lzi"));
        assert!(
            findings.is_empty(),
            "standalone bare ID with no enum partner should not fire"
        );
    }

    // ── negative (iii): enum + ID not correlated by name ──────────────────────

    /// `status: ShipStatus required` + `notes_id: ID required` — the ID
    /// field name does not match `status_id`, so no correlation.
    #[test]
    fn negative_enum_and_id_not_correlated() {
        let resource = mk_resource(
            "Shipment",
            vec![
                mk_field("status", enum_ref("ShipStatus"), true),
                mk_field("notes_id", bare_id(), true),
                mk_field("amount", text_req(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("ShipStatus", &["Pending", "Shipped"])],
            vec![resource],
        );
        let findings = check(&feature, Path::new("features/shipment/shipment.lzi"));
        assert!(
            findings.is_empty(),
            "enum + ID with mismatched names (status vs notes_id) should not fire"
        );
    }
}
