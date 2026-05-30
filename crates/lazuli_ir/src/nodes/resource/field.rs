//! `Field` + its constraint payload + owner-axis annotations.
//!
//! A `Field` is one column declaration on a `Resource` (or `Record`).
//! It carries:
//!
//! - The structural axes (name, type, required, unique, default).
//! - Resource-shaped decorators (`@slug`, `@full_text`, `@pii`).
//! - Convention-relevant decorators (`@owner_axis(through:)` +
//!   the analyser-cached `OwnerScopeSql`).
//! - Inline validation constraints (`min`, `max`, `pattern`,
//!   `between`, `length`, `in`, `sanitize_html`, `utf8_safe`,
//!   `max_recursion`, `max_size`, `covers_pii`).
//!
//! `FieldConstraints` is a passive container — combination rules
//! and default-value compatibility are checked at lowering by
//! `lazuli_analyzer`. `SanitizeHtmlProfile` is the closed catalog
//! of HTML sanitiser presets the runtime ships.
//!
//! Owner-axis types (`OwnerAxis`, `OwnerScopeSql`) are colocated
//! because the field annotation is what triggers the analyser to
//! synth them; codegen reads both together.

use serde::{Deserialize, Serialize};

use super::type_ref::TypeRef;
use crate::nodes::capability::PiiCapability;
use crate::{DefaultValue, SpanRef, is_false};

/// One named column on a [`super::Resource`]. Carries the resolved
/// type, persistence flags (`required`, `unique`, `slug`,
/// `full_text`), default value, inline constraints, PII tag, and
/// owner-axis hop.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub type_ref: TypeRef,
    pub required: bool,
    pub unique: bool,
    /// CL.C.4 — `@slug` field decorator. When `true` the field is the
    /// resource's URL slug column; codegen emits a unique index +
    /// case-insensitive lookup. Doctor enforces implicit uniqueness
    /// via `slug-uniqueness-implicit`. Additive boolean: pre-CL.C.4
    /// fields deserialize with `slug == false`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub slug: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultValue>,
    /// Phase L Tier 4c — `<name>: <Type> derived from <expr>` lifts
    /// the computed-field expression. The analyzer keeps the verbatim
    /// expression text since `Expr` doesn't yet model comparison
    /// operators outside the predicate sublanguage; doctor reads the
    /// text for cross-field resolution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<String>,
    /// W3 GAP-03 — `<name>: Date computed_date from <base> offset <offset>`
    /// structured computed-date field kind. `Some` when the field's value
    /// is a base Date field plus an integer day offset. Distinct from
    /// `derived_from` (free-text SQL expression): `computed_date` carries a
    /// typed `{ base, offset }` shape so doctor (`COMPUTED-DATE-EXPR-001`)
    /// can type-check the base/offset references and codegen can emit the
    /// `time.Time.AddDate(0, 0, offset)` helper. Materialized at write time
    /// (stored `DATE` column in DDL; omitted from the Go struct exactly as
    /// `derived_from` columns are). Additive: pre-W3 fields deserialize
    /// with `None`. W4's `schedule_rule` (GAP-08) extends this node by
    /// adding a rule-selected base via `@fn`; see [`ComputedDate`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed_date: Option<ComputedDate>,
    /// L0 #3 §10 — inline field constraints emitted to Zod, Go
    /// validator tags, and (in a follow-up) OpenAPI. Six closed
    /// catalog keywords (`min N`, `max N`, `pattern STRING`,
    /// `between A and B`, `length N`, `in [...]`). Combination
    /// rules + default-value compatibility are enforced at lowering
    /// (see `lazuli_analyzer::AnalyzeError::ConstraintConflict` and
    /// `::DefaultViolatesConstraint`).
    #[serde(default, skip_serializing_if = "FieldConstraints::is_empty")]
    pub constraints: FieldConstraints,
    /// Roadmap §1.5 (CL.C.2) — `@full_text` field decorator. When
    /// `true`, the migration emitter adds a Postgres GIN index over
    /// the `to_tsvector('english', <field>)` projection so the runtime
    /// can do `tsvector` full-text search. Doctor enforces that the
    /// field's type is text-like (Text or `@semantic.*` string).
    #[serde(default, skip_serializing_if = "is_false")]
    pub full_text: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    /// FR-PII-STACK — orthogonal PII annotation. When set, the
    /// observability redactor masks this field's values in log
    /// output AND audit data_subject inference may consume it.
    /// Distinct from `type_ref` being CapabilityRef::PII — that
    /// path is for fields that are ONLY PII (no semantic carrier).
    /// This slot lets `@semantic.BrazilianCPF` + `@cap.PII` stack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pii: Option<PiiCapability>,
    /// `ir-resource-conventions-owner-scope` §7.2 — `@owner_axis(through:
    /// <column>)` field annotation. Marks this FK field as the
    /// ownership-chain hop: the `conventions [crud]` / `[me]` synth
    /// passes (O2) restrict every emitted command to rows where
    /// `<field>.<through_column>` equals `ctx.User.ID`. Absent =
    /// tenant-only scope (today's default). Additive; pre-O1 IR
    /// snapshots deserialize with `owner_axis == None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_axis: Option<OwnerAxis>,
    /// GAP-12 — `target @feature.<feature>.<Resource>` cross-feature FK
    /// annotation on an `ID` field. `Some` when the field logically
    /// references a resource owned by another feature. The reference is
    /// *logical*: the migration emitter records the column + a btree index
    /// but does NOT emit a hard DB FOREIGN KEY (the target table belongs
    /// to a separately-ordered migration set). Doctor
    /// `REF-CROSS-FEATURE-UNKNOWN-001` enforces that the named feature is
    /// in the declaring feature's `uses` and that the resource exists.
    /// Additive: pre-GAP-12 fields deserialize with `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_feature_target: Option<CrossFeatureTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// GAP-12 — typed payload for the `target @feature.<feature>.<Resource>`
