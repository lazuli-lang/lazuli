//! `resource <Name>` declaration AST (Phase L Tier 4c).
//!
//! Resources live inside `domain` at indent 4. Their children at indent
//! 6 cover the full record-shape contract: fields, `has_many`,
//! `previously`, `soft_delete`, `timestamps`, `retention`, `validates`,
//! `lifecycle`, `invariant`, `lock`, `composite_key`, `conventions`,
//! `lifecycle_routes`, `index`, `unique`.
//!
//! Tier 4c lifts the entire canonical-indent surface through
//! `lower_resource_decl`; the legacy brace MVP keeps emitting the same
//! `ir::Resource` so the two surfaces converge.
//!
//! Field constraints (`FieldConstraintsDecl`, L0 #3 §10) are captured
//! as a typed bag at parse time. Combination + default-compat rules
//! live in the analyzer; the parser only records what the author
//! actually wrote.
//!
//! `OwnerAxisAst` mirrors `ir::OwnerAxis` for the
//! `ir-resource-conventions-owner-scope` §7.1 `@owner_axis(through: ...)`
//! field annotation. The decorator is peeled out of the type text by
//! the parser so the analyzer can project directly into
//! `ir::Field.owner_axis`.

use serde::{Deserialize, Serialize};

use super::{DefaultsTenancy, PublicContractDeclAst, Span};

/// `resource <Name>` block — Phase L Tier 4c record-shape declaration.
///
/// Lives under `domain`. Children span the full record contract:
/// fields, `has_many`, `previously`, `soft_delete`, `timestamps`,
/// `retention`, `validates`, `lifecycle`, `invariant`, `lock`,
/// `composite_key`, `conventions`, `lifecycle_routes`, `index`,
/// `unique`. See module-level docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `previously migrated <old>` (one entry per `previously` line).
    pub previously: Vec<String>,
    /// `tenancy <axis>` resource-local override.
    pub tenancy: Option<DefaultsTenancy>,
    /// Field declarations (`<name>: <Type> [modifiers...]`).
    pub fields: Vec<ResourceFieldDecl>,
    /// `has_many <name>: <Resource> [inverse <field>]` lines.
    pub has_many: Vec<ResourceHasMany>,
    /// `soft_delete` declared verbatim.
    pub soft_delete: bool,
    /// `timestamps` declared verbatim.
    pub timestamps: bool,
    /// `retention <duration> then <action>` policy.
    pub retention: Option<ResourceRetention>,
    /// `validates @validator.<name>` and `validates resource "./..."`
    /// declarations. Captured as raw text; the analyzer dispatches
    /// between resource-level and field-level validators.
    pub validates: Vec<String>,
    /// Resource-owned state machine over one discriminator field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<crate::parser::LifecycleBlockAst>,
    /// CL.C.4 — standalone `invariant <name>` blocks declared as
    /// resource children. Each block carries a closed-catalog
    /// predicate (`when <expr>`) plus an authored `message`. Shared
    /// shape with `AggregateDecl.invariants`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<InvariantDecl>,
    /// Roadmap §1.5 (CL.C.2) — `lock` decorator. Closed catalog
    /// `optimistic`/`pessimistic`/`row_level`. At most one per resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<ResourceLock>,
    /// Roadmap §1.5 (CL.C.2) — `composite_key` block. Lists fields and
    /// an optional `primary true` flag indicating that the implicit
    /// `id BIGSERIAL PRIMARY KEY` should be replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composite_key: Option<ResourceCompositeKey>,
    /// `conventions [<name>, ...]` resource-level slot. Closed catalog
    /// of named convention bundles (today: `crud`). Empty when the
    /// resource opts into no conventions. See
    /// `docs/proposals/ir-resource-conventions-crud.md` §4.1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conventions: Vec<ResourceConventionAst>,
    /// router-w4 — `lifecycle_routes` block: a `<state> -> "<url>"`
    /// table on a lifecycle-bearing resource. Lowered to
    /// `ir::LifecycleRoutes`; TS codegen emits a per-resource
    /// `<resource>LifecycleRoute(state: string | null): string` helper
    /// that routes.gen.tsx beforeLoad closures call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_routes: Option<ResourceLifecycleRoutesAst>,
    /// Resource-authored DDL declarations:
    /// `index on <field>`, `index on (<field>, ...) [using <method>]`,
    /// `unique (<field>, ...)`, and `fts on (<field>, ...)`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<ResourceConstraintAst>,
    /// GAP-13 — `polymorphic_ref <type_field> <id_field> targets [A, B, ...]`
    /// declarations. Each declares a discriminator field + an `ID` field
    /// whose referent depends on the discriminator, over a closed target
    /// resource list. Lowered to `ir::PolymorphicRef`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub polymorphic_refs: Vec<ResourcePolymorphicRefAst>,
    /// GAP-AUDIT-02 — `append_only` resource modifier (bare line). When
    /// `true`, the resource is insert-only: doctor
    /// `RESOURCE-APPEND-ONLY-001` rejects `command`s that update/delete it.
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub append_only: bool,
    /// GAP-07 — `many_through <JunctionName> to <PartnerResource>` block
    /// declarations. Each declares an M:N relationship whose junction
    /// resource carries extra payload fields beyond the two endpoint FKs.
    /// The analyzer desugars each into a synthesized junction `ir::Resource`
    /// (`<declaring>_id`, `<partner>_id`, payload columns, composite unique
    /// on the pair). Lowered to `ir::ManyThrough`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub many_through: Vec<ManyThroughAst>,
    pub span: Span,
}

