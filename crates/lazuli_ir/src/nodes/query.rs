//! Query IR — declarative reads (`query.list`, `query.lookup`,
//! `query.sql`, `query.compose`).
//!
//! A [`Query`] is the lowered shape of one of four read kinds:
//!
//! - [`ListQuery`] — `query.list <name>` returning a collection.
//! - [`LookupQuery`] — `query.lookup <name>` returning a single row.
//! - [`SqlQuery`] — `query.sql <name>` returning the declared shape from
//!   a hand-rolled SQL file.
//! - [`ComposeQuery`] — `query.compose <name>` declarative composite read
//!   (root resource + FK-path JOIN projection + a CLOSED 4-member
//!   sub-select catalog), lowering to one analyzable SELECT. See
//!   `docs/proposals/ir-composite-read-primitive-2026-05-29.md`.
//!
//! Cross-cutting decorators (`cache`, `policy`, `policy_when_denied`,
//! `scope`, `filters`, `order`, `paginate`, `modifier`) are shared across
//! the kinds where they apply.
//!
//! ## Catalog
//!
//! - [`Query`] — sum type over the four read kinds.
//! - [`ListQuery`] / [`LookupQuery`] / [`SqlQuery`] / [`ComposeQuery`] —
//!   concrete shapes.
//! - [`ComposeJoin`] / [`ComposeProjection`] / [`ProjectionSource`] /
//!   [`ComposeSubselect`] / [`SubselectKind`] / [`AggFn`] / [`FkPath`] —
//!   the composite-read sub-shapes.
//! - [`SqlQueryKind`] — closed catalog `Sql` / `View`.
//! - [`QueryCache`] — typed `cache { key, ttl, tags?, namespace? }` block
//!   on a query.
//! - [`CacheProfile`] — feature-level `cache <name>` profile.
//! - [`CacheTtl`] / [`CacheTtlLiteral`] — typed duration carrier.
//! - [`Filter`] / [`KeyClause`] / [`OrderBy`] / [`OrderDir`] — list/lookup
//!   query parts.
//! - [`Predicate`] / [`CompareOp`] — the closed predicate sublanguage.
//! - [`Expr`] / [`FnCallExpr`] / [`Path`] — the read-side expression
//!   sublanguage. `Expr` reaches into `EnumLiteral` for typed enum
//!   defaults, and `FnCall` lowers a `@fn.<name>(...)` invocation.

use serde::{Deserialize, Serialize};

use crate::nodes::command::{PolicyRef, TypedSlot};
use crate::nodes::error_vocab::TranslationKeyRef;
use crate::nodes::resource::{OwnerScopeSql, TypeRef};
use crate::{EnumLiteral, PolicyExpr, PublicContract, QualifiedName, SpanRef, is_false};

/// Sum over the three declarative read kinds. Use the variant payloads
/// for kind-specific shape (filters, keys, sql_path); use [`Query::name`]
/// when you just need the identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Query {
    /// `query.list <name>` — collection read.
    List(ListQuery),
    /// `query.lookup <name>` — single-row read.
    Lookup(LookupQuery),
    /// `query.sql <name>` — typed hand-rolled SQL.
    Sql(SqlQuery),
    /// `query.compose <name>` — declarative composite read. See
    /// [`ComposeQuery`] and
    /// `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §5.1.
    Compose(ComposeQuery),
}

impl Query {
    /// Return the authored name regardless of which read kind this is.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// # use lazuli_ir::{Query, ListQuery};
    /// # let q: Query = unimplemented!();
    /// let name = q.name();
    /// ```
    pub fn name(&self) -> &str {
        match self {
            Query::List(q) => &q.name,
            Query::Lookup(q) => &q.name,
            Query::Sql(q) => &q.name,
            Query::Compose(q) => &q.name,
        }
    }
}

