//! VOCAB-MONEY-SHAPE-001 — money modelled as something other than the
//! first-class `Money` type.
//!
//! `Money` already carries `(amount, currency, scale)` as one typed unit
//! (`BuiltinType::SemanticMoney { currency }`) and codegen auto-emits the
//! enforced `<field>_currency` sibling — so a field typed `Money` can never
//! lose its currency. The drift this rule kills is everything authored the
//! THREE hand-rolled ways instead, which the canonical pilot still does
//! inside one app:
//!
//!   * **(a) string-tagged `@semantic.Money` with no `<field>_currency`
//!     sibling** — `amount: @semantic.Money` whose currency the author had
//!     to remember to add as a separate string field. When the sibling is
//!     absent the currency silently vanishes. (The string-tagged *form*
//!     resolves to `SemanticMoney` in IR, so this case fires when a
//!     `SemanticMoney` field has no matching `<field>_currency` Currency
//!     sibling AND no resource-wide single-currency story.)
//!   * **(b) `<x>_cents: Integer` + `<x>_currency: Text` pair** — storage
//!     representation leaked into the surface (`price_amount_cents` +
//!     `price_currency`, hostpoint `catalog.lzi:205-206`, 7 pairs). The
//!     agent gets no type to reason about; scale is reconstructed from a
//!     `_cents` naming hack.
//!   * **(c) bare `Decimal` named like money** (`amount`, `price`,
//!     `*_price`, `*_amount`, `total`, `*_cents`) with NO currency sibling —
//!     pauta `hoxo_financial_integration.lzi:51` (`amount: Decimal`),
//!     `media_price_tables.lzi:89,106`. Currency is implied-by-comment,
//!     never typed.
//!
//! Each finding names `Money` as the fix and points at
//! `docs/lazuli_way/money.md`.
//!
//! ## Severity
//!
//! `Warning` — the hand-rolled shape still compiles; the rule names the
//! first-class replacement (mirrors `VOCAB-CRUD-SYNTH-AVAILABLE-001` /
//! `SUGGEST-REFERENTIAL-GUARD-001`). Suppressible per-resource with
//! `# doctor:allow VOCAB-MONEY-SHAPE-001`.
//!
//! ## Trigger cue / fixture
//!
//! Fires when a resource carries a `_cents:Integer` + `_currency:Text`
//! pair, a bare money-named `Decimal`, or a `Money` (string-tagged) field
//! with no currency sibling. Silent on a field typed `Money` (currency
//! travels in the type). See `cents_currency_pair_fires`,
//! `bare_decimal_money_fires`, `lone_cents_fires`,
//! `string_tagged_missing_sibling_fires`, and `first_class_money_is_silent`.

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, Field, Resource, TypeRef};

/// Which of the three drift encodings a finding flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftShape {
    /// `<x>_cents: Integer` (+ optional `<x>_currency: Text`) — storage
    /// representation leaked into the surface.
    CentsInteger,
    /// Bare `Decimal` named like money with no currency sibling.
    BareDecimal,
    /// String-tagged `@semantic.Money` with no `<field>_currency` sibling.
    StringTaggedNoSibling,
}

impl DriftShape {
    fn describe(self, field: &str) -> String {
        match self {
            DriftShape::CentsInteger => format!(
                "field `{field}` is an Integer minor-units amount \
                 (`_cents` + `_currency` pair) — the storage shape leaked \
                 into the surface"
            ),
            DriftShape::BareDecimal => format!(
                "field `{field}` is a bare `Decimal` named like money with \
                 no typed currency — the currency rests on a comment"
            ),
            DriftShape::StringTaggedNoSibling => format!(
                "field `{field}` is a string-tagged money amount with no \
                 `{field}_currency` sibling — the currency can silently vanish"
            ),
        }
    }
}

/// One VOCAB-MONEY-SHAPE-001 finding: a money amount modelled as one of
/// the three hand-rolled encodings instead of the first-class `Money` type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending resource lives in.
    pub path: PathBuf,
    /// Feature owning the resource.
    pub feature: String,
    /// Resource carrying the offending field.
    pub resource: String,
    /// The amount field that should be typed `Money`.
    pub field: String,
    /// Which drift shape fired.
    pub shape: DriftShape,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-MONEY-SHAPE-001";

    /// Render the "reach for `Money`" message naming the field, the drift
    /// shape, and the idiom doc.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::money_field_shape_001::{Finding, DriftShape};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("catalog.lzi"),
    ///     feature: "catalog".into(),
    ///     resource: "Service".into(),
    ///     field: "price_amount".into(),
    ///     shape: DriftShape::CentsInteger,
    /// };
    /// assert!(f.message().contains("Money"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "resource `{}` models money the hand-rolled way — {}. Reach for \
             the first-class `Money` type (amount + currency + scale as one \
             typed unit; the `_currency` sibling is enforced by codegen). \
             See docs/lazuli_way/money.md.",
            self.resource,
            self.shape.describe(&self.field),
        )
    }
}

