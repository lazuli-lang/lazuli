//! VOCAB-UNION-002 — polymorphic FK (enum discriminator + untyped id).
//!
//! Fires on the `target: <Enum> + target_id: ID` shape where the FK is
//! untyped and gated by an enum tag. The discriminated-union vocabulary
//! expresses this with typed FKs per variant.
//!
//! Sibling of VOCAB-UNION-001 (which catches enum + correlated-optional-
//! fields). This rule catches the polymorphic-id-pair variant.
//!
//! Severity: `warning` (strict), `error` (production).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, EnumDecl, Feature, Field, Resource, TypeRef};

const DISCRIMINATOR_NAMES: &[&str] = &["target", "subject", "attachment_target", "parent_target"];

// ── output ───────────────────────────────────────────────────────────────────

/// One VOCAB-UNION-002 finding: an enum-discriminated untyped FK pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// Resource name.
    pub resource: String,
    /// Enum discriminator field, e.g. `target`.
    pub discriminator_field: String,
    /// Untyped FK sibling field, e.g. `target_id`.
    pub fk_field: String,
    /// Same-feature enum name used by the discriminator.
    pub enum_name: String,
    /// Same-feature enum variants.
    pub variants: Vec<String>,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-UNION-002";

    pub fn message(&self) -> String {
        let union_variants = self
            .variants
            .iter()
            .map(|variant| {
                format!(
                    "    On{variant}\n      {}: {variant} required\n      ...common fields...",
                    variant.to_lowercase()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let typed_resources = self
            .variants
            .iter()
            .map(|variant| {
                format!(
                    "  resource {variant}{}\n    {}: {variant} required\n    ...",
                    self.resource,
                    variant.to_lowercase()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "resource `{}` declares `{}` as enum `{}` ({}) plus untyped FK `{}`; \
             suggestion: split into a discriminated union OR sibling typed-FK resources.\n\
             \n\
             union {}\n\
{}\n\
             \n\
             Or, for the typed-FK-per-resource form (vocabulary already supports this):\n\
{}",
            self.resource,
            self.discriminator_field,
            self.enum_name,
            self.variants.join(", "),
            self.fk_field,
            self.resource,
            union_variants,
            typed_resources,
        )
    }
}

// ── detection ────────────────────────────────────────────────────────────────

/// Run VOCAB-UNION-002 over one feature's resources.
///
/// `path` is the source `.lzi` file; this rule performs no I/O.
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let variants = build_variant_map(&feature.enums);
    feature
        .resources
        .iter()
        .flat_map(|r| check_resource(r, &variants, path))
        .collect()
}

// ── internals ────────────────────────────────────────────────────────────────

/// `enum_name -> [variant_name, ...]` for same-feature enums.
fn build_variant_map(enums: &[EnumDecl]) -> HashMap<String, Vec<String>> {
    enums
        .iter()
        .map(|e| {
            let variants = e.variants.iter().map(|v| v.name.clone()).collect();
            (e.name.clone(), variants)
        })
        .collect()
}

fn check_resource(
    resource: &Resource,
    variants: &HashMap<String, Vec<String>>,
    path: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    for discriminator in resource
        .fields
        .iter()
        .filter(|f| DISCRIMINATOR_NAMES.contains(&f.name.as_str()))
    {
        let Some(enum_name) = enum_name_of(discriminator) else {
            continue;
        };

        let Some(enum_variants) = variants.get(enum_name) else {
            continue;
        };

        if enum_variants.len() < 2 {
            continue;
        }

        let fk_name = format!("{}_id", discriminator.name);
        let Some(fk_field) = resource.fields.iter().find(|f| f.name == fk_name) else {
            continue;
        };

        if !is_id_or_text(fk_field) {
            continue;
        }

        findings.push(Finding {
            path: path.to_path_buf(),
            resource: resource.name.clone(),
            discriminator_field: discriminator.name.clone(),
            fk_field: fk_field.name.clone(),
            enum_name: enum_name.to_string(),
            variants: enum_variants.clone(),
        });
    }

    findings
}

fn enum_name_of(field: &Field) -> Option<&str> {
    match &field.type_ref {
        TypeRef::EnumRef(qn) if qn.feature.is_none() => Some(qn.name.as_str()),
        _ => None,
    }
}

fn is_id_or_text(field: &Field) -> bool {
    matches!(
        &field.type_ref,
        TypeRef::Builtin(BuiltinType::Id | BuiltinType::Text)
    )
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, EnumVariant, Policies, QualifiedName};

    fn mk_field(name: &str, type_ref: TypeRef, required: bool) -> Field {
        Field {
            name: name.into(),
            type_ref,
            required,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: lazuli_ir::FieldConstraints::default(),
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
            lifecycle: None,
            invariants: vec![],
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

    fn cross_feature_enum_ref(feature: &str, name: &str) -> TypeRef {
        TypeRef::EnumRef(QualifiedName {
            feature: Some(feature.into()),
            name: name.into(),
        })
    }

    fn user_defined(name: &str) -> TypeRef {
        TypeRef::UserDefined(QualifiedName {
            feature: None,
            name: name.into(),
        })
    }

    fn id() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Id)
    }

    fn text() -> TypeRef {
        TypeRef::Builtin(BuiltinType::Text)
    }

    // ── positive ─────────────────────────────────────────────────────────────

    #[test]
    fn positive_target_enum_plus_target_id_fires() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field("target", enum_ref("CommentTarget"), true),
                mk_field("target_id", id(), true),
                mk_field("body", text(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Comment");
        assert_eq!(findings[0].discriminator_field, "target");
        assert_eq!(findings[0].fk_field, "target_id");
        assert_eq!(findings[0].enum_name, "CommentTarget");
        assert_eq!(
            findings[0].variants,
            vec!["Issue".to_string(), "Customer".to_string()]
        );
        assert_eq!(Finding::CODE, "VOCAB-UNION-002");
        assert!(findings[0].message().contains("discriminated union"));
    }

    #[test]
    fn positive_subject_enum_plus_subject_id_fires() {
        let resource = mk_resource(
            "Activity",
            vec![
                mk_field("subject", enum_ref("ActivitySubject"), true),
                mk_field("subject_id", id(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("ActivitySubject", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/activity/activity.lzi"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].discriminator_field, "subject");
        assert_eq!(findings[0].fk_field, "subject_id");
    }

    // ── negative ─────────────────────────────────────────────────────────────

    #[test]
    fn negative_only_one_variant_does_not_fire() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field("target", enum_ref("CommentTarget"), true),
                mk_field("target_id", id(), true),
            ],
        );
        let feature = mk_feature(vec![mk_enum("CommentTarget", &["Issue"])], vec![resource]);

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_typed_fk_does_not_fire() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field("target", enum_ref("CommentTarget"), true),
                mk_field("target_id", user_defined("Issue"), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_random_name_pair_does_not_fire() {
        let resource = mk_resource(
            "TaggedThing",
            vec![
                mk_field("tag", enum_ref("TagKind"), true),
                mk_field("tag_id", id(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("TagKind", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/tagged/tagged.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn negative_missing_paired_id_does_not_fire() {
        let resource = mk_resource(
            "Comment",
            vec![mk_field("target", enum_ref("CommentTarget"), true)],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert!(findings.is_empty());
    }

    #[test]
    fn cross_feature_enum_does_not_fire() {
        let resource = mk_resource(
            "Comment",
            vec![
                mk_field(
                    "target",
                    cross_feature_enum_ref("other", "CommentTarget"),
                    true,
                ),
                mk_field("target_id", id(), true),
            ],
        );
        let feature = mk_feature(
            vec![mk_enum("CommentTarget", &["Issue", "Customer"])],
            vec![resource],
        );

        let findings = check(&feature, Path::new("features/comment/comment.lzi"));

        assert!(findings.is_empty());
    }
}