/// Cache bucket cycle — typed `cache` block on `query.list` / `query.sql`.
/// Adapters parse `key` verbatim; `ttl` is closed-catalog literal or
/// quoted prose; `tags`/`namespace` are author-defined labels used for
/// fan-out invalidation cross-checks.
///
/// Two shapes coexist on a query:
///  * Inline (legacy `cache { key, ttl, tags?, namespace? }`) — every
///    decorator authored at the query site.
///  * Profile reference (`cache <name>`) — `profile_ref: Some(name)` and
///    the resolved `key`/`ttl`/... mirror the feature-level
///    [`CacheProfile`] body. Lowering copies the body into this struct
///    so codegen / runtime never have to redo the lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryCache {
    /// `cache key <expr>` — opaque template stored verbatim.
    pub key: String,
    /// `cache ttl <literal>` — typed duration or quoted prose.
    pub ttl: CacheTtl,
    /// `cache tags <label>[, <label>...]` — lowercase identifiers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `cache namespace <label>` — single label; `None` defaults to the
    /// feature name at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// `cache <profile_name>` reference form. `None` for the inline
    /// shape; `Some(name)` when the query referenced a feature-level
    /// [`CacheProfile`] by name (`cache product_view`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_ref: Option<String>,
}

/// Cache bucket cycle (CL.C.3) — feature-level `cache <name>` profile.
///
/// First-class declaration sibling to `job`/`webhook`/`notification`.
/// Queries opt into a profile via `cache <profile_name>`; the inline
/// `cache { key, ttl, ... }` shape on a query keeps working for
/// one-off ttl/key pairs that don't need a named profile.
///
/// The four decorators beyond `key/ttl/tags/namespace`
/// (`stale_while_revalidate`, `coalesce`, `sliding`) carry the
/// runtime contract:
///  * `stale_while_revalidate` — serve stale value up to N seconds
///    after expiry while a background refresh runs (RFC 5861-flavoured).
///  * `coalesce` — single-flight populate when multiple readers miss
///    the same key concurrently.
///  * `sliding` — every read extends the TTL window.
///
/// The runtime owns the execution semantics; this struct declares the
/// contract typed end-to-end so doctor/codegen can cross-check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheProfile {
    /// Profile identifier. Lowercase identifier; dash-separated allowed.
    pub name: String,
    /// `key <expr>` — opaque template stored verbatim. Required.
    pub key: String,
    /// `ttl <literal>` — typed duration or quoted prose. Required.
    pub ttl: CacheTtl,
    /// `namespace <label>` — single label; `None` defaults to the
    /// feature name at runtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// `tags <label>[, <label>...]` — lowercase identifiers used by
    /// `invalidates tag:<label>` on commands for fan-out invalidation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `stale_while_revalidate <duration>` — typed duration. `None`
    /// means the runtime refreshes synchronously on TTL miss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_while_revalidate: Option<CacheTtl>,
    /// `coalesce <bool>` — single-flight populate on concurrent miss.
    /// `None` means "runtime default" (today: false). The boolean is
    /// the language-visible knob; per-key window tuning is a runtime
    /// adapter concern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coalesce: Option<bool>,
    /// `sliding <bool>` — sliding TTL semantics (read extends expiry).
    /// `None` means fixed TTL. Requires `ttl` (doctor enforces).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sliding: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// Cache bucket cycle — `ttl` literal or quoted prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum CacheTtl {
    /// `ttl 5m` — typed literal with closed unit catalog.
    Literal(CacheTtlLiteral),
    /// `ttl "5 minutes"` — adapter-parsed prose, preserved verbatim.
    Quoted(String),
}

