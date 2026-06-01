//! VOCAB-MONEY-MULTI-CURRENCY-001 — multi-Money resource without explicit
//! per-field currency overrides.
//!
//! Per `docs/proposals/semantic-types-money-brazilian.md` v0.4, a resource
//! with two or more `Money` fields gets a single shared `currency` column
//! at codegen time. That's the right default for the common case (one
//! transaction, one currency), but resources that genuinely store
//! different currencies per Money field (cross-border charges,
//! multi-currency wallets, FX settlements) silently lose precision.
//!
//! This lint fires on every resource carrying 2+ Money fields where the
//! author has NOT declared explicit `<money>_currency: Currency` opt-out
//! fields. Authors then either:
//!   1. Accept the default with `# doctor:allow VOCAB-MONEY-MULTI-CURRENCY-001`
//!      on the resource (most common — one transaction, one currency).
//!   2. Opt out by declaring `<field>_currency: Currency` next to each
//!      diverging Money field.
//!
//! Severity: `warning` (strict and production profile).
//! Reference: docs/proposals/semantic-types-money-brazilian.md §A1
//!            docs/next-checklist.md §A1 entry under Money v0.3/v0.4.

use std::path::{Path, PathBuf};

use lazuli_ir::{BuiltinType, Feature, Resource, TypeRef};

/// One VOCAB-MONEY-MULTI-CURRENCY-001 finding: a resource with two or
/// more Money fields and no explicit per-field currency overrides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file the offending resource lives in.
    pub path: PathBuf,
    /// Feature owning the resource.
    pub feature: String,
    /// Resource declaring multiple Money fields with no per-field
    /// currency override.
    pub resource: String,
    /// Number of Money-typed fields on the resource.
    pub money_field_count: usize,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "VOCAB-MONEY-MULTI-CURRENCY-001";

    /// Render the "shared currency column" message asking the author
    /// to declare per-field `<field>_currency: Currency` or annotate
    /// the intent with `# doctor:allow`.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::vocab::vocab_money_multi_currency_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     feature: "billing".into(),
    ///     resource: "Order".into(),
    ///     money_field_count: 3,
    /// };
    /// assert!(f.message().contains("shared `currency` column"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "resource `{}` declares {} Money fields with no explicit \
             `<field>_currency: Currency` overrides — codegen will emit a \
             single shared `currency` column. If that's intended, add \
             `# doctor:allow VOCAB-MONEY-MULTI-CURRENCY-001` on the \
             resource. If different Money fields can hold different \
             currencies, declare `<field>_currency: Currency` next to \
             each diverging Money field.",
            self.resource, self.money_field_count
        )
    }
}

/// Run VOCAB-MONEY-MULTI-CURRENCY-001 across one feature's resources.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::vocab::vocab_money_multi_currency_001::check;
/// use lazuli_ir::Feature;
///
/// let feature: Feature = unimplemented!("lower a feature with Money fields");
/// let _ = check(&feature, Path::new("billing.lzi"));
/// ```
pub fn check(feature: &Feature, path: &Path) -> Vec<Finding> {
    feature
        .resources
        .iter()
        .filter_map(|resource| check_resource(feature, resource, path))
        .collect()
}

fn check_resource(feature: &Feature, resource: &Resource, path: &Path) -> Option<Finding> {
    let money_field_count = resource.fields.iter().filter(|f| is_money(f)).count();
    if money_field_count < 2 {
        return None;
    }

    // Any explicit `<field>_currency: Currency` field means the author has
    // already engaged with the multi-currency story; consider this resource
    // intentionally configured and skip the lint.
    let has_any_explicit_override = resource.fields.iter().any(|f| {
        is_currency(f)
            && f.name.strip_suffix("_currency").is_some_and(|stem| {
                resource
                    .fields
                    .iter()
                    .any(|other| other.name == stem && is_money(other))
            })
    });
    if has_any_explicit_override {
        return None;
    }

    Some(Finding {
        path: path.to_path_buf(),
        feature: feature.name.clone(),
        resource: resource.name.clone(),
        money_field_count,
    })
}

fn is_money(field: &lazuli_ir::Field) -> bool {
    matches!(
        field.type_ref,
        TypeRef::Builtin(BuiltinType::SemanticMoney { .. })
    )
}

fn is_currency(field: &lazuli_ir::Field) -> bool {
    matches!(
        field.type_ref,
        TypeRef::Builtin(BuiltinType::SemanticCurrency)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_ir::{Defaults, Field, FieldConstraints, Policies};

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

    fn money(name: &str) -> Field {
        mk_field(
            name,
            TypeRef::Builtin(BuiltinType::SemanticMoney {
                currency: lazuli_ir::CurrencyCode::BRL,
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
            name: "payments".into(),
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
    fn single_money_field_does_not_fire() {
        let feature = mk_feature(vec![mk_resource("Charge", vec![money("amount")])]);
        assert!(check(&feature, Path::new("features/payments/payments.lzi")).is_empty());
    }

    #[test]
    fn two_money_fields_no_overrides_fires() {
        let feature = mk_feature(vec![mk_resource(
            "Charge",
            vec![money("amount"), money("platform_fee")],
        )]);
        let findings = check(&feature, Path::new("features/payments/payments.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Charge");
        assert_eq!(findings[0].money_field_count, 2);
        assert_eq!(Finding::CODE, "VOCAB-MONEY-MULTI-CURRENCY-001");
        assert!(findings[0].message().contains("Charge"));
        assert!(findings[0].message().contains("2 Money fields"));
        assert!(findings[0].message().contains("doctor:allow"));
    }

    #[test]
    fn three_money_fields_no_overrides_fires_once() {
        let feature = mk_feature(vec![mk_resource(
            "Charge",
            vec![money("amount"), money("platform_fee"), money("net_to_host")],
        )]);
        let findings = check(&feature, Path::new("features/payments/payments.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].money_field_count, 3);
    }

    #[test]
    fn explicit_currency_overrides_silence_lint() {
        let feature = mk_feature(vec![mk_resource(
            "CrossBorderCharge",
            vec![
                money("amount"),
                currency("amount_currency"),
                money("settlement_amount"),
                currency("settlement_amount_currency"),
            ],
        )]);
        assert!(check(&feature, Path::new("features/payments/payments.lzi")).is_empty());
    }

    #[test]
    fn one_override_silences_the_resource_lint() {
        // Author has declared intent on at least one Money field by
        // attaching an explicit currency partner; the lint defers to
        // the author for the rest.
        let feature = mk_feature(vec![mk_resource(
            "MixedCharge",
            vec![money("amount"), currency("amount_currency"), money("fee")],
        )]);
        assert!(check(&feature, Path::new("features/payments/payments.lzi")).is_empty());
    }

    #[test]
    fn lone_currency_field_does_not_count_as_override() {
        // A bare `currency: Currency` field that does NOT pair with a
        // `<currency>_currency` Money field still leaves the multi-Money
        // resource without explicit per-field overrides — lint fires.
        let feature = mk_feature(vec![mk_resource(
            "Charge",
            vec![money("amount"), money("platform_fee"), currency("currency")],
        )]);
        let findings = check(&feature, Path::new("features/payments/payments.lzi"));
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn multiple_resources_lint_per_resource() {
        let feature = mk_feature(vec![
            mk_resource("Charge", vec![money("amount"), money("fee")]),
            mk_resource("Refund", vec![money("amount"), money("fee"), money("net")]),
        ]);
        let findings = check(&feature, Path::new("features/payments/payments.lzi"));
        assert_eq!(findings.len(), 2);
    }
}