/// cross-feature FK annotation. Names the owning feature + the resource
/// the `ID` field references. The reference is logical; codegen emits an
/// index, not a hard FK, because the target lives in another migration
/// set. Mirrors `lazuli_syntax::CrossFeatureTargetAst`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CrossFeatureTarget {
    /// Owning feature name (segment after `@feature.`).
    pub feature: String,
    /// Target resource name (PascalCase).
    pub resource: String,
}

/// W3 GAP-03 — typed payload for the `computed_date from <base> offset
/// <offset>` field kind. The field's `Date` value is `base + (offset days)`.
///
/// Semantics: `<this field> = <base> + (<offset> days)`. `base` names
/// another `Date` field on the same resource; `offset` resolves to a
/// number of days (an `Integer` field on the same resource, or an integer
/// literal). Doctor `COMPUTED-DATE-EXPR-001` enforces both reference rules.
///
/// Materialized at write time: codegen emits a `Compute<Field>` Go helper
/// (`base.AddDate(0, 0, offset)`) and a stored `DATE` column in DDL (the
/// column is omitted from the Go struct, mirroring `derived_from`).
///
/// W4 extension point (`schedule_rule`, GAP-08): the `base` selector is a
/// plain same-resource field name today. W4 will widen the base selection
/// to a closed rule enum + `@fn` hook by introducing a sibling enum (e.g.
/// `ComputedDateBase { Field(String), Rule { rule, fn_ref } }`) and
/// migrating `base: String` → `base: ComputedDateBase` while keeping
/// `offset` untouched. Pre-W4 IR snapshots deserialize the bare-string
/// base via a `#[serde(untagged)]`-or-`From<String>` shim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputedDate {
    /// Selector for the base `Date` value. Either a same-resource `Date`
    /// field name ([`ComputedDateBase::Field`]) or — W4 GAP-08
    /// `schedule_rule` — a rule-enum value resolved through a bound `@fn`
    /// ([`ComputedDateBase::Rule`]). The computed value is
    /// `<base> + (<offset> days)`.
    pub base: ComputedDateBase,
    /// Day offset: a same-resource `Integer` field reference, or an
    /// integer literal.
    pub offset: ComputedDateOffset,
}

