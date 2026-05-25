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
use crate::nodes::capability::{CapabilityRef, PiiCapability};
use crate::nodes::feature_defaults::{Constraint, FieldValidation, PathRef, Tenancy};
use crate::nodes::lifecycle::Lifecycle;
use crate::{DefaultValue, PublicContract, QualifiedName, SpanRef, is_false};

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleRouteArm {
    /// Match key — lifecycle state name, `none`, or `*`.
    pub state: String,
    /// URL string the helper returns when the actor's row is in this
    /// lifecycle state.
    pub url: String,
}

/// Closed catalog of resource-level convention bundles. Adding a
/// variant is an IR change requiring a proposal; the parser MUST
/// reject any identifier not in this enum.
///
/// See `docs/proposals/ir-resource-conventions-crud.md` §4.2 and
/// `docs/proposals/ir-resource-conventions-me.md` §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConventionRef {
    /// `crud` — auto-synthesizes 3 commands + 2 queries (5 entries
    /// total) per `ir-resource-conventions-crud.md` §5.1.
    Crud,
    /// `me` — auto-synthesizes one `lookup_my_<resource>` query keyed
    /// by `ctx.User.ID` (or `ctx.User.OrgID` for org-only resources).
    /// See `ir-resource-conventions-me.md` §5.
    Me,
    // Future variants (NOT in this proposal):
    //   Timestamped, PiiAware, SoftDelete, Slugged, Paginated.
}

/// Origin of a synth-eligible entry name in `Feature.synth_origins`.
///
/// Cell C3's synthesis pass marks each name that a convention bundle
/// would have produced. The marker distinguishes the two relevant
/// states the inspect surface (§11) renders:
///
/// * `Synthesized(<bundle>)` — C3 appended this command/query as part
///   of the named bundle's expansion. Inspect annotates with
///   `[conv:<bundle>]`.
/// * `AuthorOverride(<bundle>)` — the name was in the bundle's set but
///   an author-written command/query already existed with the same
///   name. C3 skipped its synthesis. Inspect annotates with
///   `[author override; convention skipped]`.
///
/// Entries C3 does not touch (pure author-written commands not in any
/// convention's set) carry no entry in `synth_origins`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "convention", rename_all = "snake_case")]
pub enum ConventionOrigin {
    /// Entry was synthesized by the named bundle.
    Synthesized(ConventionRef),
    /// Author wrote this name; the named bundle would have synthesized it.
    AuthorOverride(ConventionRef),
}

impl ConventionOrigin {
    /// The bundle that produced (or would have produced) this entry.
    pub fn convention(&self) -> ConventionRef {
        match self {
            ConventionOrigin::Synthesized(c) | ConventionOrigin::AuthorOverride(c) => *c,
        }
    }

