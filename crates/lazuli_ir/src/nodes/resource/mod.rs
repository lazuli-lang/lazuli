//! Resource IR — declarative persistence + record shape.
//!
//! A [`Resource`] is the lowered shape of one `resource <Name> { … }` block.
//! It declares the persistent rows that a feature owns: fields (typed,
//! optionally PII-tagged, optionally derived), constraints, tenancy axis,
//! soft-delete posture, retention policy, lifecycle state machine,
//! convention bundles, lock strategy, composite-key shape, and the
//! resource-level invariants (DDD aggregate boundary).
//!
//! ## Catalog
//!
//! - [`Resource`] — root.
//! - [`Record`] — typed-value record (no persistence axis); used by SQL
//!   queries, agent discriminated outputs.
//! - [`Field`] — one column declaration.
//! - [`FieldConstraints`] + [`SanitizeHtmlProfile`] — inline field
//!   constraints (`min`, `max`, `pattern`, `between`, `length`, `in`,
//!   `sanitize_html`, `utf8_safe`, `max_recursion`, `max_size`,
//!   `covers_pii`).
//! - [`OwnerAxis`] — `@owner_axis(through: <column>)` typed payload.
//! - [`OwnerScopeSql`] — analyzer-cached owner-scope SQL fragment.
//! - [`EnumDecl`] / [`EnumVariant`] / [`StorageValue`] — declarative enums.
//! - [`TypeRef`] / [`BuiltinType`] / [`CurrencyCode`] — the closed
//!   catalog of typed references the rest of the IR consumes.
//! - [`LifecycleRoutes`] / [`LifecycleRouteArm`] — state→URL table.
//! - [`ConventionRef`] / [`ConventionOrigin`] — closed-catalog
//!   convention bundle references + synth provenance.
//! - [`LockSpec`] — concurrency-control strategy.
//! - [`CompositeKey`] — `composite_key` block.
//! - [`RetentionSpec`] + [`RetentionAction`] — retention policy.

use serde::{Deserialize, Serialize};

use crate::nodes::aggregate::Invariant;
use crate::nodes::feature_defaults::{Constraint, FieldValidation, PathRef, Tenancy};
use crate::nodes::lifecycle::Lifecycle;
use crate::{PublicContract, SpanRef, is_false};

mod convention;
pub use convention::{ConventionOrigin, ConventionRef};

mod field;
pub use field::{Field, FieldConstraints, OwnerAxis, OwnerScopeSql, SanitizeHtmlProfile};

mod type_ref;
pub use type_ref::{BuiltinType, CurrencyCode, TypeRef};

/// CL.C.4 — `Resource` drops `Eq` because `Invariant` (transitively
/// carrying `EvalPredicate` ⇒ `Expr`) is not `Eq`. Mirrors the existing
/// `Feature` derive note. Consumers needing equality use `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    /// Tenancy axis: `tenancy org`, `tenancy team`, or `tenancy none` (opt-out).
    /// `None` means inherit from feature `defaults`. After lowering's derived
    /// pass this should be resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenancy: Option<Tenancy>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub soft_delete: bool,
    /// `None` means inherit from feature `defaults`. `Some(true)` = explicit
    /// `timestamps`, `Some(false)` = explicit `no_timestamps` opt-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<bool>,
    pub fields: Vec<Field>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    /// Resource-level inline validator: `validates resource "./domain/validate_row.go"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate: Option<PathRef>,
    /// Field-level inline validators: `validates field <field> "./hooks/validate_tier.go"`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validates: Vec<FieldValidation>,
    /// Phase L Tier 4c — `retention <duration> then <action>` policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention: Option<RetentionSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
    /// Lifecycle vocabulary (L0 #7 / lifecycle-vocab.md). When `Some`,
    /// the resource declares a state machine bound to a discriminator
    /// field; the lowering emits N commands + an auto-generated enum.
    /// `None` when the resource has no lifecycle (the vast majority).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
    /// CL.C.4 — resource-level `invariant <name>` declarations. Each
    /// invariant carries a closed-catalog predicate (`when <pred>`)
    /// plus an authored `message`. Lowered from the canonical-indent
    /// slice. Shared shape with `Aggregate.invariants` (DDD aggregate
    /// boundary). Additive: pre-CL.C.4 fixtures deserialize empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<Invariant>,
    /// Roadmap §1.5 (CL.C.2) — `lock` decorator. Declares a
    /// concurrency-control strategy for writes to this resource.
    /// `None` when no lock is declared (the framework default
    /// `id BIGSERIAL` row identity is used as-is, races resolved by
    /// last-write-wins at the SQL layer).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<LockSpec>,
    /// Roadmap §1.5 (CL.C.2) — `composite_key` block. When `Some` and
    /// `primary == true`, the migration emitter replaces the implicit
    /// `id BIGSERIAL PRIMARY KEY` with a `PRIMARY KEY (<fields>)`
    /// clause over the listed field columns. Doctor cross-checks each
    /// listed name against `Resource.fields`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite_key: Option<CompositeKey>,
    /// Resource-level conventions opt-in: `conventions [crud, ...]`.
    /// Each entry references a closed-catalog convention bundle that
    /// auto-synthesizes commands/queries during lowering. Empty when
    /// the resource opts into no conventions (the default). See
    /// `docs/proposals/ir-resource-conventions-crud.md` §4.2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conventions: Vec<ConventionRef>,
    /// router-w4 — `lifecycle_routes` block on a lifecycle-bearing
    /// resource. Declares a state→URL table that codegen lowers into
    /// a per-resource helper function (`<resource>LifecycleRoute(state)`).
    /// Route `requires_lifecycle X = state` / `on_lifecycle_pending
    /// dispatch_via X.lifecycle_route` consumes the helper to redirect
    /// when the actor's row is at a different lifecycle state.
    /// `None` for resources without a lifecycle routes table; doctor
    /// will eventually warn when a route declares `dispatch_via
    /// X.lifecycle_route` against a resource that doesn't have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_routes: Option<LifecycleRoutes>,
}