/// Cache bucket cycle — typed duration literal. Closed catalog:
/// `s` (seconds), `m` (minutes), `h` (hours), `d` (days).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", content = "amount")]
pub enum CacheTtlLiteral {
    Seconds(u32),
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

/// `query.list <name>` — collection read. Carries the typed parameter
/// list, optional owner-scope predicates, filters, ordering, pagination
/// cap, and per-query cache + policy decorators. The analyzer composes
/// `owner_scope_sql` at synth time when the target resource bears
/// `@owner_axis`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<OrderBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginate: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier: Option<String>,
    /// Cache bucket cycle — typed `cache` block (key/ttl/tags?/namespace?).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<QueryCache>,
    /// Per-query authored `policy @policy.<name>`. Mirrors
    /// `Command.policy`. `PolicyRef::None` (the default) means "fall
    /// back to the feature-level default policy" — preserving the
    /// pre-existing precedence for fixtures that never authored a
    /// per-query policy.
    #[serde(default, skip_serializing_if = "PolicyRef::is_none")]
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form when the authored
    /// policy contained predicates (`has_role` / `has_permission` /
    /// `authenticated`) or boolean combinators. Coexists with `policy`
    /// (legacy atom ref) for back-compat. Mirrors `Command.policy_expr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-query override for the `policy_denied`
    /// error message. Highest-precedence step in the resolution chain
    /// for queries (proposal §3.3). Codegen wiring tracks the
    /// `Command` slot — queries are end-user-reachable HTTP boundaries
    /// in the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
    /// `ir-resource-conventions-owner-scope.md` §7.3 + §8.4 — owner-
    /// scope WHERE fragment composed at synth time for `list_<r>`
    /// when the resource bears `@owner_axis`. See Cell O2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_scope_sql: Option<OwnerScopeSql>,
}

/// `query.lookup <name>` — single-row read keyed by one or more
/// [`KeyClause`] equalities. Pairs with optional owner-scope predicates
/// and the standard policy/filter decorators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    pub keys: Vec<KeyClause>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    /// Per-query authored `policy @policy.<name>`. Mirrors
    /// `Command.policy`. `PolicyRef::None` (the default) means "fall
    /// back to the feature-level default policy".
    #[serde(default, skip_serializing_if = "PolicyRef::is_none")]
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form. Mirrors
    /// `Command.policy_expr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-query override for the `policy_denied`
    /// error message. Same shape as `Command.policy_when_denied`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
    /// `ir-resource-conventions-owner-scope.md` §7.3 + §8.3 — owner-
    /// scope WHERE fragment composed at synth time for `lookup_<r>`
    /// AND `lookup_my_<r>` when the resource bears `@owner_axis`. See
    /// Cell O2. `None` for author-written queries or tenant-only
    /// shapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_scope_sql: Option<OwnerScopeSql>,
}

/// `query.sql <name>` — hand-rolled SQL whose return shape is declared
/// inline via `returns <Type>`. Codegen wires the SQL file at
/// `sql_path` into a typed function; doctor validates the path exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "SqlQueryKind::is_sql")]
    pub sql_kind: SqlQueryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    pub returns: TypeRef,
    pub sql_path: String,
    /// Cache bucket cycle — typed `cache` block. Same shape as
    /// `ListQuery.cache`. `LookupQuery` deliberately does not gain a
    /// cache slot (lookup caching is runtime-implicit; the fixture
    /// only authors cache on list/sql shapes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache: Option<QueryCache>,
    /// Per-query authored `policy @policy.<name>`. Mirrors
    /// `Command.policy`. `PolicyRef::None` (the default) means "fall
    /// back to the feature-level default policy".
    #[serde(default, skip_serializing_if = "PolicyRef::is_none")]
    pub policy: PolicyRef,
    /// RB.S6 — structured `policy <expr>` form. Mirrors
    /// `Command.policy_expr`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    /// IR Error-Vocab — per-query override for the `policy_denied`
    /// error message. Same shape as `Command.policy_when_denied`. See
    /// `docs/proposals/ir-error-messages-vocab.md` §3.3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// `query.compose <name>` — declarative composite read (proposal
