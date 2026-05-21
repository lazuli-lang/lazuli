//! MONEY-COMPARE-001 — comparison expression spans two `Money` fields with
//! different declared currencies.
//!
//! Per `docs/proposals/semantic-types-money-brazilian.md` v0.3 + MONEY-1
//! §3.2 of the hostpoint roadmap. Carrying the ISO 4217 code on the
//! `SemanticMoney` IR variant lets us reject mixed-currency comparisons
//! at analyse time without re-walking surface text.
//!
//! Today's coverage: comparisons that surface as a `Predicate::Comparison`
//! inside a resource's `invariants` block (the only typed expression
//! site that exists at the time of MONEY-1 landing). General
//! command/rule predicates flow through the same `Predicate` shape, so
//! the resolver below transparently handles them once they're wired in.
//!
//! Severity: `error`. Authors who genuinely need cross-currency
//! comparison must materialise an explicit conversion (proposal §A2;
//! pending bucket).
//!
//! TODO(MONEY-EXPR-001): once `creates …`/`updates …` and rule bodies
//! grow general expression analysis, extend the resolver to those sites.
//! The shape of this lint stays the same — only the set of predicate
//! roots grows.

use std::path::{Path as FsPath, PathBuf};

use lazuli_ir::{
    BuiltinType, CurrencyCode, Expr, Feature, Path, Predicate, Resource, TypeRef,
};

/// One MONEY-COMPARE-001 finding: a `Predicate::Comparison` whose two
/// sides resolve to Money fields with disagreeing currencies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub resource: String,
    /// `<resource>.<field>` for the LHS so the message reads naturally.
    pub left: String,
    /// `<resource>.<field>` for the RHS.
    pub right: String,
    pub left_currency: CurrencyCode,
    pub right_currency: CurrencyCode,
}

impl Finding {
    pub const CODE: &'static str = "MONEY-COMPARE-001";

    pub fn message(&self) -> String {
        format!(
            "Cannot compare `Money({})` with `Money({})` — different \
             currencies. Convert one side explicitly before comparing \
             `{}` and `{}`.",
            self.left_currency.as_iso(),
            self.right_currency.as_iso(),
            self.left,
            self.right,
        )
    }
}

/// Run MONEY-COMPARE-001 across one feature's resources.
pub fn check(feature: &Feature, path: &FsPath) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .flat_map(|resource| check_resource(feature, resource, path))
        .collect()
}

fn check_resource(feature: &Feature, resource: &Resource, path: &FsPath) -> Vec<Finding> {
    let mut out = Vec::new();
    for invariant in &resource.invariants {
        if let lazuli_ir::EvalPredicate::Closed(predicate) = &invariant.when {
            collect_predicate(feature, resource, predicate, path, &mut out);
        }
    }
    out
}

fn collect_predicate(
    feature: &Feature,
    resource: &Resource,
    predicate: &Predicate,
    path: &FsPath,
    out: &mut Vec<Finding>,
) {
    match predicate {
        Predicate::Comparison { left, op, right } => {
            // All six comparison operators carry the same mixed-currency
            // bug — there is no operator for which mixing makes sense.
            let _ = op;
            let lhs_currency = expr_money_currency(resource, left);
            let rhs_currency = expr_money_currency(resource, right);
            if let (Some((lhs_name, lhs_cur)), Some((rhs_name, rhs_cur))) =
                (lhs_currency, rhs_currency)
            {
                if lhs_cur != rhs_cur {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        resource: resource.name.clone(),
                        left: format!("{}.{}", resource.name, lhs_name),
                        right: format!("{}.{}", resource.name, rhs_name),
                        left_currency: lhs_cur,
                        right_currency: rhs_cur,
                    });
                }
            }
        }
        Predicate::And(parts) | Predicate::Or(parts) => {
            for inner in parts {
                collect_predicate(feature, resource, inner, path, out);
            }
        }
        Predicate::Has { .. } => {}
    }
}

/// Resolve an `Expr::Path(<resource-local field>)` to its declared
/// currency. Returns `None` for anything other than a Money-typed field
/// on the same resource — non-Money paths and literals contribute
/// nothing to a mixed-currency error.
fn expr_money_currency(resource: &Resource, expr: &Expr) -> Option<(String, CurrencyCode)> {
    let path = match expr {
        Expr::Path(p) => p,
        _ => return None,
    };
    let field_name = leaf_field_name(path)?;
    let field = resource.fields.iter().find(|f| f.name == field_name)?;
    match field.type_ref {
        TypeRef::Builtin(BuiltinType::SemanticMoney { currency }) => {
            Some((field_name, currency))
        }
        _ => None,
    }
}

