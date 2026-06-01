//! CONSTRAINT-UNIQUE-WHEN-001 — a conditional `unique (...) when
//! <predicate>` constraint references a field the declaring resource
//! does not declare.
//!
//! GAP-NEW-001 partial-unique-index form. Fires when:
//!  - a unique-constraint column is not a declared field, OR
//!  - the `when <predicate>` closed-predicate `lhs`/`rhs` paths
//!    reference a field name not present on the resource.
//!
//! Severity: `error`. A partial index whose predicate (or column) can't
//! bind would either fail at migration time or silently index nothing —
//! Rule Zero (vocabulary, not silent no-op). Mirrors the field-reference
//! walk in `invariant_predicate_invalid` so the two checks stay aligned.
//!
//! Reference: GAP-NEW-001 (`docs/proposals/ir-pauta-gaps-bundle-2026-05-28.md`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{Constraint, EvalPredicate, Expr, Feature, Path as IrPath, Predicate};

/// One CONSTRAINT-UNIQUE-WHEN-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` path that hosts the resource.
    pub path: PathBuf,
    /// Feature containing the resource.
    pub feature: String,
    /// Resource whose conditional unique constraint is unresolved.
    pub resource: String,
    /// Bare field name (column or predicate path head) that did not
    /// resolve on the resource.
    pub unresolved_field: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "CONSTRAINT-UNIQUE-WHEN-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::domain::constraint_unique_when_invalid::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("catalog.lzi"),
    ///     feature: "catalog".into(),
    ///     resource: "PriceTable".into(),
    ///     unresolved_field: "ghost".into(),
    /// };
    /// assert!(f.message().contains("PriceTable"));
    /// assert!(f.message().contains("ghost"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "conditional `unique ... when` on resource `{}` references unknown \
             field `{}` (not declared on `{}`). Declare the field or fix the \
             constraint columns / predicate.",
            self.resource, self.unresolved_field, self.resource
        )
    }
}

/// Run CONSTRAINT-UNIQUE-WHEN-001 over one feature.
///
/// Only conditional uniques (`when` is `Some`) are checked — an
/// unconditional `unique (...)` over an unknown column is a separate
/// concern (the migration emitter would FK/column-mismatch). The `when`
/// predicate is walked with the same closed-predicate path collector as
/// `invariant_predicate_invalid`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::domain::constraint_unique_when_invalid::check;
///
/// let findings = check(&feature, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for resource in &feature.resources {
        let mut fields: HashSet<&str> =
            resource.fields.iter().map(|f| f.name.as_str()).collect();
        // A `soft_delete` resource synthesizes `deleted_at` (and, for the
        // `soft_delete by` actor form, `deleted_by`) columns that are NOT in
        // `resource.fields`. Credit them so the canonical soft-delete-aware
        // partial unique `unique (...) when deleted_at == nil` (which codegen
        // lowers to `WHERE deleted_at IS NULL`) resolves. Derived from the IR
        // SoT (`Resource::synthesized_soft_delete_columns`) so the doctor
        // agrees with codegen by construction — only the synthesized columns
        // are credited, never arbitrary unknowns.
        fields.extend(resource.synthesized_soft_delete_columns().iter().copied());
        for constraint in &resource.constraints {
            let Constraint::Unique(unique) = constraint else {
                continue;
            };
            // Only the conditional (partial-index) form is in scope.
            let Some(when) = &unique.when else {
                continue;
            };

            // 1. Constraint columns must resolve.
            for column in &unique.fields {
                if !fields.contains(column.as_str()) {
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        resource: resource.name.clone(),
                        unresolved_field: column.clone(),
                    });
                }
            }

            // 2. Predicate field-path heads must resolve.
            for missing in unresolved_top_fields(when, &fields) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    resource: resource.name.clone(),
                    unresolved_field: missing,
                });
            }
        }
    }

    findings
}

