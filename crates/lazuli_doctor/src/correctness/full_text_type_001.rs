//! FULL-TEXT-TYPE-001 — `@full_text` decorator on non-text field.
//!
//! Doctor correctness rule. Fires when a field carries the
//! `@full_text` decorator but its type is not text-like. Postgres
//! `to_tsvector('english', col)` only makes sense for text columns;
//! numeric / boolean / date / JSON fields can't be tokenized for
//! full-text search.
//!
//! Allowed types: `Text`, and the semantic string variants
//! (`@semantic.Email`, `@semantic.Phone`, `@semantic.Url`,
//! `@semantic.Uuid`, `@semantic.Currency`).
//!
//! Reference: docs/roadmap.md §1.5.

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, Field, TypeRef};

// ── output ───────────────────────────────────────────────────────────────────

/// One FULL-TEXT-TYPE-001 finding — `@full_text` was attached to a
/// field whose type isn't in the text-like catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending field lives in.
    pub path: PathBuf,
    /// Feature owning the resource.
    pub feature: String,
    /// Resource carrying the offending field.
    pub resource: String,
    /// Field name carrying the misapplied `@full_text` marker.
    pub field: String,
    /// Verbatim text describing the field's type (for the message).
    pub field_type: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "FULL-TEXT-TYPE-001";

    /// Render the "@full_text only applies to text-like types" message
    /// listing the catalog members the author can switch to.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::full_text_type_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "catalog".into(),
    ///     resource: "Property".into(),
    ///     field: "price".into(),
    ///     field_type: "Integer".into(),
    /// };
    /// assert!(f.message().contains("text-like types"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "field `{}.{}` carries `@full_text` but its type is `{}` — `@full_text` only applies to text-like types \
             (`Text`, `@semantic.Email`, `@semantic.Phone`, `@semantic.Url`, `@semantic.Uuid`, `@semantic.Currency`). \
             Either change the type or drop `@full_text`.",
            self.resource, self.field, self.field_type
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run FULL-TEXT-TYPE-001 over one feature's resources.
///
/// Emits one finding per field whose `@full_text` marker is attached
/// to a non-text-like type.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::full_text_type_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with @full_text fields");
/// let _ = check(&feature, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for resource in &feature.resources {
        for field in &resource.fields {
            if !field.full_text {
                continue;
            }
            if is_text_like(&field.type_ref) {
                continue;
            }
            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                resource: resource.name.clone(),
                field: field.name.clone(),
                field_type: render_type(field),
            });
        }
    }
    out
}

fn is_text_like(type_ref: &TypeRef) -> bool {
    matches!(
        type_ref,
        TypeRef::Builtin(
            BuiltinType::Text
                | BuiltinType::SemanticEmail
                | BuiltinType::SemanticPhone
                | BuiltinType::SemanticUrl
                | BuiltinType::SemanticUuid
                | BuiltinType::SemanticCurrency
                // W1 GAP-04 — HexColor is a text carrier, so a GIN
                // full-text index is admissible (Percentage is numeric and
                // intentionally omitted).
                | BuiltinType::SemanticHexColor
        )
    )
}

fn render_type(field: &Field) -> String {
    match &field.type_ref {
        TypeRef::Builtin(b) => format!("{:?}", b),
        TypeRef::UserDefined(qn) => qn
            .feature
            .as_deref()
            .map(|f| format!("{}.{}", f, qn.name))
            .unwrap_or_else(|| qn.name.clone()),
        TypeRef::EnumRef(qn) => format!("enum {}", qn.name),
        TypeRef::Many(inner) => format!("Many<{}>", render_inner(inner)),
        TypeRef::Capability(_) => "@cap.*".to_owned(),
        TypeRef::Unresolved(name) => name.clone(),
    }
}

fn render_inner(inner: &TypeRef) -> String {
    match inner {
        TypeRef::Builtin(b) => format!("{:?}", b),
        TypeRef::UserDefined(qn) => qn.name.clone(),
        TypeRef::EnumRef(qn) => qn.name.clone(),
        TypeRef::Many(inner) => format!("Many<{}>", render_inner(inner)),
        TypeRef::Capability(_) => "@cap.*".to_owned(),
        TypeRef::Unresolved(name) => name.clone(),
    }
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, Field, FieldConstraints, Policies, Resource, TypeRef};

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "blog".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: vec![],
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: vec![],
            enums: vec![],
            resources,
            events: vec![],
            rules: vec![],
            policies: Policies::default(),
            errors: None,
            commands: vec![],
            apis: vec![],
            records: vec![],
            queries: vec![],
            resume_routers: vec![],
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
            reports: vec![],
            channels: vec![],
            caches: vec![],
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: vec![],
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
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
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            many_through: Vec::new(),
            append_only: false,
        }
    }

    fn mk_field(name: &str, type_ref: TypeRef, full_text: bool) -> Field {
        Field {
            name: name.to_owned(),
            type_ref,
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    #[test]
    fn positive_full_text_on_integer_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Article",
            vec![mk_field(
                "view_count",
                TypeRef::Builtin(BuiltinType::Integer),
                true,
            )],
        )]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "view_count");
        assert!(findings[0].message().contains("Integer"));
        assert_eq!(Finding::CODE, "FULL-TEXT-TYPE-001");
    }

    #[test]
    fn positive_full_text_on_boolean_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Article",
            vec![mk_field(
                "published",
                TypeRef::Builtin(BuiltinType::Boolean),
                true,
            )],
        )]);
        assert_eq!(check(&feature, Path::new("f.lzi")).len(), 1);
    }

    #[test]
    fn negative_full_text_on_text_does_not_fire() {
        let feature = mk_feature(vec![mk_resource(
            "Article",
            vec![mk_field("title", TypeRef::Builtin(BuiltinType::Text), true)],
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_full_text_on_semantic_email_does_not_fire() {
        let feature = mk_feature(vec![mk_resource(
            "Contact",
            vec![mk_field(
                "email",
                TypeRef::Builtin(BuiltinType::SemanticEmail),
                true,
            )],
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_no_full_text_flag_skipped() {
        let feature = mk_feature(vec![mk_resource(
            "Article",
            vec![mk_field(
                "view_count",
                TypeRef::Builtin(BuiltinType::Integer),
                false,
            )],
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
