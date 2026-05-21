//! COMPOSITE-KEY-CONTRACT-001 — `composite_key` references undefined field.
//!
//! Doctor correctness rule. Fires when a `composite_key { fields ... }`
//! block lists a name that does not match any declared field on the
//! resource. Also fires when the field list is empty (the parser
//! already rejects this, but the doctor check guards against future
//! IR construction sites that bypass the parser).
//!
//! Severity: `error` — the SQL emitter would otherwise produce
//! `PRIMARY KEY (<missing>)` / `UNIQUE (<missing>)` clauses that fail
//! to apply at migration time with `relation "X" does not have column
//! "Y"`.
//!
//! Reference: docs/roadmap.md §1.5.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Resource};

// ── output ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub resource: String,
    /// Unresolved field name listed in `composite_key.fields`.
    pub field: String,
    pub reason: Reason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// `fields` listed an identifier that doesn't match any
    /// declared `<name>: <Type>` on the resource.
    UnknownField,
    /// `fields` list is empty (defensive — parser rejects).
    EmptyFields,
}

impl Finding {
    pub const CODE: &'static str = "COMPOSITE-KEY-CONTRACT-001";

    pub fn message(&self) -> String {
        match self.reason {
            Reason::UnknownField => format!(
                "resource `{}` declares `composite_key` listing field `{}`, but no field named `{}` exists on the resource. \
                 Either declare the field or remove it from `composite_key.fields`.",
                self.resource, self.field, self.field
            ),
            Reason::EmptyFields => format!(
                "resource `{}` declares a `composite_key` block with an empty `fields` list. \
                 List at least one field (e.g. `fields order, line_number`).",
                self.resource
            ),
        }
    }
}

// ── detection ────────────────────────────────────────────────────────────────

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for resource in &feature.resources {
        collect_findings_for_resource(feature, resource, path, &mut out);
    }
    out
}

fn collect_findings_for_resource(
    feature: &Feature,
    resource: &Resource,
    path: &Path,
    out: &mut Vec<Finding>,
) {
    let Some(ck) = resource.composite_key.as_ref() else {
        return;
    };
    if ck.fields.is_empty() {
        out.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            resource: resource.name.clone(),
            field: String::new(),
            reason: Reason::EmptyFields,
        });
        return;
    }
    let declared: HashSet<&str> = resource.fields.iter().map(|f| f.name.as_str()).collect();
    for field_name in &ck.fields {
        if !declared.contains(field_name.as_str()) {
            out.push(Finding {
                path: path.to_path_buf(),
                feature: feature.name.clone(),
                resource: resource.name.clone(),
                field: field_name.clone(),
                reason: Reason::UnknownField,
            });
        }
    }
}

// ── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, CompositeKey, Defaults, Field, FieldConstraints, Policies, Resource, TypeRef,
    };

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "orders".into(),
            purpose: None,
            non_goals: vec![],
            context_path: None,
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

    fn mk_resource(name: &str, fields: Vec<Field>, ck: Option<CompositeKey>) -> Resource {
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
            composite_key: ck,
            conventions: Vec::new(),
        }
    }

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Integer),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    #[test]
    fn positive_unknown_field_in_composite_key_fires() {
        let feature = mk_feature(vec![mk_resource(
            "OrderLine",
            vec![mk_field("order")],
            Some(CompositeKey {
                fields: vec!["order".into(), "missing".into()],
                primary: true,
            }),
        )]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].field, "missing");
        assert_eq!(findings[0].reason, Reason::UnknownField);
        assert_eq!(Finding::CODE, "COMPOSITE-KEY-CONTRACT-001");
    }

    #[test]
    fn positive_empty_fields_fires() {
        let feature = mk_feature(vec![mk_resource(
            "OrderLine",
            vec![mk_field("order")],
            Some(CompositeKey {
                fields: vec![],
                primary: true,
            }),
        )]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].reason, Reason::EmptyFields);
    }

    #[test]
    fn negative_resolved_fields_does_not_fire() {
        let feature = mk_feature(vec![mk_resource(
            "OrderLine",
            vec![mk_field("order"), mk_field("line_number")],
            Some(CompositeKey {
                fields: vec!["order".into(), "line_number".into()],
                primary: true,
            }),
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn negative_no_composite_key_skipped() {
        let feature = mk_feature(vec![mk_resource(
            "OrderLine",
            vec![mk_field("order")],
            None,
        )]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