/// Run VOCAB-MONEY-SHAPE-001 across one feature's resources.
///
/// Returns one finding per hand-rolled money amount. A field typed `Money`
/// (`SemanticMoney`) is the idiom and contributes nothing.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::money_field_shape_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a money-bearing feature");
/// let _ = check(&feature, Path::new("catalog.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .flat_map(|resource| check_resource(feature, resource, path))
        .collect()
}

fn check_resource(feature: &Feature, resource: &Resource, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for field in &resource.fields {
        let Some(shape) = drift_shape(resource, field) else {
            continue;
        };
        out.push(Finding {
            path: path.to_path_buf(),
            feature: feature.name.clone(),
            resource: resource.name.clone(),
            field: amount_stem(&field.name).to_owned(),
            shape,
        });
    }
    out
}

/// Classify a single field as one of the three drift shapes, or `None`
/// if it is not a hand-rolled money amount.
fn drift_shape(resource: &Resource, field: &Field) -> Option<DriftShape> {
    match &field.type_ref {
        // (b) `<x>_cents: Integer` — the minor-units storage hack. The
        // paired `<x>_currency: Text` (when present) is reported as part
        // of the same finding via the `_cents` field, so we anchor on the
        // amount, not the currency partner.
        TypeRef::Builtin(BuiltinType::Integer) if field.name.ends_with("_cents") => {
            Some(DriftShape::CentsInteger)
        }
        // (c) bare `Decimal` named like money with no currency sibling.
        TypeRef::Builtin(BuiltinType::Decimal) if is_money_named(&field.name) => {
            if has_currency_sibling(resource, &field.name) {
                None
            } else {
                Some(DriftShape::BareDecimal)
            }
        }
        // (a) string-tagged `@semantic.Money` whose currency sibling is
        // absent. `Money` carries its currency in the type, but a resource
        // can still hand-roll a *separate* `_currency` field; when that
        // separate sibling is missing the author relied on the type's
        // default and the currency is invisible at the field. We only fire
        // when the resource has NO currency story at all for this field
        // (no `<field>_currency` Currency sibling).
        TypeRef::Builtin(BuiltinType::SemanticMoney { .. }) => {
            if has_currency_sibling(resource, &field.name) {
                None
            } else {
                Some(DriftShape::StringTaggedNoSibling)
            }
        }
        _ => None,
    }
}

/// `price_amount_cents` → `price_amount`; otherwise unchanged. Used so the
/// finding names the conceptual money field, not the storage column.
fn amount_stem(name: &str) -> &str {
    name.strip_suffix("_cents").unwrap_or(name)
}

/// True when the resource declares a `<stem>_currency` field (any of
/// `Currency`/`Text`) for the given money amount field — i.e. the author
/// already paired a currency by hand.
fn has_currency_sibling(resource: &Resource, amount_field: &str) -> bool {
    let stem = amount_stem(amount_field);
    let wanted = format!("{stem}_currency");
    resource
        .fields
        .iter()
        .any(|f| f.name == wanted && is_currency_carrier(f))
}

/// A field that can carry a currency code — the typed `Currency` semantic
/// or the hand-rolled `Text` form (`price_currency: Text`).
fn is_currency_carrier(field: &Field) -> bool {
    matches!(
        field.type_ref,
        TypeRef::Builtin(BuiltinType::SemanticCurrency) | TypeRef::Builtin(BuiltinType::Text)
    )
}

