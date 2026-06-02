//! Query IR — declarative reads (`query.list`, `query.lookup`, `query.sql`).
//!
//! A [`Query`] is the lowered shape of one of three read kinds:
//!
//! - [`ListQuery`] — `query.list <name>` returning a collection.
//! - [`LookupQuery`] — `query.lookup <name>` returning a single row.
//! - [`SqlQuery`] — `query.sql <name>` returning the declared shape from
//!   a hand-rolled SQL file.
//!
//! Cross-cutting decorators (`cache`, `policy`, `policy_when_denied`,
//! `scope`, `filters`, `order`, `paginate`, `modifier`) are shared across
//! the three kinds where they apply.
//!
//! ## Catalog
//!
//! - [`Query`] — sum type over the three read kinds.
//! - [`ListQuery`] / [`LookupQuery`] / [`SqlQuery`] — concrete shapes.
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
    /// Embedded SQL body, read from `sql_path` at module-load time by the
    /// CLI (`build_module_from_path` / `build_module_with_source_from_path`)
    /// which is the only stage that knows the on-disk feature directory the
    /// `sql_path` is relative to. Codegen emits this as
    /// `lazuli.Query.SQLText`; the runtime prefers it over reading the file
    /// at the `sql_path`, so the generated app has no runtime dependency on
    /// the `.sql` file shipping alongside the binary. `None` when the body
    /// could not be read (e.g. single-file `check` mode, or a missing file —
    /// doctor flags the latter); codegen then falls back to emitting only
    /// the `SQL:` path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql_text: Option<String>,
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
    include!("query_tests.rs");
}