/// router-w4 — `lifecycle_routes` block on a Resource. Maps every
/// lifecycle state name (plus `none` for "no row" / `*` for the
/// catch-all wildcard) to a URL string. Codegen emits a per-resource
/// `<resource>LifecycleRoute(state: string | null): string` helper
/// that the routes.gen.tsx beforeLoad closures call when the route's
/// `requires_lifecycle` gate fails.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LifecycleRoutes {
    /// Ordered arms — state name → URL. Insertion order preserved
    /// for deterministic codegen output. The special keys `none`
    /// (missing row / 404) and `*` (wildcard fallback) are accepted
    /// alongside lifecycle state names.
    pub arms: Vec<LifecycleRouteArm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One arm inside a [`LifecycleRoutes`] block — maps a lifecycle state
/// (or the `none` / `*` sentinels) to the URL the route helper returns
/// for actors in that state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleRouteArm {
    /// Match key — lifecycle state name, `none`, or `*`.
    pub state: String,
    /// URL string the helper returns when the actor's row is in this
    /// lifecycle state.
    pub url: String,
}

/// Roadmap §1.5 (CL.C.2) — `lock` resource-level decorator. Closed
/// catalog of three concurrency-control strategies:
///
/// - `Optimistic { version_field }` — writes carry a `WHERE version = ?`
///   check; the version column is monotonic. Doctor enforces that
///   `version_field` names an existing `Integer` field on the resource.
/// - `Pessimistic` — runtime acquires an advisory or row-level lock
///   for the whole transaction (`SELECT ... FOR UPDATE` style).
/// - `RowLevel` — `FOR UPDATE` per-row at read time. Distinct from
///   `Pessimistic` (which is broader); rendered as a runtime hint
///   today.
///
/// DDL impact: `Optimistic` requires the version column to exist (an
/// authored field; doctor would have already verified the reference).
/// `Pessimistic` and `RowLevel` produce no DDL change — the runtime
/// applies `FOR UPDATE` at execution time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum LockSpec {
    Optimistic { version_field: String },
    Pessimistic,
    RowLevel,
}

/// Roadmap §1.5 (CL.C.2) — `composite_key` resource-level decorator.
/// Lists the fields participating in the key and whether the key is
/// the table's `PRIMARY KEY`. When `primary == false`, today this is
/// equivalent to a unique constraint declaration — kept as a separate
/// construct so future evolutions (e.g. clustered index, partitioning)
/// have a place to land without re-purposing `Constraint::Unique`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompositeKey {
    pub fields: Vec<String>,
    /// `primary true` — replace the implicit `id BIGSERIAL PRIMARY
    /// KEY` with `PRIMARY KEY (<fields>)`. When `false`, the implicit
    /// row identity is kept and the composite is emitted as a UNIQUE
    /// constraint.
    #[serde(default, skip_serializing_if = "is_false")]
    pub primary: bool,
}

