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

use crate::nodes::capability::PiiCapability;
use crate::{DefaultValue, SpanRef, is_false};

use super::type_ref::TypeRef;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
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
}