/// W4 GAP-08 — selector for the base `Date` of a [`ComputedDate`].
///
/// W3 shipped a plain same-resource `Date` field name as the base. W4's
/// `schedule_rule` widens the selection: instead of a fixed base field, the
/// base date is picked by a *rule enum* and computed by a bound `@fn`. The
/// `@fn` returns the base `Date` selected by the rule argument; the same
/// `AddDate(0, 0, offset)` arithmetic then applies.
///
/// Surface syntax of the two arms:
/// - `due_date: Date computed_date from campaign_start offset offset_days`
///   → [`ComputedDateBase::Field`] (W3 form).
/// - `due_date: Date schedule_rule from @fn.activity_date_rule(input.rule) offset offset_days`
///   → [`ComputedDateBase::Rule`] (W4 form).
///
/// ## Back-compat serde shim
///
/// Pre-W4 IR snapshots serialized `base` as a bare JSON string (the field
/// name). `#[serde(untagged)]` keeps that path alive: a bare string
/// deserializes into [`ComputedDateBase::Field`]; the `{ "rule": ...,
/// "fn_ref": ... }` object deserializes into [`ComputedDateBase::Rule`].
/// `Field` re-serializes to a bare string so snapshots stay byte-stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComputedDateBase {
    /// W3 form — name of a same-resource `Date` field. Serializes as a
    /// bare JSON string for byte-stable back-compat with pre-W4 snapshots.
    Field(String),
    /// W4 GAP-08 `schedule_rule` form — the base `Date` is produced by a
    /// bound `@fn` that selects it from the closed rule enum. `rule` is the
    /// `@fn` argument (a same-resource field reference, e.g. `input.rule`);
    /// `fn_ref` is the bare binding-fn name (after `@fn.`).
    Rule {
        /// The rule-enum argument passed to the `@fn` (verbatim, e.g.
        /// `input.rule`). Doctor `SCHEDULE-RULE-001` checks it references
        /// a declared field.
        rule: String,
        /// Bare binding-fn name (the `<name>` in `@fn.<name>`). Doctor
        /// `SCHEDULE-RULE-001` checks it is a declared `Function` extension.
        fn_ref: String,
    },
}

impl ComputedDateBase {
    /// The base `Date` field name when this base is a plain field
    /// selector ([`ComputedDateBase::Field`]); `None` for the rule form.
    /// Doctor's W3 base-type check reads this to resolve the same-resource
    /// `Date` field.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::ComputedDateBase;
    ///
    /// assert_eq!(
    ///     ComputedDateBase::Field("created_at".to_owned()).field(),
    ///     Some("created_at"),
    /// );
    /// ```
    pub fn field(&self) -> Option<&str> {
        match self {
            ComputedDateBase::Field(name) => Some(name.as_str()),
            ComputedDateBase::Rule { .. } => None,
        }
    }
}

impl From<String> for ComputedDateBase {
    fn from(name: String) -> Self {
        ComputedDateBase::Field(name)
    }
}

/// W3 GAP-03 — the offset operand of a [`ComputedDate`]. Either a
/// same-resource `Integer` field (referenced by name) or an inline
/// integer literal (number of days). Both resolve to a count of days.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComputedDateOffset {
    /// `offset <field>` — name of an `Integer` field on the same resource.
    Field(String),
    /// `offset <int>` — inline integer-literal day count.
    Literal(i64),
}

/// `ir-resource-conventions-owner-scope` §7.2 — typed payload for the
/// `@owner_axis(through: <column>)` field annotation. `through_column`
/// is the column on the FK target resource that holds the actor key
/// (`user` for the canonical pilot's Property → Host chain). Multi-hop chains
/// are deferred per §13; v0 captures one hop.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OwnerAxis {
    pub through_column: String,
}