/// GAP-07 — one `many_through <JunctionName> to <PartnerResource>` block on
/// a [`ResourceDecl`]. Models an M:N relationship with metadata: the
/// junction resource named `name` carries an FK to the declaring resource,
/// an FK to `partner`, plus the `payload` fields authored under the block.
///
/// Surface (canonical-indent block form):
///
/// ```text
/// resource Job
///   many_through JobMember to User
///     role_in_job: Text
/// ```
///
/// The partner endpoint is **explicit and required** via the `to
/// <PartnerResource>` clause — the clearest closed form. Inference from the
/// junction name (e.g. stripping the declaring-resource prefix) is
/// unreliable (`JobMember` → `Member` ≠ `User`), so the partner is named
/// outright and doctor `MANY-THROUGH-ENDPOINT-001` verifies it resolves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManyThroughAst {
    /// Junction resource name (PascalCase), e.g. `JobMember`.
    pub name: String,
    /// Partner endpoint resource name (PascalCase) from `to <Resource>`,
    /// e.g. `User`.
    pub partner: String,
    /// Payload fields authored under the block — extra columns the
    /// junction carries beyond the two endpoint FKs. Reuses the shared
    /// resource field parser, so each supports the full field surface
    /// (`<name>: <Type> [modifiers]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payload: Vec<ResourceFieldDecl>,
    pub span: Span,
}

/// GAP-13 — one `polymorphic_ref <type_field> <id_field> targets
/// [A, B, C]` declaration on a [`ResourceDecl`]. Models a discriminated
/// foreign key: the `type_field` column holds the target resource name
/// (an enum over `targets`) and the `id_field` column holds the row id of
/// whichever target the discriminator names. No single DB FK can express
/// this (it would point at multiple tables), so codegen emits the two
/// columns + a CHECK over the target names + a composite index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourcePolymorphicRefAst {
    /// Discriminator field name (e.g. `entity_type`).
    pub type_field: String,
    /// FK id field name (e.g. `entity_id`).
    pub id_field: String,
    /// Closed list of target resource names, in source order.
    pub targets: Vec<String>,
    pub span: Span,
}

/// Closed two-arm catalog for resource-authored DDL constraints —
/// `index` / `unique`. `fts on (...)` lifts into `ResourceIndexAst`
/// with `full_text = true`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ResourceConstraintAst {
    /// `index on <field>` / `index on (<field>, ...) [using <method>]`.
    Index(ResourceIndexAst),
    /// `unique (<field>, ...)`.
    Unique(ResourceUniqueAst),
}

/// One `index` row on a [`ResourceDecl`]. Drives DDL emission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceIndexAst {
    /// Column list verbatim, in source order.
    pub fields: Vec<String>,
    /// `using <method>` — closed catalog (btree/gin/gist).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<ResourceIndexMethodAst>,
    /// `fts on (...)` shorthand sets this and implies `using gin`.
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub full_text: bool,
    pub span: Span,
}

/// One `unique (<field>, ...)` row on a [`ResourceDecl`].
///
/// GAP-NEW-001 — the optional `when <predicate>` clause turns the
/// constraint into a PARTIAL unique index. The predicate text is
/// captured verbatim (analyzer runs it through the closed-predicate
/// parser, mirroring `InvariantDecl.when`); `None` is the unconditional
/// `unique (...)` form that maps to a table-level UNIQUE constraint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceUniqueAst {
    /// Column list verbatim, in source order.
    pub fields: Vec<String>,
    /// `when <predicate>` — verbatim predicate text (analyzer parses).
    /// `Some` makes this a partial unique index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    pub span: Span,
}