/// `ir-composite-read-primitive-2026-05-29.md` §5.1).
///
/// Rooted at exactly one resource ([`Self::root`]), projecting columns
/// from itself and FK-reachable neighbors ([`Self::joins`] +
/// [`Self::projections`]), plus a CLOSED 4-member per-row sub-select
/// catalog ([`Self::subselects`]). Inherits tenant/soft-delete scope like
/// [`ListQuery`] and lowers (W5) to one parameterized SELECT the analyzer
/// fully understands because it generated it.
///
/// Everything but the `Compose*` sub-shapes reuses existing IR atoms
/// ([`TypedSlot`], [`Filter`], [`Predicate`], [`KeyClause`], [`OrderBy`],
/// [`PolicyRef`], [`PolicyExpr`], [`TypeRef`], [`OwnerScopeSql`]). No
/// [`crate::Feature`] slot changes — `ComposeQuery` lives inside the
/// existing [`Query`] sum the domain aggregator already carries.
///
/// ## Pinned constraints (§3.2)
///
/// * [`Self::key`] presence ⇒ single-row ⇒ framework `not_found` on zero
///   rows (mirrors [`LookupQuery`]); `None` ⇒ list semantics.
/// * sub-select `where`/`filter` predicates are the closed predicate
///   language plus `in [...]` with a literal-set RHS ONLY (the parser
///   rejects `in (subselect)` / `in params.x` / `in <expr>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeQuery {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContract>,
    /// `from <Resource>` — the single root resource.
    pub root: TypeRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<TypedSlot>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub joins: Vec<ComposeJoin>,
    /// `select` projections (≥1); derives the return record.
    pub projections: Vec<ComposeProjection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subselects: Vec<ComposeSubselect>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    /// `key` clause — presence makes this a single-row read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<KeyClause>,
    /// Local safety `scope` predicates, atop the inherited tenant/soft-
    /// delete scope.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scope: Vec<Predicate>,
    /// `scope override` flag — requires `policy` + `reason`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub scope_override: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<OrderBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paginate: Option<u32>,
    /// Per-query authored `policy @policy.<name>`. `PolicyRef::None`
    /// falls back to the feature-level default policy.
    #[serde(default, skip_serializing_if = "PolicyRef::is_none")]
    pub policy: PolicyRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExpr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_when_denied: Option<TranslationKeyRef>,
    /// Generated return record (default `<Compose>Row` when the author
    /// omitted `returns`).
    pub returns: TypeRef,
    /// W2 — resolved origin of the row-level scope predicate the runtime
    /// will inject into the generated SELECT's root `WHERE`. The
    /// load-bearing verifiability artifact: the analyzer stamps this at
    /// lowering time so inspect (W6) can surface
    /// `origin: inherited:tenancy+soft_delete` and doctor
    /// (`COMPOSE-SCOPE-UNGROUNDED-001`, W3) has a direct anchor without
    /// re-deriving from `(root tenancy, scope_override)`. The default
    /// ([`ComposeScopeOrigin::Inherited`]) is the secure case — the
    /// tenant/soft-delete predicate is GENERATED by codegen exactly like
    /// [`ListQuery`], so it cannot be forgotten. See §5.2 / §5.3.
    #[serde(default, skip_serializing_if = "ComposeScopeOrigin::is_default")]
    pub scope_origin: ComposeScopeOrigin,
    /// `@owner_axis` lowering — owner-scope WHERE fragment composed at
    /// synth time. `None` for tenant-only shapes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_scope_sql: Option<OwnerScopeSql>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span_ref: Option<SpanRef>,
}

