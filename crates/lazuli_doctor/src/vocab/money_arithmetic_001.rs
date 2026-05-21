//! MONEY-ARITHMETIC-001 — arithmetic on `Money` values requires a
//! same-currency operand or an explicit cast.
//!
//! Per MONEY-1 §3.2 of the hostpoint roadmap (close to proposal
//! `semantic-types-money-brazilian.md` v0.3). Two surface arithmetic
//! patterns are unambiguous bugs:
//!
//!   * `Money(X) + Decimal` (or any non-Money) — adds bare numbers to a
//!     currency-tagged amount; almost always means the author wanted
//!     a same-currency Money on the other side.
//!   * `Money(X) * Money(Y)` — multiplying two Money values is never
//!     valid, even when X == Y; the dimensional analysis breaks
//!     (currency² has no semantic).
//!
//! Today's coverage: surface-syntax declaration-time check over
//! `Field.derived_from` strings on Money-typed fields. The current IR
//! does not carry general arithmetic expressions; `derived_from` is a
//! short SQL-like text fragment that round-trips into `GENERATED ALWAYS
//! AS …` at codegen time. That's enough to catch the canonical bug:
//! `total: Money(BRL) derived_from "subtotal + tax"` where `subtotal`
//! is `Money(USD)`.
//!
//! TODO(MONEY-EXPR-001): once command/rule bodies grow typed arithmetic
//! `Expr` nodes (today they only model `Path`/literal comparisons), lift
//! this scan into the structured walker that MONEY-COMPARE-001 uses.
//! Until then, derived_from is the one declaration-time site general
//! enough to need this guard.

use std::path::{Path as FsPath, PathBuf};

use lazuli_ir::{BuiltinType, CurrencyCode, Feature, Field, Resource, TypeRef};

/// One MONEY-ARITHMETIC-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub feature: String,
    pub resource: String,
    /// The field whose `derived_from` carries the bad expression.
    pub field: String,
    pub field_currency: CurrencyCode,
    /// `"Money({other_currency})"` or `"Decimal"` etc. — the offending
    /// operand surface, rendered for the diagnostic message.
    pub operand: String,
    /// One of `+`, `-`, `*`, `/` — the operator detected in the
    /// `derived_from` text.
    pub op: char,
}

impl Finding {
    pub const CODE: &'static str = "MONEY-ARITHMETIC-001";

    pub fn message(&self) -> String {
        format!(
            "Arithmetic on `Money({})` requires same-currency operand or \
             explicit cast — field `{}` derives `… {} {}`.",
            self.field_currency.as_iso(),
            self.field,
            self.op,
            self.operand,
        )
    }
}

/// Run MONEY-ARITHMETIC-001 across one feature's resources.
pub fn check(feature: &Feature, path: &FsPath) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .flat_map(|resource| check_resource(feature, resource, path))
        .collect()
}

fn check_resource(feature: &Feature, resource: &Resource, path: &FsPath) -> Vec<Finding> {
    let mut out = Vec::new();
    for field in &resource.fields {
        let currency = match field.type_ref {
            TypeRef::Builtin(BuiltinType::SemanticMoney { currency }) => currency,
            _ => continue,
        };
        let Some(expr) = field.derived_from.as_deref() else {
            continue;
        };
        scan_derived(feature, resource, field, currency, expr, path, &mut out);
    }
    out
}