/// Closed catalog of Postgres index methods authored via
/// `using <method>` on a [`ResourceIndexAst`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceIndexMethodAst {
    /// Default B-tree index (`using btree`).
    Btree,
    /// GIN index (`using gin`) — required for `fts on (...)`.
    Gin,
    /// GiST index (`using gist`).
    Gist,
}

/// Closed-catalog identifier inside a resource's `conventions [...]`
/// slot. Adding a variant is an IR/parser change requiring a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceConventionAst {
    Crud,
    Me,
}

/// Roadmap §1.5 (CL.C.2) — `lock` decorator closed catalog. Variant
/// data preserved so the analyzer can lift into `ir::LockSpec` and
/// doctor can cross-check the optimistic `version_field` against
/// `Resource.fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ResourceLock {
    /// `lock optimistic version <field>` — version-column compare-and-swap.
    Optimistic { version_field: String },
    /// `lock pessimistic` — explicit row lock per write.
    Pessimistic,
    /// `lock row_level` — per-row lock acquired during the transaction.
    RowLevel,
}

/// Roadmap §1.5 (CL.C.2) — `composite_key` block AST shape. The parser
/// walks the children of the `composite_key` line (`fields <list>`,
/// `primary true|false`) and produces this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceCompositeKey {
    pub fields: Vec<String>,
    #[serde(default)]
    pub primary: bool,
    pub span: Span,
}

/// router-w4 — `lifecycle_routes` block AST. Each arm pairs a
/// lifecycle state name (or `none` / `*`) with a literal URL string.
/// router-w4 — `lifecycle_routes` block container. Holds one arm per
/// `<state> -> "<url>"` line authored on the resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLifecycleRoutesAst {
    pub arms: Vec<ResourceLifecycleRouteArmAst>,
    pub span: Span,
}

/// One `<state> -> "<url>"` arm inside [`ResourceLifecycleRoutesAst`].
/// `state` accepts `none` and `*` (wildcard) per the proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLifecycleRouteArmAst {
    pub state: String,
    /// URL pattern verbatim (quotes stripped).
    pub url: String,
    pub span: Span,
}

/// CL.C.4 — `aggregate <Name>` declaration block. DDD consistency
/// boundary: one `root` resource, a closed `contains` member list,
/// and zero-or-more invariants whose predicates span the cluster.
/// CL.C.4 `aggregate <Name>` block — DDD consistency boundary.
///
/// Pairs a single `root` resource with a closed `contains` member list
/// and cluster-spanning invariants. Lowers to `ir::Aggregate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregateDecl {
    pub name: String,
    /// `root <Resource>` — the consistency-boundary root.
    pub root: String,
    /// `contains <Resource>, <Resource>, ...` — comma-separated.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contains: Vec<String>,
    /// `invariants` sub-block — zero or more `invariant <name>` blocks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invariants: Vec<InvariantDecl>,
    pub span: Span,
}

/// CL.C.4 — `invariant <name>` declaration. Shared by `ResourceDecl`
/// and `AggregateDecl` (both surfaces author identical syntax). The
/// `when` text is parsed by the analyzer into `ir::EvalPredicate`; the
/// AST keeps it verbatim so doctor can echo the source on failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantDecl {
    pub name: String,
    /// `when <expr>` — verbatim predicate text (analyzer parses).
    pub when: String,
    /// `message "<text>"` — authored message body (empty when absent).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    pub span: Span,
}