/// W2 — resolved origin of a [`ComposeQuery`]'s row-level scope predicate.
///
/// The single fact W3 (`COMPOSE-SCOPE-UNGROUNDED-001`) and W6 (inspect's
/// `scope.origin`) read to prove the tenant/soft-delete predicate is present
/// without reading a handler — the exact auditable property `query.sql`
/// cannot offer (§5.3). It is GENERATED by the analyzer, never authored:
///
/// - [`Self::Inherited`] (the default + secure case): the read inherits
///   tenant + soft-delete scope exactly like [`ListQuery`]. Codegen injects
///   the predicates from `effective_tenancy` into the root `WHERE`, so a
///   dropped tenant predicate is structurally impossible for the absorbed
///   shapes.
/// - [`Self::Overridden`]: the author wrote `scope override` (requires
///   `policy` + `reason`, mirroring the other queries). The inherited
///   predicate is suppressed by explicit, doctor-gated opt-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposeScopeOrigin {
    /// Inherited `tenancy` + `soft_delete` scope (the default). Surfaces as
    /// `origin: inherited:tenancy+soft_delete` in inspect.
    #[default]
    Inherited,
    /// `scope override` — inherited tenancy suppressed by explicit opt-out.
    Overridden,
}

impl ComposeScopeOrigin {
    /// `true` when this is the default [`Self::Inherited`] origin — the
    /// serde `skip_serializing_if` guard so the secure default never leaks
    /// a discriminator into the JSON wire shape.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::ComposeScopeOrigin;
    ///
    /// assert!(ComposeScopeOrigin::Inherited.is_default());
    /// assert!(!ComposeScopeOrigin::Overridden.is_default());
    /// ```
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Inherited)
    }
}

/// An FK path — one-or-more lowercase relation segments (`host.user`),
/// resolved (W2+) against the IR relation graph. A JOIN by FK PATH, never
/// by an authored `ON`-clause string — the single biggest closure lever
/// (§3.2 #2). Stored as a flat segment list so consumers never re-split a
/// delimited string (mirrors [`Path`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FkPath {
    pub segments: Vec<String>,
}

impl FkPath {
    /// Build an [`FkPath`] from any iterable of stringy segments.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::FkPath;
    ///
    /// let p = FkPath::from_segments(["host", "user"]);
    /// assert_eq!(p.segments, vec!["host", "user"]);
    /// ```
    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
        }
    }
}

/// One `join <fk.path> [as <alias>] [optional]` on a [`ComposeQuery`].
/// `nullable` ⇒ LEFT JOIN; default ⇒ INNER. The `ON` clause is GENERATED
/// from the resolved relation edge, never authored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeJoin {
    pub path: FkPath,
    pub alias: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub nullable: bool,
}

/// One `<name> = <source>` projection on a [`ComposeQuery`]. The set of
/// projections derives the generated return record 1:1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeProjection {
    pub name: String,
    pub source: ProjectionSource,
}

/// Where a [`ComposeProjection`] reads from. Closed three-arm catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ProjectionSource {
    /// `self.<column>` — a column on the root resource.
    SelfCol(String),
    /// `<alias>.<column>` — a column on a joined node.
    Joined(String, String),
    /// A declared `subselect` name.
    Subselect(String),
}

/// The CLOSED 4-member sub-select kind catalog (§3.1 `subselect_kind`).
/// Each is a scalar-per-row computation over a resource related to root —
/// NOT an arbitrary subquery. Adding a kind is a spec edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum SubselectKind {
    /// `count <Resource>`.
    Count(TypeRef),
    /// `exists <Resource>` (with `negate` ⇒ NOT EXISTS anti-join).
    Exists { resource: TypeRef, negate: bool },
    /// `latest <column> of <Resource>`.
    Latest { column: String, resource: TypeRef },
    /// `aggregate <fn> <column> of <Resource>`.
    Aggregate {
        func: AggFn,
        column: String,
        resource: TypeRef,
    },
}

/// The CLOSED 5-member aggregate-function catalog (§3.1 `agg_fn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggFn {
    Sum,
    Avg,
    Min,
    Max,
    CountDistinct,
}

