//! COMPOSITE-KEY-CONTRACT-001 — `composite_key` references undefined field.
//!
//! ## Rule statement
//!
//! Fires when a resource-level `composite_key` block lists a field name that
//! does not match any declared field on that resource. It also defensively
//! fires when the field list is empty, even though the parser already rejects
//! that surface form, so future IR construction sites cannot bypass the
//! contract.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles. This is a
//! correctness rule, not a vocabulary preference: the SQL emitter would
//! otherwise generate `PRIMARY KEY (<missing>)` or `UNIQUE (<missing>)`
//! clauses that fail at migration time because the referenced column does not
//! exist.
//!
//! ## Fixture example
//!
//! ```lzi
//! feature orders
//!   resource OrderLine
//!     order: Order required
//!     line_number: Integer required
//!     composite_key
//!       fields order, missing
//!       primary true
//! ```
//!
//! Canonical fix:
//!
//! ```lzi
//! feature orders
//!   resource OrderLine
//!     order: Order required
//!     line_number: Integer required
//!     composite_key
//!       fields order, line_number
//!       primary true
//! ```
//!
//! ## Proposal anchor
//!
//! No dedicated per-rule proposal was found in historical `docs/proposals/`.
//! The driving design is historical `docs/wave-c-deferred-integration.md`
//! §CL.C.2 plus `docs/roadmap.md` §1.5; the implementation landed in commit
//! `604637a` (`DB resource decorators — lock + composite_key + @full_text`).
//!
//! Diagnostic ID / code constant: `COMPOSITE-KEY-CONTRACT-001`;
//! `Finding::CODE` is `pub const CODE: &'static str =
//! "COMPOSITE-KEY-CONTRACT-001";`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Feature, Resource};

// ── output ───────────────────────────────────────────────────────────────────

/// One COMPOSITE-KEY-CONTRACT-001 finding — a resource's `composite_key`
/// block references a field that doesn't exist on the resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending resource lives in.
    pub path: PathBuf,
    /// Feature owning the resource.
    pub feature: String,
    /// Resource whose `composite_key` block triggered the check.
    pub resource: String,
    /// Unresolved field name listed in `composite_key.fields`.
    pub field: String,
    /// Why the rule fired — drives the diagnostic prose.
    pub reason: Reason,
}

/// Sub-classification of the composite-key contract violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// `fields` listed an identifier that doesn't match any
    /// declared `<name>: <Type>` on the resource.
    UnknownField,
    /// `fields` list is empty (defensive — parser rejects).
    EmptyFields,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "COMPOSITE-KEY-CONTRACT-001";

    /// Render the per-reason message — unknown field vs empty-fields
    /// list — each pointing at the canonical fix.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::composite_key_contract_001::{Finding, Reason};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "orders".into(),
    ///     resource: "OrderLine".into(),
    ///     field: "missing".into(),
    ///     reason: Reason::UnknownField,
    /// };
    /// assert!(f.message().contains("composite_key.fields"));
    /// ```
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

/// Run COMPOSITE-KEY-CONTRACT-001 across one feature's resources.
///
/// Walks every resource's `composite_key.fields` list and emits one
/// finding per unresolved name (plus a defensive `EmptyFields` if the
/// list is empty). No I/O — `path` is anchor metadata.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::correctness::composite_key_contract_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with composite_key");
/// let _ = check(&feature, Path::new("orders.lzi"));
/// ```
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
            lifecycle_routes: None,
            polymorphic_refs: Vec::new(),
            append_only: false,
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
            computed_date: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: vec![],
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
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