/// `ir-resource-conventions-owner-scope.md` §7.3 + §8.5.A — analyzer
/// synth output for owner-scope mode. Carries the SQL fragment the
/// analyzer composes once at synth time so downstream codegen can
/// emit it verbatim. One field per shape produced by the unified
/// builder: `where_predicate` for DELETE/UPDATE/LOOKUP/LIST tails,
/// `cte_owner_check` for the CREATE-side CTE prefix per §8.5.A.
///
/// **RULE-VOCAB-03**: this is a passive metadata container — codegen
/// pastes the captured string into the lowered SQL. No runtime
/// branching is introduced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerScopeSql {
    /// Field name on the resource that bears `@owner_axis`. Stored for
    /// inspect annotations (O3) and to support multi-axis composition
    /// in future cycles. Example: `"host"` for the canonical pilot Property.
    pub field_name: String,
    /// FK target resource name (PascalCase). Codegen lowers this to
    /// the snake-cased table identifier. Example: `"Host"`.
    pub fk_target: String,
    /// The `through:` column on the FK target — typically `"user"`.
    pub through_column: String,
    /// Pre-composed predicate fragment used by DELETE / UPDATE /
    /// LOOKUP / LIST. Example:
    /// `host IN (SELECT id FROM "host" WHERE "user" = ctx.User.ID)`.
    pub where_predicate: String,
    /// Pre-composed CTE prefix for `create_<resource>` per §8.5.A.
    /// Example: `WITH owner_check AS (SELECT 1 FROM "host" WHERE id = $host AND "user" = ctx.User.ID)`.
    /// `None` when this slot is attached to a Lookup/List/Delete/Update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cte_owner_check: Option<String>,
}

/// L0 #3 §10 — inline field constraints. Each slot is `Option` so an
/// absent constraint serializes off via `is_empty`. Combination rules
/// (§10.2) and default-value compatibility (§10.3) are checked in
/// `lazuli_analyzer` at lowering; this struct is a passive container.
///
/// `r#in` carries values as strings; numeric-typed `in [...]` values
/// are parsed on the consumer side (Go emitter / Zod emitter). This
/// avoids splitting the field per-type and keeps the wire shape
/// stable across numeric / text variants.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FieldConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between: Option<(i64, i64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub r#in: Option<Vec<String>>,
    /// `validate sanitize_html(<profile>)` — runtime strips dangerous
    /// HTML before persist. `profile` is a closed catalog of named
    /// rule sets (`strict`, `basic`, `markdown_safe`). None means no
    /// sanitization is applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitize_html: Option<SanitizeHtmlProfile>,
    /// `validate utf8_safe` — reject control chars + invalid UTF-8
    /// sequences. Cheap guard against subtle injection vectors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utf8_safe: Option<bool>,
    /// `validate max_recursion:<n>` — for JSON/JSONB fields, cap
    /// nesting depth. Mitigates OOM via crafted deeply-nested input.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_recursion: Option<u32>,
    /// `validate max_size:<n>` — cap field byte-length at persist
    /// time (distinct from upload-stream cap on `@cap.File`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    /// `validator covers_pii` — declares the validator function
    /// covers a known PII shape. References the validator catalog
    /// entry by snake_case name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covers_pii: Option<String>,
}

/// Closed catalog of HTML sanitization profiles. Each profile picks a
/// different cut-off between "strip everything" and "allow rich text"
/// — codegen wires the matching adapter at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SanitizeHtmlProfile {
    /// Strip ALL tags + decode entities. Use for plain-text fields
    /// that briefly accept rich input from rich-text editors.
    Strict,
    /// Allow `<b>`, `<i>`, `<em>`, `<strong>`, `<a href>`, `<br>`,
    /// `<p>`. Strip script/style/iframe/object/embed.
    Basic,
    /// Add markdown-friendly: `<code>`, `<pre>`, `<blockquote>`,
    /// `<ul>`, `<ol>`, `<li>`, `<h1..h6>`. Still strips all
    /// script-bearing tags + on* attributes.
    MarkdownSafe,
}

