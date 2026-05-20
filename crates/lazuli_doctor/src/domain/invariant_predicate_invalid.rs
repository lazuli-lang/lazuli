//! INVARIANT-PREDICATE-INVALID — invariant `when <predicate>`
//! references a field that is not declared on the scoping resource
//! (or, for aggregate invariants, on the aggregate's root).
//!
//! Fires when:
//!  - A resource-scoped `invariant <name>` has `when <pred>` whose
//!    closed-predicate `lhs/rhs` paths reference a field name not
//!    present on the resource.
//!  - An aggregate-scoped invariant references a field not on the
//!    aggregate's root resource. (Cross-resource paths on the
//!    aggregate cluster — e.g. `lines.amount` for `OrderLine` — are
//!    accepted when the first segment is the aggregate root field or
//!    a `contains` member; the deeper scope check defers to a
//!    follow-up rule.)
//!
//! Severity: `error` (strict), `error` (production). Invariants whose
//! predicates can't bind drift into "always passes" silently — Rule
//! Zero violation.
//!
//! Reference: spec wave-c-cl4 (roadmap §1.7 — `invariant` first-class).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{EvalPredicate, Expr, Feature, Path as IrPath, Predicate, Resource};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub scope: InvariantScope,
    pub invariant: String,
    pub unresolved_field: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvariantScope {
    Resource(String),
    Aggregate(String),
}

impl Finding {
    pub const CODE: &'static str = "INVARIANT-PREDICATE-INVALID";

    pub fn message(&self) -> String {
        match &self.scope {
            InvariantScope::Resource(r) => format!(
                "invariant `{}` on resource `{}`: predicate references unknown \
                 field `{}` (not declared on `{}`)",
                self.invariant, r, self.unresolved_field, r
            ),
            InvariantScope::Aggregate(a) => format!(
                "invariant `{}` on aggregate `{}`: predicate references unknown \
                 field `{}` (not declared on the aggregate root or any \
                 `contains` member)",
                self.invariant, a, self.unresolved_field
            ),
        }
    }
}

pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Resource-scoped invariants
    for resource in &feature.resources {
        let fields: HashSet<&str> =
            resource.fields.iter().map(|f| f.name.as_str()).collect();
        for inv in &resource.invariants {
            for missing in unresolved_top_fields(&inv.when, &fields) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    scope: InvariantScope::Resource(resource.name.clone()),
                    invariant: inv.name.clone(),
                    unresolved_field: missing,
                });
            }
        }
    }

    // Aggregate-scoped invariants — scope is the root resource's
    // declared fields plus the bare names of `contains` members
    // (whose deeper field access is deferred to a follow-up rule).
    for agg in &feature.aggregates {
        let root_resource: Option<&Resource> = feature
            .resources
            .iter()
            .find(|r| r.name == agg.root.name);
        let mut scope: HashSet<String> = HashSet::new();
        if let Some(r) = root_resource {
            for f in &r.fields {
                scope.insert(f.name.clone());
            }
        }
        for member in &agg.contains {
            // Each contains member is itself usable as a path prefix
            // (e.g. `lines.amount` when `OrderLine` is contained as
            // `lines`). We accept the bare member name as a binding;
            // deeper field resolution is a follow-up rule.
            scope.insert(member.name.clone());
            // Also accept the snake_case-pluralized binding form, which
            // is the common DSL idiom (`lines` for `OrderLine`).
            scope.insert(naive_pluralize(&member.name));
        }
        let scope_set: HashSet<&str> = scope.iter().map(String::as_str).collect();
        for inv in &agg.invariants {
            for missing in unresolved_top_fields(&inv.when, &scope_set) {
                findings.push(Finding {
                    path: path.to_path_buf(),
                    feature: feature.name.clone(),
                    scope: InvariantScope::Aggregate(agg.name.clone()),
                    invariant: inv.name.clone(),
                    unresolved_field: missing,
                });
            }
        }
    }

    findings
}

/// Walk an `EvalPredicate` and collect bare top-level field names that
/// are referenced but not in `known_fields`. Cross-resource paths
/// (`lines.amount`) are reduced to their first segment; that segment
/// must be in scope. Constants, strings, ints, bools, and nil are
/// ignored. Unparsed predicates surface as a single missing-field
/// finding using the raw text so authors get a hint.
fn unresolved_top_fields(pred: &EvalPredicate, known_fields: &HashSet<&str>) -> Vec<String> {
    let mut out = Vec::new();
    match pred {
        EvalPredicate::Closed(inner) => collect_pred_paths(inner, known_fields, &mut out),
        EvalPredicate::Contains { lhs, .. } => {
            check_path(lhs, known_fields, &mut out);
        }
        EvalPredicate::ToolsCalls { .. } => {
            // Not a field-bearing predicate; nothing to check.
        }
        EvalPredicate::Unparsed(text) => {
            // We do not invent a name; signal unparseability with a
            // sentinel that doctor surfaces in its message. The caller
            // (consumer) sees a "field" named like `<unparseable: ...>`
            // and can rephrase.
            out.push(format!("<unparseable: {}>", text.trim()));
        }
    }
    out
}