    /// `true` when an author wrote a command/query with this name and the
    /// convention's synth for that name was skipped.
    pub fn is_author_override(&self) -> bool {
        matches!(self, ConventionOrigin::AuthorOverride(_))
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionAction {
    Anonymize,
    Delete,
    Archive,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum StorageValue {
    Integer(i64),
    String(String),
}

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
/// (`user` for Hostpoint's Property → Host chain). Multi-hop chains
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
    /// in future cycles. Example: `"host"` for Hostpoint Property.
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
    pub fn new() -> Self {
        Self::default()
    }
}

/// Closed catalog of type references. Strings are forbidden; the analyzer
/// decides which variant a syntactic type name resolves to. Unrecognised
/// names become `TypeRef::Unresolved` so downstream consumers can surface a
/// targeted diagnostic without crashing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum TypeRef {
    Builtin(BuiltinType),
    UserDefined(QualifiedName),
    EnumRef(QualifiedName),
    Many(Box<TypeRef>),
    Unresolved(String),
    /// Phase L Tier 2 — capability decorators with structured
    /// arguments (`@cap.File(max_size:...,accept:...)`). Today only
    /// `File` is typed; other `@cap.*` decorators (`Hashed`,
    /// `Encrypted`, `Token`) stay as text-pattern in LSP and project
    /// through `Unresolved`/`UserDefined` until the cycle that types
    /// them lands.
    Capability(CapabilityRef),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuiltinType {
    Id,
    Text,
    Boolean,
    Integer,
    Decimal,
    Date,
    DateTime,
    Json,
    SemanticEmail,
    /// Per `docs/proposals/semantic-types-money-brazilian.md` v0.3 +
    /// MONEY-1 §3.2 of the hostpoint roadmap. Carries the declared ISO
    /// 4217 currency so downstream doctor checks (MONEY-COMPARE-001,
    /// MONEY-ARITHMETIC-001) can reject mixed-currency operations at
    /// analyse time without re-walking surface text. The default
    /// authoring shorthand `Money` lowers to `SemanticMoney { currency:
    /// BRL }` (Hostpoint-pilot reality); explicit
    /// `@semantic.Money(currency: <ISO>)` overrides.
    SemanticMoney {
        currency: CurrencyCode,
    },
    /// Phase L Tier 4 follow-up — `@semantic.Phone`. Closed catalog
    /// addition so auth-identity diagnostics can read the shape
    /// without text-walking.
    SemanticPhone,
    /// Phase L Tier 4 follow-up — `@semantic.Url`.
    SemanticUrl,
    /// Phase L Tier 4 follow-up — `@semantic.Uuid`.
    SemanticUuid,
    /// Currency follow-up — `@semantic.Currency`. ISO 4217 3-letter
    /// uppercase code (`USD`, `BRL`). Pairs with `SemanticMoney` for
    /// typed amount-currency tuples; emitter maps to Go `lazuli.Currency`
    /// alias (already exists in `runtime/go/lazuli/types.go`).
    SemanticCurrency,
    /// GeoPoint follow-up (2026-05-11) — `@semantic.GeoPoint`.
    /// Closed-catalog single semantic carrying `{ lat, lng }`. Required
    /// by `codegen-lazuli-go.md` §6.3/§9.1 to materialise as
    /// `postgis.Point` in generated Go + drive the `GIST` index
    /// emission in DDL migrations.
    SemanticGeoPoint,
    /// B3 — plugin-contributed `@semantic.<Name>` resolved through a
    /// plugin's `manifest.toml`. The IR layer is locale-agnostic: it
    /// knows only the declaring plugin namespace (`@lazuli/plugin-scalars-br`),
    /// the manifest-local alias terminal name (`BrazilianCPF`), the
    /// carrier built-in (currently always `Text`), and the validator
    /// function name from the manifest. Codegen reads the validator
    /// to build the `<plugin-short>.<validator>` go-playground tag
    /// without re-reading the manifest at emission time. The plugin
    /// owns checksum rules, formatting, and any upstream library. See
    /// `docs/proposals/semantic-types-plugin-locales.md`.
    SemanticPluginType {
        plugin: String,
        name: String,
        carrier: Box<BuiltinType>,
        /// Exported Go function on the plugin adapter (e.g.
        /// `ValidateCPF`). Carried so codegen can emit the validate
        /// tag without re-reading `manifest.toml`. Authoritative
        /// source is the plugin's manifest `[[semantic_types]].validator`
        /// — the resolver pass copies it here at lift time.
        validator: String,
        /// W2 (ir-semantic-auto-validate-2026-05-22): effective Go
        /// module path of the plugin (`lazuli.dev/plugin/scalars-br`).
        /// Plugin-level value or convention fallback. Empty when the
        /// IR predates W2 lift.
        #[serde(default)]
        go_module: String,
        /// W2: effective TS/npm package (`@lazuli/plugin-scalars-br`).
        #[serde(default)]
        ts_package: String,
        /// W2: effective error code surfaced on validation_failed
        /// (`cpf_invalid`).
        #[serde(default)]
        error_code: String,
        /// W2: optional i18n message key. Empty when not declared.
        #[serde(default)]
        message_key: String,
        /// W2: TS validator function (`validateCPF`). Empty when not
        /// declared — TS preflight emission is skipped.
        #[serde(default)]
        ts_validator: String,
    },
    CapSecret,
    /// Deprecated: the flat `CapFile` variant never carried arguments.
    /// Phase L Tier 2 introduces `TypeRef::Capability(CapabilityRef::File(...))`
    /// which carries the parsed `max_size`/`accept`/`visibility`/`signed_ttl`
    /// slots. Kept for back-compat with serialized payloads predating the
    /// typed shape.
    CapFile,
}

/// MONEY-1 §3.2 — closed-catalog ISO 4217 codes the language understands
/// at IR time. Expansion is additive: new currencies land here when a
/// pilot demands them. Other ISO codes the user might type fall through
/// to the analyzer's "unknown currency" diagnostic and never reach IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CurrencyCode {
    BRL,
    USD,
    EUR,
    GBP,
    JPY,
    CHF,
}

impl CurrencyCode {
    /// Canonical 3-letter ISO 4217 form (`"BRL"`, `"USD"`...). Used by
    /// codegen to emit the `CHECK (<col> = '<ISO>')` constraint and by
    /// doctor diagnostics when interpolating into messages.
    pub fn as_iso(&self) -> &'static str {
        match self {
            Self::BRL => "BRL",
            Self::USD => "USD",
            Self::EUR => "EUR",
            Self::GBP => "GBP",
            Self::JPY => "JPY",
            Self::CHF => "CHF",
        }
    }

    /// Parse a 3-letter ISO 4217 code into the closed catalog. Returns
    /// `None` for unknown codes; the analyzer surfaces that as a typed
    /// diagnostic rather than silently accepting it.
    pub fn from_iso(raw: &str) -> Option<Self> {
        match raw {
            "BRL" => Some(Self::BRL),
            "USD" => Some(Self::USD),
            "EUR" => Some(Self::EUR),
            "GBP" => Some(Self::GBP),
            "JPY" => Some(Self::JPY),
            "CHF" => Some(Self::CHF),
            _ => None,
        }
    }
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