/// Collect bare top-level field names referenced by the predicate but not
/// in `known_fields`. Mirrors `invariant_predicate_invalid`. `Unparsed`
/// predicates surface a `<unparseable: ...>` sentinel so the author gets
/// a hint instead of a silent skip.
fn unresolved_top_fields(pred: &EvalPredicate, known_fields: &HashSet<&str>) -> Vec<String> {
    let mut out = Vec::new();
    match pred {
        EvalPredicate::Closed(inner) => collect_pred_paths(inner, known_fields, &mut out),
        EvalPredicate::Contains { lhs, .. } => check_path(lhs, known_fields, &mut out),
        EvalPredicate::ToolsCalls { .. } => {}
        EvalPredicate::Unparsed(text) => {
            out.push(format!("<unparseable: {}>", text.trim()));
        }
    }
    out
}

fn collect_pred_paths(pred: &Predicate, known_fields: &HashSet<&str>, out: &mut Vec<String>) {
    match pred {
        Predicate::Comparison { left, right, .. } => {
            check_expr(left, known_fields, out);
            check_expr(right, known_fields, out);
        }
        Predicate::Has {
            collection,
            element,
        } => {
            check_expr(collection, known_fields, out);
            check_expr(element, known_fields, out);
        }
        Predicate::And(parts) | Predicate::Or(parts) => {
            for part in parts {
                collect_pred_paths(part, known_fields, out);
            }
        }
    }
}

fn check_expr(expr: &Expr, known_fields: &HashSet<&str>, out: &mut Vec<String>) {
    if let Expr::Path(p) = expr {
        check_path(p, known_fields, out);
    }
}