/// One `subselect <name> = <kind>` on a [`ComposeQuery`]. The
/// `related_by` FK path correlates the child to root; `where_pred` /
/// `filter_pred` use the closed predicate language; `order` is for
/// `latest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeSubselect {
    pub name: String,
    pub kind: SubselectKind,
    /// `related_by <fk.path>` — the resolved child→root edge.
    pub related_by: FkPath,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub where_pred: Vec<Predicate>,
    /// SQL `FILTER (WHERE ...)` on an aggregate.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter_pred: Vec<Predicate>,
    /// For `latest`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<OrderBy>,
}

/// Closed catalog distinguishing a regular SQL query from a SQL view
/// declaration. `Sql` is the default; `View` flags `query.sql.view` for
/// codegen + doctor specialisation (views are materialised differently).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SqlQueryKind {
    /// Standard `query.sql` — direct SQL file execution.
    #[default]
    Sql,
    /// `query.sql.view` — surfaces as a materialised SQL view.
    View,
}

impl SqlQueryKind {
    /// Returns `true` when the kind is `Sql` (the default). Used by
    /// `skip_serializing_if` so the default round-trips without leaking
    /// the discriminator into JSON.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::SqlQueryKind;
    ///
    /// assert!(SqlQueryKind::Sql.is_sql());
    /// assert!(!SqlQueryKind::View.is_sql());
    /// ```
    pub fn is_sql(&self) -> bool {
        matches!(self, Self::Sql)
    }
}

/// One `filter <predicate>` entry on a list/lookup/sql query. The
/// optional `when` ties the filter to a typed parameter so codegen can
/// skip the predicate when the param is unset (`when params.search`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    pub predicate: Predicate,
    /// `Some(param_name)` for guarded `when params.X` filters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

/// One `<path> = <expr>` lookup key. Lookups can declare multiple keys
/// for composite primary keys; the analyzer enforces all-or-none cover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyClause {
    pub path: Path,
    pub equals: Expr,
}

/// One `order by <field> <asc|desc>` entry on a list query. Multiple
/// entries are applied in authored order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderBy {
    pub field: String,
    pub direction: OrderDir,
}

/// Ordering direction. Closed catalog `Asc` / `Desc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderDir {
    /// Ascending (lowest first).
    Asc,
    /// Descending (highest first).
    Desc,
}

/// Closed predicate sublanguage. See `docs/canonical-semantics.md` "Predicate
/// Expressions" for the ceiling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Predicate {
    Comparison {
        left: Expr,
        op: CompareOp,
        right: Expr,
    },
    Has {
        collection: Expr,
        element: Expr,
    },
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
}

/// Comparison operators admitted in [`Predicate::Comparison`]. `Eq` /
/// `Ne` are universally admissible; the ordered operators (`Lt`/`Le`/
/// `Gt`/`Ge`) are restricted to numeric sites and inside eval blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompareOp {
    /// `==`.
    Eq,
    /// `!=`.
    Ne,
    /// `<`. Cut A — ordered operators are admissible only inside an agent's
    /// `evals` block (proposal §A3). Doctor diagnostic
    /// `eval_ordered_op_invalid_diagnostics` rejects them outside that
    /// scope and when either side resolves to a non-numeric type.
    Lt,
    /// `<=`. See `Lt` for scope restriction.
    Le,
    /// `>`. See `Lt` for scope restriction.
    Gt,
    /// `>=`. See `Lt` for scope restriction.
    Ge,
}

/// Read-side expression sublanguage. Closed catalog of leaves
/// (literals + paths + enum constants) plus the one composite shape
/// `FnCall` for `@fn.<name>(...)` invocations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Expr {
    /// `params.x` / `ctx.user.id` / `target.field`.
    Path(Path),
    /// `"hello"`.
    String(String),
    /// `42`.
    Integer(i64),
    /// `true` / `false`.
    Boolean(bool),
    /// `Color.Red`.
    Enum(EnumLiteral),
    /// `nil`.
    Nil,
    /// `@fn.<name>(<arg>...)` — closes WAR-VOCAB-CREATES-FN-CALL-01.
    /// Declarative `creates`/`updates` field bindings can now invoke
    /// a user-registered extension fn at command time. Codegen lowers
    /// to a `lazuli.FromFn` source; runtime resolves args first, then
    /// invokes the registered fn with the resolved args.
    FnCall(FnCallExpr),
}