impl FieldConstraints {
    /// `true` when no constraint is set. Used by serde to skip the
    /// whole struct from JSON output (keeps `Module` byte-for-byte
    /// stable for declarations without inline constraints).
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::FieldConstraints;
    ///
    /// assert!(FieldConstraints::default().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.min.is_none()
            && self.max.is_none()
            && self.pattern.is_none()
            && self.between.is_none()
            && self.length.is_none()
            && self.r#in.is_none()
            && self.sanitize_html.is_none()
            && self.utf8_safe.is_none()
            && self.max_recursion.is_none()
            && self.max_size.is_none()
            && self.covers_pii.is_none()
    }

    /// Convenience constructor used by tests and call sites that build
    /// the struct from scratch without serde.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::FieldConstraints;
    ///
    /// let c = FieldConstraints::new();
    /// assert!(c.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_constraints_new_is_empty() {
        assert!(FieldConstraints::new().is_empty());
    }

    // W3 GAP-03 — `ComputedDate` serde round-trip + shape.

    #[test]
    fn computed_date_field_offset_round_trips() {
        let cd = ComputedDate {
            base: ComputedDateBase::Field("campaign_start".into()),
            offset: ComputedDateOffset::Field("offset_days".into()),
        };
        let json = serde_json::to_string(&cd).unwrap();
        // The `Field` base serializes as a bare string (untagged), so it
        // stays byte-stable with pre-W4 snapshots.
        assert!(json.contains("\"base\":\"campaign_start\""));
        assert!(json.contains("field"));
        let back: ComputedDate = serde_json::from_str(&json).unwrap();
        assert_eq!(cd, back);
    }

    #[test]
    fn computed_date_literal_offset_round_trips() {
        let cd = ComputedDate {
            base: ComputedDateBase::Field("campaign_start".into()),
            offset: ComputedDateOffset::Literal(30),
        };
        let json = serde_json::to_string(&cd).unwrap();
        let back: ComputedDate = serde_json::from_str(&json).unwrap();
        assert_eq!(cd, back);
        assert!(matches!(back.offset, ComputedDateOffset::Literal(30)));
    }

    // W4 GAP-08 — `schedule_rule` base + back-compat serde shim.

    #[test]
    fn computed_date_pre_w4_bare_string_base_deserializes_as_field() {
        // A pre-W4 snapshot serialized `base` as a bare string; it must
        // still load (into the `Field` arm) without churn.
        let json = r#"{"base":"campaign_start","offset":{"field":"offset_days"}}"#;
        let back: ComputedDate = serde_json::from_str(json).unwrap();
        assert_eq!(back.base, ComputedDateBase::Field("campaign_start".into()));
        assert_eq!(back.base.field(), Some("campaign_start"));
    }

    #[test]
    fn computed_date_rule_base_round_trips() {
        let cd = ComputedDate {
            base: ComputedDateBase::Rule {
                rule: "input.rule".into(),
                fn_ref: "activity_date_rule".into(),
            },
            offset: ComputedDateOffset::Field("offset_days".into()),
        };
        let json = serde_json::to_string(&cd).unwrap();
        assert!(json.contains("activity_date_rule"));
        let back: ComputedDate = serde_json::from_str(&json).unwrap();
        assert_eq!(cd, back);
        assert_eq!(back.base.field(), None);
    }

    #[test]
    fn field_without_computed_date_omits_it_from_json() {
        // Additive: a field with `computed_date: None` must not emit the
        // key (skip_serializing_if) so pre-W3 snapshots stay byte-stable.
        use crate::nodes::resource::type_ref::{BuiltinType, TypeRef};
        let field = super::Field {
            name: "x".into(),
            type_ref: TypeRef::Builtin(BuiltinType::Date),
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
        };
        let json = serde_json::to_string(&field).unwrap();
        assert!(!json.contains("computed_date"), "got: {json}");
    }
}