fn check_path(p: &IrPath, known_fields: &HashSet<&str>, out: &mut Vec<String>) {
    let Some(head) = p.segments.first() else {
        return;
    };
    if known_fields.contains(head.as_str()) {
        return;
    }
    out.push(head.clone());
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        BuiltinType, CompareOp, Constraint, Defaults, EvalPredicate, Expr, Feature, Field,
        FieldConstraints, Path as IrPath, Policies, Predicate, Resource, TypeRef, UniqueConstraint,
    };

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
            type_ref: TypeRef::Builtin(BuiltinType::Boolean),
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

    fn cmp_pred(lhs: &str, rhs: bool) -> EvalPredicate {
        EvalPredicate::Closed(Predicate::Comparison {
            left: Expr::Path(IrPath::from_segments([lhs])),
            op: CompareOp::Eq,
            right: Expr::Boolean(rhs),
        })
    }

    fn mk_resource(name: &str, fields: Vec<Field>, constraints: Vec<Constraint>) -> Resource {
        Resource {
            name: name.into(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
            timestamps: None,
            fields,
            constraints,
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
            restrict_on_delete: Vec::new(),
            append_only: false,
        }
    }

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "catalog".into(),
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

    #[test]
    fn negative_known_field_predicate_passes() {
        let resource = mk_resource(
            "PriceTable",
            vec![mk_field("is_default")],
            vec![Constraint::Unique(UniqueConstraint {
                fields: vec!["is_default".into()],
                per: None,
                when: Some(cmp_pred("is_default", true)),
                error_code: None,
            })],
        );
        let feature = mk_feature(vec![resource]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn positive_unknown_predicate_field_fires() {
        let resource = mk_resource(
            "PriceTable",
            vec![mk_field("is_default")],
            vec![Constraint::Unique(UniqueConstraint {
                fields: vec!["is_default".into()],
                per: None,
                when: Some(cmp_pred("ghost", true)),
                error_code: None,
            })],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unresolved_field, "ghost");
        assert_eq!(Finding::CODE, "CONSTRAINT-UNIQUE-WHEN-001");
    }

    #[test]
    fn positive_unknown_constraint_column_fires() {
        let resource = mk_resource(
            "PriceTable",
            vec![mk_field("is_default")],
            vec![Constraint::Unique(UniqueConstraint {
                fields: vec!["nonexistent".into()],
                per: None,
                when: Some(cmp_pred("is_default", true)),
                error_code: None,
            })],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unresolved_field, "nonexistent");
    }

    fn cmp_pred_nil(lhs: &str) -> EvalPredicate {
        // `<lhs> == nil` — the canonical soft-delete-aware partial-unique
        // predicate. `nil` lowers to a Null literal expr; the field-head
        // membership check only inspects the path side.
        EvalPredicate::Closed(Predicate::Comparison {
            left: Expr::Path(IrPath::from_segments([lhs])),
            op: CompareOp::Eq,
            right: Expr::Nil,
        })
    }

    fn mk_soft_delete_resource(
        name: &str,
        actor: bool,
        fields: Vec<Field>,
        constraints: Vec<Constraint>,
    ) -> Resource {
        let mut r = mk_resource(name, fields, constraints);
        r.soft_delete = true;
        r.soft_delete_actor = actor;
        r
    }

    #[test]
    fn soft_delete_deleted_at_predicate_passes() {
        // Canonical soft-delete-aware partial unique. `deleted_at` is a
        // SYNTHESIZED column (not in `resource.fields`) — it must still
        // resolve because the resource is `soft_delete`.
        let resource = mk_soft_delete_resource(
            "PriceTable",
            false,
            vec![mk_field("sku"), mk_field("region")],
            vec![Constraint::Unique(UniqueConstraint {
                fields: vec!["sku".into(), "region".into()],
                per: None,
                when: Some(cmp_pred_nil("deleted_at")),
                error_code: None,
            })],
        );
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "soft-delete-synthesized `deleted_at` in the `when` predicate must resolve"
        );
    }

    #[test]
    fn soft_delete_by_credits_deleted_by_column() {
        // The actor form (`soft_delete by`) synthesizes `deleted_by` too.
        let resource = mk_soft_delete_resource(
            "PriceTable",
            true,
            vec![mk_field("sku")],
            vec![
                Constraint::Unique(UniqueConstraint {
                    fields: vec!["sku".into()],
                    per: None,
                    when: Some(cmp_pred_nil("deleted_at")),
                    error_code: None,
                }),
                Constraint::Unique(UniqueConstraint {
                    fields: vec!["deleted_by".into()],
                    per: None,
                    when: Some(cmp_pred_nil("deleted_by")),
                    error_code: None,
                }),
            ],
        );
        let feature = mk_feature(vec![resource]);
        assert!(
            check(&feature, Path::new("f.lzi")).is_empty(),
            "soft_delete_actor must credit both `deleted_at` and `deleted_by`"
        );
    }

    #[test]
    fn soft_delete_does_not_open_gate_for_arbitrary_unknown_field() {
        // Only the synthesized soft-delete columns get credited — a genuine
        // typo (`bogus`) on a soft-delete resource STILL errors.
        let resource = mk_soft_delete_resource(
            "PriceTable",
            false,
            vec![mk_field("sku")],
            vec![Constraint::Unique(UniqueConstraint {
                fields: vec!["sku".into()],
                per: None,
                when: Some(cmp_pred_nil("bogus")),
                error_code: None,
            })],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].unresolved_field, "bogus");
    }

    #[test]
    fn non_soft_delete_resource_does_not_credit_deleted_at() {
        // A NON-soft-delete resource has no synthesized `deleted_at`, so
        // referencing it is still an error (don't over-open the gate).
        let resource = mk_resource(
            "PriceTable",
            vec![mk_field("sku")],
            vec![Constraint::Unique(UniqueConstraint {
                fields: vec!["sku".into()],
                per: None,
                when: Some(cmp_pred_nil("deleted_at")),
                error_code: None,
            })],
        );
        let feature = mk_feature(vec![resource]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].unresolved_field, "deleted_at");
    }

    #[test]
    fn unconditional_unique_is_ignored() {
        // No `when` → not in scope for this rule even with a bad column.
        let resource = mk_resource(
            "PriceTable",
            vec![mk_field("is_default")],
            vec![Constraint::Unique(UniqueConstraint {
                fields: vec!["ghost".into()],
                per: None,
                when: None,
                error_code: None,
            })],
        );
        let feature = mk_feature(vec![resource]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