/// `@fn.<name>(<arg>...)` invocation. `name` is the qualified fn
/// reference; `args` are the resolved argument expressions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FnCallExpr {
    pub name: QualifiedName,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<Expr>,
}

/// Dotted path expression (e.g. `ctx.user.id`). Stored as a flat segment
/// list so consumers never have to re-split a delimited string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Path {
    pub segments: Vec<String>,
}

impl Path {
    /// Build a [`Path`] from any iterable of stringy segments.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_ir::Path;
    ///
    /// let p = Path::from_segments(["ctx", "user", "id"]);
    /// assert_eq!(p.segments, vec!["ctx", "user", "id"]);
    /// ```
    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_name_dispatches_across_kinds() {
        let v = Query::List(ListQuery {
            name: "all".into(),
            public_contract: None,
            params: vec![],
            scope: vec![],
            scope_override: false,
            filters: vec![],
            order: vec![],
            paginate: None,
            modifier: None,
            cache: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            previous_names: vec![],
            span_ref: None,
            owner_scope_sql: None,
        });
        assert_eq!(v.name(), "all");
    }

    #[test]
    fn sql_query_kind_default_is_sql() {
        assert!(SqlQueryKind::default().is_sql());
        assert!(!SqlQueryKind::View.is_sql());
    }