/// `total` and `self.total` should both resolve. Predicate paths from
/// invariants today use a bare `<field>` head; future predicate
/// extensions tag a `self` prefix the same way command bodies do, so
/// we accept both.
fn leaf_field_name(path: &Path) -> Option<String> {
    let segments = &path.segments;
    if segments.is_empty() {
        return None;
    }
    let last = segments.last().unwrap();
    if segments.len() == 1
        || segments.first().map(String::as_str) == Some("self")
            && segments.len() == 2
    {
        Some(last.clone())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        CompareOp, Defaults, EvalPredicate, Field, FieldConstraints, Invariant, Policies,
    };

    fn money_field(name: &str, currency: CurrencyCode) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::SemanticMoney { currency }),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            span_ref: None,
        }
    }

    fn comparison_invariant(name: &str, lhs: &str, op: CompareOp, rhs: &str) -> Invariant {
        Invariant {
            name: name.to_owned(),
            when: EvalPredicate::Closed(Predicate::Comparison {
                left: Expr::Path(Path::from_segments([lhs.to_owned()])),
                op,
                right: Expr::Path(Path::from_segments([rhs.to_owned()])),
            }),
            message: String::new(),
            span_ref: None,
        }
    }

    fn mk_resource(name: &str, fields: Vec<Field>, invariants: Vec<Invariant>) -> Resource {
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
            invariants,
            lock: None,
            composite_key: None,
            conventions: Vec::new(),
        }
    }

    fn mk_feature(resources: Vec<Resource>) -> Feature {
        Feature {
            name: "payments".into(),
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
            span_ref: None,
        }
    }

    #[test]
    fn same_currency_comparison_is_silent() {
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("total", CurrencyCode::BRL),
                money_field("tax", CurrencyCode::BRL),
            ],
            vec![comparison_invariant("tax_below_total", "tax", CompareOp::Le, "total")],
        )]);
        let findings = check(&feature, FsPath::new("features/payments/payments.lzi"));
        assert!(findings.is_empty(), "got: {:?}", findings);
    }

    #[test]
    fn cross_currency_comparison_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("total", CurrencyCode::BRL),
                money_field("paid", CurrencyCode::USD),
            ],
            vec![comparison_invariant("paid_matches_total", "paid", CompareOp::Eq, "total")],
        )]);
        let findings = check(&feature, FsPath::new("features/payments/payments.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "MONEY-COMPARE-001");
        let f = &findings[0];
        assert_eq!(f.left_currency, CurrencyCode::USD);
        assert_eq!(f.right_currency, CurrencyCode::BRL);
        assert!(f.message().contains("Money(USD)"));
        assert!(f.message().contains("Money(BRL)"));
    }

    #[test]
    fn self_qualified_path_resolves() {
        let invariant = Invariant {
            name: "paid_matches_total".into(),
            when: EvalPredicate::Closed(Predicate::Comparison {
                left: Expr::Path(Path::from_segments(["self", "paid"])),
                op: CompareOp::Eq,
                right: Expr::Path(Path::from_segments(["self", "total"])),
            }),
            message: String::new(),
            span_ref: None,
        };
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("total", CurrencyCode::BRL),
                money_field("paid", CurrencyCode::USD),
            ],
            vec![invariant],
        )]);
        let findings = check(&feature, FsPath::new("x.lzi"));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn comparison_against_non_money_is_silent() {
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("total", CurrencyCode::BRL),
                Field {
                    name: "label".into(),
                    type_ref: TypeRef::Builtin(BuiltinType::Text),
                    required: true,
                    unique: false,
                    slug: false,
                    default: None,
                    derived_from: None,
                    constraints: FieldConstraints::default(),
                    full_text: false,
                    previous_names: Vec::new(),
                    pii: None,
                    span_ref: None,
                },
            ],
            vec![comparison_invariant("trivia", "total", CompareOp::Eq, "label")],
        )]);
        assert!(check(&feature, FsPath::new("x.lzi")).is_empty());
    }

    #[test]
    fn and_or_recurse() {
        let invariant = Invariant {
            name: "compound".into(),
            when: EvalPredicate::Closed(Predicate::And(vec![Predicate::Comparison {
                left: Expr::Path(Path::from_segments(["paid"])),
                op: CompareOp::Ge,
                right: Expr::Path(Path::from_segments(["total"])),
            }])),
            message: String::new(),
            span_ref: None,
        };
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("total", CurrencyCode::BRL),
                money_field("paid", CurrencyCode::USD),
            ],
            vec![invariant],
        )]);
        let findings = check(&feature, FsPath::new("x.lzi"));
        assert_eq!(findings.len(), 1);
    }
}