/// One field row inside a [`ResourceDecl`] / [`RecordDecl`](crate::ast::RecordDecl).
///
/// Captures the typed-name + modifier-chain shape `<name>: <Type>
/// [required|optional|unique|@full_text|@slug] [= <default>] [derived
/// from <expr>] [<constraints>]` plus any `@owner_axis(through: <ident>)`
/// decorator the parser peels out of the raw type text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceFieldDecl {
    pub name: String,
    /// Raw type text including decorator chain. The analyzer projects
    /// to `TypeRef` via `type_ref_from_text`.
    pub type_text: String,
    pub required: bool,
    pub optional: bool,
    pub unique: bool,
    /// CL.C.4 — `@slug` field decorator. When `true` the field is the
    /// resource's URL slug. Doctor enforces implicit uniqueness via
    /// `slug-uniqueness-implicit`. Captured at parse time as a typed
    /// modifier (sibling of `required`/`optional`/`unique`).
    #[serde(default)]
    pub slug: bool,
    /// `= <expr>` default value (verbatim).
    pub default: Option<String>,
    /// `derived from <expr>` computed-field expression (Phase L Tier 4c).
    pub derived_from: Option<String>,
    /// W3 GAP-03 — `computed_date from <base> offset <offset>` typed
    /// computed-date field kind. `Some` when the field's value is a base
    /// `Date` field plus an integer day offset. The analyzer projects this
    /// onto `ir::Field.computed_date`; doctor `COMPUTED-DATE-EXPR-001`
    /// type-checks the base (must be a declared `Date` field) and offset
    /// (a declared `Integer` field or an integer literal). Mutually
    /// exclusive with `derived_from` at parse time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed_date: Option<ComputedDateAst>,
    /// L0 #3 §10 — inline constraints (`min N`, `max N`, `pattern
    /// STRING`, `between A and B`, `length N`, `in [...]`). Parser
    /// captures them; the analyzer validates combination rules and
    /// lifts into `ir::FieldConstraints`.
    #[serde(default, skip_serializing_if = "FieldConstraintsDecl::is_empty")]
    pub constraints: FieldConstraintsDecl,
    /// Roadmap §1.5 (CL.C.2) — `@full_text` decorator marks this field
    /// for Postgres GIN tsvector index emission. Mutually compatible
    /// with `required`/`optional`/`unique` modifiers. The analyzer
    /// rejects `@full_text` on non-text-like types.
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub full_text: bool,
    /// `ir-resource-conventions-owner-scope` §7.1 — `@owner_axis(through:
    /// <ident>)` field annotation. Parser peels the decorator out of
    /// `type_text` and lifts it here so the analyzer projects directly
    /// into `ir::Field.owner_axis`. Absent = field carries no ownership
    /// chain, synth pass uses tenant-scope (today's default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_axis: Option<OwnerAxisAst>,
    /// GAP-12 — `target @feature.<feature>.<Resource>` cross-feature FK
    /// annotation on an `ID` field. Parser peels the `target ...` clause
    /// out of the type tail and lifts it here so the analyzer projects
    /// into `ir::Field.cross_feature_target`. Absent = the field is a
    /// plain `ID` (or a same-feature FK via the bare-type-name path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cross_feature_target: Option<CrossFeatureTargetAst>,
    /// Child `previously migrated <old>` lines beneath the field.
    pub previously: Vec<String>,
    pub span: Span,
}

/// GAP-12 — typed payload for the `target @feature.<feature>.<Resource>`
/// cross-feature FK annotation. Names the owning feature and the resource
/// the `ID` field logically references. The reference is *logical*: codegen
/// does not emit a hard DB FOREIGN KEY across feature/migration-set
/// boundaries (the target table may be owned by a separately-ordered
/// migration set); the analyzer + doctor enforce that the feature is
/// declared in the consumer's `uses` (Dependencies) and that the resource
/// exists in it (`REF-CROSS-FEATURE-UNKNOWN-001`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossFeatureTargetAst {
    /// Owning feature name (the segment after `@feature.`).
    pub feature: String,
    /// Target resource name (PascalCase).
    pub resource: String,
}

/// W3 GAP-03 — typed payload for the `computed_date from <base> offset
/// <offset>` field kind. Surface syntax:
///
/// ```text
/// due_date: Date computed_date from campaign_start offset offset_days
/// ```
///
/// `base` names another `Date` field on the same resource; `offset` is
/// either a same-resource `Integer` field name or an integer literal
/// (number of days). The parser keeps the operands as raw identifiers /
/// literal; the analyzer lifts into `ir::ComputedDate` and doctor
/// (`COMPUTED-DATE-EXPR-001`) type-checks the references. Mirrors
/// `lazuli_ir::ComputedDate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputedDateAst {
    /// Selector for the base `Date` value. `Field` is the W3
    /// `computed_date from <field>` form; `Rule` is the W4 GAP-08
    /// `schedule_rule from @fn.<name>(<rule_arg>)` form.
    pub base: ComputedDateBaseAst,
    /// The `offset <offset>` operand.
    pub offset: ComputedDateOffsetAst,
}