fn scan_derived(
    feature: &Feature,
    resource: &Resource,
    field: &Field,
    field_currency: CurrencyCode,
    expr: &str,
    path: &FsPath,
    out: &mut Vec<Finding>,
) {
    // Tokenise on the arithmetic operators we care about. We don't need
    // a full SQL parser — we only need the *operands flanking* each
    // operator, and a name on either side.
    let bytes = expr.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() {
        let ch = bytes[idx] as char;
        if matches!(ch, '+' | '-' | '*' | '/') {
            let lhs = ident_ending_at(expr, idx);
            let rhs = ident_starting_at(expr, idx + 1);
            // Multiplication of two Money fields is *always* a bug,
            // even at the same currency. The other operators only fire
            // when currencies mismatch.
            let lhs_currency =
                lhs.as_deref().and_then(|n| money_currency(resource, n));
            let rhs_currency =
                rhs.as_deref().and_then(|n| money_currency(resource, n));
            if ch == '*' {
                if let (Some(lc), Some(rc)) = (lhs_currency, rhs_currency) {
                    // Two Money operands — fire regardless of currency.
                    out.push(Finding {
                        path: path.to_path_buf(),
                        feature: feature.name.clone(),
                        resource: resource.name.clone(),
                        field: field.name.clone(),
                        field_currency,
                        operand: format!("Money({})", rc.as_iso()),
                        op: ch,
                    });
                    let _ = lc;
                }
            } else if matches!(ch, '+' | '-') {
                // Each operand must be a Money of the result's currency.
                // Anything else — different-currency Money, Decimal/Integer,
                // unresolved name — is the bug we exist to flag.
                for side in [&lhs, &rhs] {
                    let name = match side.as_deref() {
                        Some(n) => n,
                        None => continue,
                    };
                    let other = match resource.fields.iter().find(|f| f.name == name) {
                        Some(f) => f,
                        None => continue,
                    };
                    match other.type_ref {
                        TypeRef::Builtin(BuiltinType::SemanticMoney {
                            currency: other_currency,
                        }) if other_currency != field_currency => {
                            out.push(Finding {
                                path: path.to_path_buf(),
                                feature: feature.name.clone(),
                                resource: resource.name.clone(),
                                field: field.name.clone(),
                                field_currency,
                                operand: format!("Money({})", other_currency.as_iso()),
                                op: ch,
                            });
                        }
                        TypeRef::Builtin(BuiltinType::SemanticMoney { .. }) => {
                            // same-currency Money — by design OK on +/-.
                        }
                        ref ty if !is_money(ty) && !is_currency_typed(ty) => {
                            out.push(Finding {
                                path: path.to_path_buf(),
                                feature: feature.name.clone(),
                                resource: resource.name.clone(),
                                field: field.name.clone(),
                                field_currency,
                                operand: render_type(ty),
                                op: ch,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        idx += 1;
    }
}

fn ident_ending_at(expr: &str, idx: usize) -> Option<String> {
    let bytes = expr.as_bytes();
    let mut end = idx;
    while end > 0 && matches!(bytes[end - 1] as char, ' ' | '\t') {
        end -= 1;
    }
    if end == 0 {
        return None;
    }
    let mut start = end;
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    if start == end {
        return None;
    }
    Some(expr[start..end].to_owned())
}

fn ident_starting_at(expr: &str, idx: usize) -> Option<String> {
    let bytes = expr.as_bytes();
    let mut start = idx;
    while start < bytes.len() && matches!(bytes[start] as char, ' ' | '\t') {
        start += 1;
    }
    if start >= bytes.len() {
        return None;
    }
    let mut end = start;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some(expr[start..end].to_owned())
}

fn is_ident_byte(b: u8) -> bool {
    let c = b as char;
    c.is_ascii_alphanumeric() || c == '_'
}

fn money_currency(resource: &Resource, name: &str) -> Option<CurrencyCode> {
    let field = resource.fields.iter().find(|f| f.name == name)?;
    match field.type_ref {
        TypeRef::Builtin(BuiltinType::SemanticMoney { currency }) => Some(currency),
        _ => None,
    }
}

fn is_money(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Builtin(BuiltinType::SemanticMoney { .. }))
}

fn is_currency_typed(ty: &TypeRef) -> bool {
    matches!(ty, TypeRef::Builtin(BuiltinType::SemanticCurrency))
}

fn render_type(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Builtin(BuiltinType::Integer) => "Integer".to_owned(),
        TypeRef::Builtin(BuiltinType::Decimal) => "Decimal".to_owned(),
        TypeRef::Builtin(b) => format!("{:?}", b),
        _ => "non-Money".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{Defaults, Field, FieldConstraints, Policies};

    fn money_field(name: &str, currency: CurrencyCode, derived: Option<&str>) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::SemanticMoney { currency }),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: derived.map(str::to_owned),
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            span_ref: None,
        }
    }

    fn decimal_field(name: &str) -> Field {
        Field {
            name: name.to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Decimal),
            required: true,
            unique: false,
            slug: false,
            default: None,
            derived_from: None,
            constraints: FieldConstraints::default(),
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
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
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    #[test]
    fn same_currency_addition_is_silent() {
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("subtotal", CurrencyCode::BRL, None),
                money_field("tax", CurrencyCode::BRL, None),
                money_field("total", CurrencyCode::BRL, Some("subtotal + tax")),
            ],
        )]);
        let findings = check(&feature, FsPath::new("x.lzi"));
        assert!(findings.is_empty(), "got: {:?}", findings);
    }

    #[test]
    fn cross_currency_addition_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("subtotal", CurrencyCode::USD, None),
                money_field("tax", CurrencyCode::BRL, None),
                money_field("total", CurrencyCode::BRL, Some("subtotal + tax")),
            ],
        )]);
        let findings = check(&feature, FsPath::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(Finding::CODE, "MONEY-ARITHMETIC-001");
        let f = &findings[0];
        assert_eq!(f.op, '+');
        assert_eq!(f.field_currency, CurrencyCode::BRL);
        assert!(f.message().contains("Money(BRL)"));
    }

    #[test]
    fn money_plus_decimal_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("subtotal", CurrencyCode::BRL, None),
                decimal_field("fee"),
                money_field("total", CurrencyCode::BRL, Some("subtotal + fee")),
            ],
        )]);
        let findings = check(&feature, FsPath::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message().contains("Decimal"));
        assert_eq!(findings[0].op, '+');
    }

    #[test]
    fn money_times_money_always_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Order",
            vec![
                money_field("rate", CurrencyCode::BRL, None),
                money_field("hours", CurrencyCode::BRL, None),
                money_field("total", CurrencyCode::BRL, Some("rate * hours")),
            ],
        )]);
        let findings = check(&feature, FsPath::new("x.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].op, '*');
    }
}