/// Phase L Tier 4c — retention policy lifted from
/// `retention <duration> then <action>`. Duration stays verbatim
/// (adapter parses), action is a closed catalog so doctor can pin a
/// finite ruleset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionSpec {
    pub duration: String,
    pub action: RetentionAction,
}

/// Phase L Tier 4d — typed value record. Mirrors `Resource` minus the
/// tenancy / constraint / soft-delete machinery; used for SQL-query
/// return types, agent discriminated-record outputs, and other typed
/// projection bags. Distinct from `Resource` (no persistence axis) and
/// from `ContractRecord` (cross-feature contracts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    pub fields: Vec<Field>,
    /// `discriminator <field>` marker — `Some(field_name)` when the
    /// record carries a tagged-union discriminator (Cut A.6). Lowering
    /// finds the field on the surface; doctor cross-checks the enum
    /// type via `output discriminator <Record>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discriminator_field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Closed catalog of retention-expiry actions. `Anonymize` strips PII
/// in place; `Delete` removes the row; `Archive` moves it to cold
/// storage (adapter-defined).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionAction {
    /// Strip PII fields, keep the row.
    Anonymize,
    /// Remove the row entirely.
    Delete,
    /// Move to cold storage (adapter-defined shape).
    Archive,
}

/// `enum <Name> { … }` declaration. Closed list of named variants;
/// each variant may carry storage-value, label, and i18n metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    pub variants: Vec<EnumVariant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// One variant inside an [`EnumDecl`]. Carries the storage value the
/// runtime persists (or `None` to let codegen pick a default per target)
/// and optional UI metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    /// Authored storage value. `None` means the codegen picks per target;
    /// derived storage values do not enter the IR.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_value: Option<StorageValue>,
    /// Optional UI-facing metadata. Keys are opaque app-catalog strings;
    /// IR and codegen do not validate catalog/icon existence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
}

/// Closed catalog of authored enum storage values. `Integer` packs
/// the variant as a tinyint column; `String` keeps the variant name
/// as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum StorageValue {
    /// `as 1` — store as integer.
    Integer(i64),
    /// `as "active"` — store as the literal string.
    String(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convention_origin_distinguishes_author_override() {
        let s = ConventionOrigin::Synthesized(ConventionRef::Crud);
        let a = ConventionOrigin::AuthorOverride(ConventionRef::Me);
        assert_eq!(s.convention(), ConventionRef::Crud);
        assert!(!s.is_author_override());
        assert_eq!(a.convention(), ConventionRef::Me);
        assert!(a.is_author_override());
    }

    #[test]
    fn convention_origin_serialises_with_kind_and_convention() {
        let v = ConventionOrigin::Synthesized(ConventionRef::Crud);
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"kind\":\"synthesized\""));
        assert!(s.contains("\"convention\":\"crud\""));
        let back: ConventionOrigin = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn currency_code_round_trips_via_iso() {
        for code in [
            CurrencyCode::BRL,
            CurrencyCode::USD,
            CurrencyCode::EUR,
            CurrencyCode::GBP,
            CurrencyCode::JPY,
            CurrencyCode::CHF,
        ] {
            let iso = code.as_iso();
            assert_eq!(CurrencyCode::from_iso(iso), Some(code));
        }
        assert_eq!(CurrencyCode::from_iso("XYZ"), None);
    }

    #[test]
    fn field_constraints_default_is_empty() {
        let c = FieldConstraints::default();
        assert!(c.is_empty());
        let s = serde_json::to_string(&c).expect("serialize");
        assert_eq!(s, "{}");
    }

    #[test]
    fn lock_spec_optimistic_carries_version_field() {
        let v = LockSpec::Optimistic {
            version_field: "version".into(),
        };
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"kind\":\"Optimistic\""));
        assert!(s.contains("\"version_field\":\"version\""));
        let back: LockSpec = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn storage_value_tag_kind_value() {
        let v = StorageValue::Integer(7);
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{\"kind\":\"Integer\",\"value\":7}");
        let v = StorageValue::String("x".into());
        let s = serde_json::to_string(&v).expect("serialize");
        assert_eq!(s, "{\"kind\":\"String\",\"value\":\"x\"}");
    }
}
