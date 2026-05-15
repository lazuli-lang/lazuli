//! SLUG-UNIQUENESS-IMPLICIT — a `@slug` field that did not also
//! declare `unique`. Slug columns are URL-addressable identifiers; a
//! non-unique slug breaks every downstream consumer (router, lookup
//! query, share link). Doctor surfaces this as a warning and the
//! codegen still emits a unique index — the IR's `Field.slug` is the
//! canonical signal — but we keep the lint to push authors toward
//! explicit `unique` so cold-readers see the contract.
//!
//! Severity: `warning` (strict and production). Promoting to `error`
//! is deferred to pilot evidence (we want to see whether any port
//! intentionally allows duplicate slugs scoped by tenant, in which
//! case the rule needs a tenant-aware variant).
//!
//! Reference: spec wave-c-cl4 (roadmap §1.7 — `@slug` decorator).

use std::path::{Path, PathBuf};

use lazuli_ir::Feature;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub resource: String,
    pub field: String,
}

impl Finding {
    pub const CODE: &'static str = "SLUG-UNIQUENESS-IMPLICIT";

    pub fn message(&self) -> String {
        format!(
            "field `{}` on resource `{}` is `@slug` but does not declare \
             `unique`. Slugs are URL-addressable; add `unique` to the \
             field modifiers so the contract is explicit and surfaces to \
             cold-readers.",
            self.field, self.resource
        )
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for resource in &feature.resources {
        for field in &resource.fields {
            if field.slug && !field.unique {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    field: field.name.clone(),
                });
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, Defaults, Feature, Field, FieldConstraints, Policies, Resource, TypeRef,
    };

    fn mk_field(name: &str, slug: bool, unique: bool) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Text),
            required: true,
            unique,
            slug,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
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

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "blog".into(),
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
            reports: vec![],
            channels: vec![],
            aggregates: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn positive_slug_without_unique_fires() {
        let feature = mk_feature(vec![mk_resource("Post", vec![mk_field("slug", true, false)])]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Post");
        assert_eq!(findings[0].field, "slug");
        assert_eq!(Finding::CODE, "SLUG-UNIQUENESS-IMPLICIT");
    }

    #[test]
    fn negative_slug_with_unique_does_not_fire() {
        let feature = mk_feature(vec![mk_resource("Post", vec![mk_field("slug", true, true)])]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_non_slug_field_does_not_fire() {
        let feature = mk_feature(vec![mk_resource("Post", vec![mk_field("name", false, false)])]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