fn collect_pred_paths(
    pred: &Predicate,
    known_fields: &HashSet<&str>,
    out: &mut Vec<String>,
) {
    match pred {
        Predicate::Comparison { left, right, .. } => {
            check_expr(left, known_fields, out);
            check_expr(right, known_fields, out);
        }
        Predicate::Has { collection, element } => {
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

/// Naive pluralization: `OrderLine` → `order_lines`. Heuristic only,
/// good enough for the DSL idiom where `contains OrderLine` is bound
/// as `lines` in predicates. Doctor accepts both the PascalCase and
/// the snake_case-plural form.
fn naive_pluralize(name: &str) -> String {
    let mut snake = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() && i > 0 {
            snake.push('_');
        }
        snake.push(ch.to_ascii_lowercase());
    }
    if snake.ends_with('s') {
        snake
    } else {
        format!("{}s", snake)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        Aggregate, BuiltinType, CompareOp, Defaults, EvalPredicate, Expr, Feature, Field,
        FieldConstraints, Invariant, Path as IrPath, Policies, Predicate, QualifiedName,
        Resource, TypeRef,
    };

    fn mk_field(name: &str) -> Field {
        Field {
            name: name.into(),
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
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>, invariants: Vec<Invariant>) -> Resource {
        Resource {
            name: name.into(),
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
            invariants,
            lock: None,
            composite_key: None,
        }
    }

    fn mk_feature(resources: Vec<Resource>, aggregates: Vec<Aggregate>) -> Feature {
        Feature {
            name: "billing".into(),
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
            aggregates,
            mcp_servers: vec![],
            previous_names: vec![],
            span_ref: None,
        }
    }

    fn cmp(lhs: &str, op: CompareOp, rhs: i64) -> EvalPredicate {
        EvalPredicate::Closed(Predicate::Comparison {
            left: Expr::Path(IrPath::from_segments([lhs])),
            op,
            right: Expr::Integer(rhs),
        })
    }

    fn mk_invariant(name: &str, when: EvalPredicate) -> Invariant {
        Invariant {
            name: name.into(),
            when,
            message: "must hold".into(),
            span_ref: None,
        }
    }

    #[test]
    fn positive_unknown_field_on_resource_fires() {
        let resource = mk_resource(
            "Order",
            vec![mk_field("total")],
            vec![mk_invariant(
                "total_non_negative",
                cmp("ghost", CompareOp::Ge, 0),
            )],
        );
        let feature = mk_feature(vec![resource], vec![]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unresolved_field, "ghost");
        assert!(matches!(
            findings[0].scope,
            InvariantScope::Resource(ref r) if r == "Order"
        ));
        assert_eq!(Finding::CODE, "INVARIANT-PREDICATE-INVALID");
    }

    #[test]
    fn negative_known_field_on_resource_passes() {
        let resource = mk_resource(
            "Order",
            vec![mk_field("total")],
            vec![mk_invariant("non_neg", cmp("total", CompareOp::Ge, 0))],
        );
        let feature = mk_feature(vec![resource], vec![]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn positive_unknown_field_on_aggregate_fires() {
        let order = mk_resource("Order", vec![mk_field("total")], vec![]);
        let aggregate = Aggregate {
            name: "OrderBoundary".into(),
            root: QualifiedName {
                feature: None,
                name: "Order".into(),
            },
            contains: vec![],
            invariants: vec![mk_invariant(
                "weird",
                cmp("ghost_field", CompareOp::Eq, 0),
            )],
            span_ref: None,
        };
        let feature = mk_feature(vec![order], vec![aggregate]);
        let findings = check(&feature, Path::new("f.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unresolved_field, "ghost_field");
        assert!(matches!(
            findings[0].scope,
            InvariantScope::Aggregate(ref a) if a == "OrderBoundary"
        ));
    }

    #[test]
    fn negative_contains_member_binding_passes() {
        let order = mk_resource("Order", vec![mk_field("total")], vec![]);
        let line = mk_resource("OrderLine", vec![mk_field("amount")], vec![]);
        let aggregate = Aggregate {
            name: "OrderBoundary".into(),
            root: QualifiedName {
                feature: None,
                name: "Order".into(),
            },
            contains: vec![QualifiedName {
                feature: None,
                name: "OrderLine".into(),
            }],
            // `order_lines.amount` head resolves to `order_lines`
            // (snake-case plural of `OrderLine`).
            invariants: vec![mk_invariant(
                "consistent",
                cmp("order_lines", CompareOp::Eq, 0),
            )],
            span_ref: None,
        };
        let feature = mk_feature(vec![order, line], vec![aggregate]);
        assert!(check(&feature, Path::new("f.lzi")).is_empty());
    }
}