/// W4 GAP-08 — AST mirror of `lazuli_ir::ComputedDateBase`. Selects the
/// base `Date` of a [`ComputedDateAst`] either by a same-resource field
/// name (W3 `computed_date`) or by a rule-enum value resolved through a
/// bound `@fn` (W4 `schedule_rule`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputedDateBaseAst {
    /// `computed_date from <field> ...` — bare same-resource `Date` field.
    Field(String),
    /// `schedule_rule from @fn.<name>(<rule_arg>) ...` — the base date is
    /// produced by the bound `@fn`, which selects it from the rule arg.
    Rule {
        /// The rule-enum argument inside the `@fn(...)` call (verbatim).
        rule: String,
        /// Bare binding-fn name (the `<name>` in `@fn.<name>`).
        fn_ref: String,
    },
}

/// W3 GAP-03 — the `offset` operand of a [`ComputedDateAst`]. Either a
/// same-resource `Integer` field name or an inline integer literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComputedDateOffsetAst {
    /// `offset <field>` — bare identifier naming an `Integer` field.
    Field(String),
    /// `offset <int>` — integer-literal day count.
    Literal(i64),
}

/// `ir-resource-conventions-owner-scope` §7.1 — AST-level mirror of
/// `ir::OwnerAxis`. `through_column` carries the bare identifier the
/// author wrote between the parens (e.g. `user` for the canonical pilot's
/// `Property → Host → User` chain). String-literal arguments are a
/// parse error (per §7.1, the value is a syntactic identifier).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerAxisAst {
    /// Bare identifier inside the parens of `@owner_axis(through: <ident>)`.
    pub through_column: String,
}

pub(crate) fn is_false_bool(value: &bool) -> bool {
    !*value
}

/// L0 #3 §10 — parser-side capture of the 6 inline field constraints.
/// Mirrors `ir::FieldConstraints` but stays in the AST layer so the
/// analyzer can apply combination + default-compat checks before
/// projecting into the IR. `r#in` values are stored verbatim (no
/// surrounding quotes for string literals; numerics as their text
/// form) — the analyzer / emitters interpret per type.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FieldConstraintsDecl {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sanitize_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utf8_safe: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_recursion: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covers_pii: Option<String>,
}

impl FieldConstraintsDecl {
    /// `true` when none of the constraint slots were authored. Used as
    /// the `skip_serializing_if` guard so untouched fields don't ship
    /// an empty constraints object in JSON.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::FieldConstraintsDecl;
    ///
    /// let empty = FieldConstraintsDecl::default();
    /// assert!(empty.is_empty());
    ///
    /// let with_min = FieldConstraintsDecl { min: Some(0), ..Default::default() };
    /// assert!(!with_min.is_empty());
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
}

/// One `has_many <name>: <Resource> [inverse <field>]` row on a [`ResourceDecl`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceHasMany {
    pub name: String,
    /// Resource type reference, e.g. `CustomerNote`.
    pub type_text: String,
    /// `inverse <field>` clause — captured verbatim.
    pub inverse: Option<String>,
    pub span: Span,
}

/// `retention <duration> then <action>` row on a [`ResourceDecl`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRetention {
    /// Duration literal, e.g. `7y`, `30d`. Captured verbatim.
    pub duration: String,
    /// `Anonymize | Delete | Archive` closed catalog.
    pub action: ResourceRetentionAction,
    pub span: Span,
}

/// Closed three-arm catalog for the `then <action>` clause of
/// [`ResourceRetention`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRetentionAction {
    /// Strip PII fields, keep the row.
    Anonymize,
    /// Hard-delete the row.
    Delete,
    /// Move the row to cold storage / archive table.
    Archive,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_constraints_decl_default_is_empty() {
        assert!(FieldConstraintsDecl::default().is_empty());
    }

    #[test]
    fn resource_retention_action_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(ResourceRetentionAction::Anonymize).unwrap(),
            serde_json::json!("anonymize")
        );
    }

    #[test]
    fn resource_index_method_ast_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(ResourceIndexMethodAst::Btree).unwrap(),
            serde_json::json!("btree")
        );
        assert_eq!(
            serde_json::to_value(ResourceIndexMethodAst::Gin).unwrap(),
            serde_json::json!("gin")
        );
    }

    #[test]
    fn resource_lock_optimistic_carries_version_field() {
        let l = ResourceLock::Optimistic {
            version_field: "lock_version".into(),
        };
        let v = serde_json::to_value(&l).unwrap();
        assert_eq!(v["kind"], "Optimistic");
        assert_eq!(v["version_field"], "lock_version");
    }
}
