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
    /// Spec 0015 — `soft_delete by` actor projection. When `true` the
    /// trait projects a `deleted_by: ID` column alongside `deleted_at`,
    /// populated from `ctx.actor` on the soft-delete write (mirroring how
    /// authors hand-rolled `deleted_by` next to `deleted_at`). Implies
    /// `soft_delete == true`. Additive: pre-0015 fixtures deserialize
    /// `false` (bare `soft_delete`, `deleted_at`-only — unchanged shape).
    #[serde(default, skip_serializing_if = "is_false_bool")]
    pub soft_delete_actor: bool,
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
    /// Spec 0018 — `crud` overlay block on a `conventions [crud]`
    /// resource. Carries per-effect (`create`/`update`/`delete`)
    /// policy / validate / `input excludes` / `assign` / `emits`
    /// clauses that the analyzer's conventions pass merges into the
    /// synthesized commands BEFORE lowering. The overlay never reaches
    /// IR as a resource field — it is consumed entirely in the synth
    /// pass (analyzer-only). Absent = today's bare synth, byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crud_overlay: Option<CrudOverlayAst>,
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
    /// Spec 0014 — `restrict on_delete references <relation> via <fk>
    /// [where <predicate>]` referential-guard clauses. Repeatable (one
    /// per inbound relation that blocks deletion). Lowered to
    /// `ir::RestrictOnDelete`; codegen emits a tenant-scoped,
    /// soft-delete-aware `EXISTS` precondition before every delete of the
    /// resource. Additive: pre-0014 fixtures deserialize with an empty vec.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restrict_on_delete: Vec<ResourceRestrictOnDelete>,
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
    /// `error <CODE>` — optional per-constraint domain error code (mirrors
    /// `restrict on_delete ... error <CODE>`). When set, codegen emits a
    /// DETERMINISTICALLY-named UNIQUE constraint plus runtime glue so a
    /// 23505 violation on it surfaces as `<CODE>` (409) instead of the
    /// generic `unique_violation`. `None` keeps the generic behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
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

/// Spec 0018 — the `crud` overlay block authored under a `conventions
/// [crud]` resource. Carries up to three per-effect overlays. Each is
/// optional; an empty `crud` block (header with no sub-blocks) is a
/// parse error (author omits the block entirely instead).
///
/// ```text
/// resource Customer
///   conventions [crud]
///   crud
///     create
///       policy @policy.edit
///       validate @validator.percentage
///       input excludes situation, is_active, is_defaulter
///       assign situation = prospect
///       assign is_active = true
///       assign category = input.category_id
///       emits customer_created
///     update
///       policy @policy.edit
///     delete
///       policy @policy.remove
/// ```
///
/// The block is consumed by the analyzer's conventions pass and merged
/// into the synthesized `create_<r>` / `update_<r>` / `delete_<r>`
/// commands before lowering, so the emitted IR is byte-identical to the
/// equivalent hand-rolled command. It never reaches `ir::Resource`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrudOverlayAst {
    /// `create` sub-block overlay (default-literal assigns + emits + ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub create: Option<CrudEffectOverlayAst>,
    /// `update` sub-block overlay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<CrudEffectOverlayAst>,
    /// `delete` sub-block overlay (policy + emits; soft-delete-aware via 0015).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delete: Option<CrudEffectOverlayAst>,
    pub span: Span,
}

/// Spec 0018 — one per-effect overlay (`create` / `update` / `delete`)
/// inside a [`CrudOverlayAst`]. Every clause is optional; merge semantics
/// (analyzer): `policy` REPLACES the synth default; `validate` / `emits` /
/// `assigns` ADD to the synthesized effect; `input_excludes` removes
/// fields from the synth-generated input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrudEffectOverlayAst {
    /// `policy @policy.<x>` — overrides the synth's `authenticated` default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
    /// `validate @validator.<v>` lines (0..n). Doctor-only on IR (the
    /// hand-rolled `validate` does not lower to a command field either),
    /// so these never affect IR-equivalence — carried for surface parity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validate: Vec<String>,
    /// `input excludes <field>, <field>` — system/derived fields dropped
    /// from the synth-generated input. Flattened across multiple lines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_excludes: Vec<String>,
    /// `assign <field> = <expr>` rows (0..n) merged into the synthesized
    /// `creates`/`updates` effect. RHS reuses the hand-rolled effect
    /// assignment grammar verbatim (literal / `input.<f>` / `ctx.<f>` /
    /// enum variant), captured as raw text the analyzer lowers via the
    /// same `lower_raw_expr` the hand-rolled effect uses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assigns: Vec<AssignmentDecl>,
    /// `emits <event>` lines (0..n) appended to the command's emits list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emits: Vec<String>,
    pub span: Span,
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