    #[test]
    fn cache_ttl_literal_round_trips_minutes() {
        let v = CacheTtl::Literal(CacheTtlLiteral::Minutes(5));
        let s = serde_json::to_string(&v).expect("serialize");
        assert!(s.contains("\"kind\":\"Literal\""));
        assert!(s.contains("\"unit\":\"Minutes\""));
        assert!(s.contains("\"amount\":5"));
        let back: CacheTtl = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn path_from_segments_collects_strs() {
        let p = Path::from_segments(["a", "b", "c"]);
        assert_eq!(p.segments, vec!["a", "b", "c"]);
    }

    #[test]
    fn predicate_comparison_round_trips() {
        let v = Predicate::Comparison {
            left: Expr::Path(Path::from_segments(["x"])),
            op: CompareOp::Eq,
            right: Expr::Integer(7),
        };
        let s = serde_json::to_string(&v).expect("serialize");
        let back: Predicate = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn order_dir_round_trips() {
        let v = OrderDir::Asc;
        let s = serde_json::to_string(&v).expect("serialize");
        let back: OrderDir = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    // =========================================================================
    // query.compose IR (W1) —
    // `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §5.1.
    // =========================================================================

    fn type_ref(name: &str) -> TypeRef {
        TypeRef::UserDefined(crate::QualifiedName {
            feature: None,
            name: name.into(),
        })
    }

    fn sample_compose() -> ComposeQuery {
        ComposeQuery {
            name: "property_kpis".into(),
            public_contract: None,
            root: type_ref("Property"),
            params: vec![],
            joins: vec![ComposeJoin {
                path: FkPath::from_segments(["property", "host"]),
                alias: "h".into(),
                nullable: false,
            }],
            projections: vec![
                ComposeProjection {
                    name: "property_id".into(),
                    source: ProjectionSource::SelfCol("id".into()),
                },
                ComposeProjection {
                    name: "host_name".into(),
                    source: ProjectionSource::Joined("h".into(), "name".into()),
                },
                ComposeProjection {
                    name: "revenue_cents".into(),
                    source: ProjectionSource::Subselect("revenue_cents".into()),
                },
            ],
            subselects: vec![ComposeSubselect {
                name: "revenue_cents".into(),
                kind: SubselectKind::Aggregate {
                    func: AggFn::Sum,
                    column: "total_amount_cents".into(),
                    resource: type_ref("ServiceTransaction"),
                },
                related_by: FkPath::from_segments(["service_transaction", "property"]),
                where_pred: vec![],
                filter_pred: vec![Predicate::Comparison {
                    left: Expr::Path(Path::from_segments(["status"])),
                    op: CompareOp::Eq,
                    right: Expr::String("paid".into()),
                }],
                order: vec![],
            }],
            filters: vec![],
            key: Some(KeyClause {
                path: Path::from_segments(["self", "id"]),
                equals: Expr::Path(Path::from_segments(["params", "property_id"])),
            }),
            scope: vec![],
            scope_override: false,
            order: vec![],
            paginate: None,
            policy: PolicyRef::None,
            policy_expr: None,
            policy_when_denied: None,
            returns: type_ref("PropertyKpiRow"),
            scope_origin: ComposeScopeOrigin::Inherited,
            owner_scope_sql: None,
            previous_names: vec![],
            span_ref: None,
        }
    }

    #[test]
    fn query_name_dispatches_for_compose() {
        let q = Query::Compose(sample_compose());
        assert_eq!(q.name(), "property_kpis");
    }

    #[test]
    fn compose_query_round_trips_through_serde() {
        let q = Query::Compose(sample_compose());
        let s = serde_json::to_string(&q).expect("serialize");
        // The `kind` tag distinguishes the new variant on the wire.
        assert!(s.contains("\"kind\":\"Compose\""), "got {s}");
        let back: Query = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(q, back);
    }

    #[test]
    fn fk_path_from_segments_collects() {
        let p = FkPath::from_segments(["host", "user"]);
        assert_eq!(p.segments, vec!["host", "user"]);
    }

    #[test]
    fn subselect_kind_catalog_round_trips() {
        for kind in [
            SubselectKind::Count(type_ref("ChatMessage")),
            SubselectKind::Exists {
                resource: type_ref("Review"),
                negate: true,
            },
            SubselectKind::Latest {
                column: "body".into(),
                resource: type_ref("ChatMessage"),
            },
            SubselectKind::Aggregate {
                func: AggFn::CountDistinct,
                column: "rating".into(),
                resource: type_ref("Review"),
            },
        ] {
            let s = serde_json::to_string(&kind).expect("serialize");
            let back: SubselectKind = serde_json::from_str(&s).expect("deserialize");
            assert_eq!(kind, back);
        }
    }

    #[test]
    fn agg_fn_round_trips_snake_case() {
        let s = serde_json::to_string(&AggFn::CountDistinct).expect("serialize");
        assert_eq!(s, "\"count_distinct\"");
        let back: AggFn = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(AggFn::CountDistinct, back);
    }

    #[test]
    fn projection_source_round_trips() {
        let v = ProjectionSource::Joined("h".into(), "name".into());
        let s = serde_json::to_string(&v).expect("serialize");
        let back: ProjectionSource = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(v, back);
    }

    #[test]
    fn compose_scope_origin_default_skips_on_wire() {
        // W2 — the secure default (`Inherited`) must not leak a
        // discriminator: a tenant-inherited compose serializes WITHOUT a
        // `scope_origin` key, and a fresh deserialize recovers the default.
        let q = Query::Compose(sample_compose());
        let s = serde_json::to_string(&q).expect("serialize");
        assert!(!s.contains("scope_origin"), "got {s}");

        let mut overridden = sample_compose();
        overridden.scope_origin = ComposeScopeOrigin::Overridden;
        overridden.scope_override = true;
        let s2 = serde_json::to_string(&Query::Compose(overridden)).expect("serialize");
        assert!(s2.contains("\"scope_origin\":\"overridden\""), "got {s2}");
        let back: Query = serde_json::from_str(&s2).expect("deserialize");
        if let Query::Compose(c) = back {
            assert_eq!(c.scope_origin, ComposeScopeOrigin::Overridden);
        } else {
            panic!("expected Compose");
        }
    }
}