/// Conservative money-naming heuristic for the bare-`Decimal` case:
/// `amount`, `price`, `total`, `subtotal`, `balance`, `fee`, `cost`,
/// `revenue`, or any name carrying `_price`/`_amount`/`_total`/`_fee`.
/// Geo / rating / ratio decimals (`latitude`, `avg_rating`) are
/// deliberately NOT matched — only the framework's money-shaped names fire.
fn is_money_named(name: &str) -> bool {
    const EXACT: &[&str] = &[
        "amount", "price", "total", "subtotal", "balance", "fee", "cost", "revenue",
    ];
    const SUFFIX: &[&str] = &["_price", "_amount", "_total", "_fee", "_cost", "_subtotal"];
    const PREFIX: &[&str] = &["price_", "amount_", "total_"];
    EXACT.contains(&name)
        || SUFFIX.iter().any(|s| name.ends_with(s))
        || PREFIX.iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{CurrencyCode, Defaults, FieldConstraints, Policies};

    fn mk_field(name: &str, type_ref: TypeRef) -> Field {
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
            full_text: false,
            previous_names: Vec::new(),
            pii: None,
            owner_axis: None,
            cross_feature_target: None,
            span_ref: None,
        }
    }

    fn integer(name: &str) -> Field {
        mk_field(name, TypeRef::Builtin(BuiltinType::Integer))
    }
    fn text(name: &str) -> Field {
        mk_field(name, TypeRef::Builtin(BuiltinType::Text))
    }
    fn decimal(name: &str) -> Field {
        mk_field(name, TypeRef::Builtin(BuiltinType::Decimal))
    }
    fn money(name: &str) -> Field {
        mk_field(
            name,
            TypeRef::Builtin(BuiltinType::SemanticMoney {
                currency: CurrencyCode::BRL,
            }),
        )
    }
    fn currency(name: &str) -> Field {
        mk_field(name, TypeRef::Builtin(BuiltinType::SemanticCurrency))
    }

    fn mk_resource(name: &str, fields: Vec<Field>) -> Resource {
        Resource {
            name: name.to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            soft_delete_actor: false,
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
    fn cents_currency_pair_fires() {
        // hostpoint catalog.lzi:205-206 shape.
        let feature = mk_feature(vec![mk_resource(
            "Service",
            vec![integer("price_amount_cents"), text("price_currency")],
        )]);
        let findings = check(&feature, Path::new("features/catalog/catalog.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shape, DriftShape::CentsInteger);
        assert_eq!(findings[0].field, "price_amount");
        assert_eq!(Finding::CODE, "VOCAB-MONEY-SHAPE-001");
        assert!(findings[0].message().contains("Money"));
    }

    #[test]
    fn lone_cents_fires() {
        // `min_service_price_cents` / `revenue_cents` — no currency at all.
        let feature = mk_feature(vec![mk_resource(
            "Snapshot",
            vec![integer("revenue_cents")],
        )]);
        let findings = check(&feature, Path::new("features/operations/operations.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shape, DriftShape::CentsInteger);
        assert_eq!(findings[0].field, "revenue");
    }

    #[test]
    fn bare_decimal_money_fires() {
        // pauta hoxo_financial_integration.lzi:51 `amount: Decimal`.
        let feature = mk_feature(vec![mk_resource("Entry", vec![decimal("amount")])]);
        let findings = check(&feature, Path::new("features/hoxo/hoxo.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shape, DriftShape::BareDecimal);
    }

    #[test]
    fn bare_decimal_with_currency_sibling_is_silent() {
        let feature = mk_feature(vec![mk_resource(
            "Entry",
            vec![decimal("amount"), currency("amount_currency")],
        )]);
        assert!(check(&feature, Path::new("x.lzi")).is_empty());
    }

    #[test]
    fn geo_and_rating_decimals_are_silent() {
        // latitude/longitude/avg_rating must NOT be mistaken for money.
        let feature = mk_feature(vec![mk_resource(
            "Place",
            vec![
                decimal("latitude"),
                decimal("longitude"),
                decimal("avg_rating"),
            ],
        )]);
        assert!(check(&feature, Path::new("x.lzi")).is_empty());
    }

    #[test]
    fn string_tagged_missing_sibling_fires() {
        // The case VOCAB-MONEY-MULTI-CURRENCY-001 used to own: a money
        // amount with no `<field>_currency` sibling. Now reported here.
        let feature = mk_feature(vec![mk_resource("Charge", vec![money("amount")])]);
        let findings = check(&feature, Path::new("features/payments/payments.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].shape, DriftShape::StringTaggedNoSibling);
    }

    #[test]
    fn string_tagged_with_currency_sibling_is_silent() {
        // payments.lzi:61,70 — `amount` + `amount_currency: Currency`.
        let feature = mk_feature(vec![mk_resource(
            "Charge",
            vec![money("amount"), currency("amount_currency")],
        )]);
        assert!(check(&feature, Path::new("x.lzi")).is_empty());
    }

    #[test]
    fn first_class_money_is_silent() {
        // The idiom: a `Money` field whose currency sibling is supplied.
        // Multiple money fields each paired stays silent.
        let feature = mk_feature(vec![mk_resource(
            "Charge",
            vec![
                money("amount"),
                currency("amount_currency"),
                money("platform_fee"),
                currency("platform_fee_currency"),
            ],
        )]);
        assert!(check(&feature, Path::new("x.lzi")).is_empty());
    }
}
