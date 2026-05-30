//! Query + Record AST surfaces (Phase L Tier 4d).
//!
//! Three query shapes sharing the `query.` namespace, picked by the
//! `kind` keyword the author writes after `query.`:
//!
//! - `query.list <name>` — paginated/searchable list, returns a
//!   collection. Carries `params`, `filters`, `search`, `cache`,
//!   `paginate`, `order`.
//! - `query.lookup <name>` — singular fetch by typed key(s) (`by id:
//!   ID` / `by slug: Text`). May carry `filters` so a ctx-keyed lookup
//!   (e.g. `my_host` filtered by `user_id = ctx.actor.user_id`)
//!   round-trips through the runtime's RunLookup mechanism.
//! - `query.sql <name>` — SQL-backed query with explicit `sql "./..."`
//!   reference and `returns <Type>`. The `kind` slot also closes the
//!   `view` shape for materialized views.
//!
//! `RecordDecl` is the simple typed-field bag (no constraints, no
//! tenancy) that lives under `domain` alongside resources and queries.
//! Cut A.6 used records with a `discriminator` field for tagged-union
//! agent outputs.
//!
//! All three queries + record opt into `public contract <name> as v<N>`
//! per cross-feature-contracts §3.5 + §5.3.

use serde::{Deserialize, Serialize};

use super::{CommandInputSlot, PolicyExprAst, PublicContractDeclAst, ResourceFieldDecl, Span};

/// Four-arm catalog of query shapes — `query.list` / `query.lookup` /
/// `query.sql` / `query.compose`. See module-level docs for the surface
/// of each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum QueryDecl {
    /// `query.list <name>` — paginated/searchable list (collection result).
    List(ListQueryDecl),
    /// `query.lookup <name>` — singular fetch by typed key(s).
    Lookup(LookupQueryDecl),
    /// `query.sql <name>` — SQL-backed query with `returns <Type>`.
    Sql(SqlQueryDecl),
    /// `query.compose <name>` — declarative composite read (root resource
    /// + FK-path JOIN projection + closed sub-select catalog). See
    /// [`ComposeQueryDecl`] and
    /// `docs/proposals/ir-composite-read-primitive-2026-05-29.md`.
    Compose(ComposeQueryDecl),
}

impl QueryDecl {
    /// The query identifier, irrespective of variant.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::{QueryDecl, LookupQueryDecl, Span};
    ///
    /// let q = QueryDecl::Lookup(LookupQueryDecl {
    ///     name: "by_id".into(),
    ///     public_contract: None,
    ///     policy: None,
    ///     policy_expr: None,
    ///     keys: vec![],
    ///     filters: vec![],
    ///     span: Span::new(0, 0),
    /// });
    /// assert_eq!(q.name(), "by_id");
    /// ```
    pub fn name(&self) -> &str {
        match self {
            QueryDecl::List(q) => &q.name,
            QueryDecl::Lookup(q) => &q.name,
            QueryDecl::Sql(q) => &q.name,
            QueryDecl::Compose(q) => &q.name,
        }
    }
}

/// `query.list <name>` — paginated/searchable list query.
///
/// Carries the full surface for collection queries: params, filters,
/// search, cache, paginate, order. Mutually-exclusive cache forms
/// (inline block vs profile ref) are rejected at parse time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListQueryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// RB.S6 — structured form of `policy <expr>` when predicates
    /// (`has_role` / `has_permission` / `authenticated`) are present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `modifier @query_modifier.<name>` reference.
    pub modifier: Option<String>,
    /// `params` block (typed slots).
    pub params: Vec<CommandInputSlot>,
    /// `scope override` flag — when set, the query opts out of feature
    /// default tenancy.
    pub scope_override: bool,
    /// `scope override\n  reason "..."` text.
    pub scope_reason: Option<String>,
    /// `scope override\n  deleted_at = nil` raw assignments captured
    /// for cross-check; not yet lowered to typed predicate.
    pub scope_assignments: Vec<String>,
    /// `scope` block (without `override`) — verbatim lines for now;
    /// the legacy lowering produces typed predicates.
    pub scope_lines: Vec<String>,
    /// `filters` block lines (`field when params.field`).
    pub filters: Vec<String>,
    /// `search params.<key> over <fields>` line with optional `mode contains`.
    pub search: Option<QuerySearch>,
    /// `cache` block — verbatim lines (inline shape).
    pub cache: Vec<String>,
    /// Cache bucket cycle (CL.C.3) — `cache <profile_name>` reference
    /// form. Single-line shape pointing at a feature-level `cache
    /// <name>` profile. Mutually exclusive with the inline `cache`
    /// block at parse time; the parser rejects the combination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_profile_ref: Option<String>,
    /// `paginate <N>` page size.
    pub paginate: Option<u32>,
    /// `order <field> <asc|desc>` declarations.
    pub order: Vec<String>,
    pub span: Span,
}

/// `query.lookup <name>` — singular fetch by typed key(s).
///
/// May carry `filters` so a ctx-keyed lookup (e.g. `my_host` filtered
/// by `user_id = ctx.actor.user_id`) round-trips through the runtime's
/// RunLookup mechanism.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupQueryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `by <field>: <Type>` keys. Authored on the same line as the
    /// header in the fixture (`query.lookup by_id by id: ID`).
    pub keys: Vec<LookupKey>,
    /// `filters` block — verbatim lines (`field = ctx.actor.X` form),
    /// same shape as ListQueryDecl.filters. Lowered into
    /// `ir::LookupQuery.filters` by the analyzer; codegen merges them
    /// with `keys` into `LookupBy` so a ctx-keyed lookup (e.g.
    /// `my_host` filtered by `user_id = ctx.actor.user_id`) round-trips
    /// through the runtime's RunLookup mechanism.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<String>,
    pub span: Span,
}

/// One `<name>: <Type>` key inside a [`LookupQueryDecl`]'s `by` clause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupKey {
    pub name: String,
    /// Type literal verbatim (`ID`, `Text`, ...).
    pub type_text: String,
    pub span: Span,
}

/// `query.sql <name>` — SQL-backed query with explicit `returns <Type>`
/// and a `sql "./..."` path / `source @file.<name>.sql` reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SqlQueryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "SqlQueryKind::is_sql")]
    pub kind: SqlQueryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `params` block.
    pub params: Vec<CommandInputSlot>,
    /// `scope` block — verbatim lines.
    pub scope_lines: Vec<String>,
    /// `returns <Type>` declaration (required for SQL-backed queries).
    pub returns: String,
    /// `sql "./queries/<name>.sql"` path literal or `source @file.<name>.sql`.
    pub sql_path: String,
    pub span: Span,
}

/// Closed two-arm catalog distinguishing `query.sql` (one-shot) from
/// `view` (materialized) on a [`SqlQueryDecl`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SqlQueryKind {
    /// `query.sql <name>` — request-time SQL execution.
    #[default]
    Sql,
    /// `view <name>` — materialised view backed by SQL.
    View,
}

impl SqlQueryKind {
    /// `true` when this kind is the default `Sql` arm.
    ///
    /// Used as the serde `skip_serializing_if` guard so authored
    /// `query.sql` blocks don't pay an extra `kind` JSON field.
    ///
    /// ## Examples
    ///
    /// ```
    /// use lazuli_syntax::SqlQueryKind;
    ///
    /// assert!(SqlQueryKind::Sql.is_sql());
    /// assert!(!SqlQueryKind::View.is_sql());
    /// ```
    pub fn is_sql(&self) -> bool {
        matches!(self, Self::Sql)
    }
}

/// `query.compose <name>` — declarative composite read AST.
///
/// Rooted at exactly one resource (`from <Resource>`), projecting columns
/// from itself and FK-reachable neighbors (`join <fk.path>` + `select`),
/// plus a **closed 4-member** per-row sub-select catalog (`count` /
/// `exists` / `latest` / `aggregate`). Inherits tenant/soft-delete scope
/// like `query.list`; lowers (W2+) to one analyzable SELECT. See
/// `docs/proposals/ir-composite-read-primitive-2026-05-29.md` §3.
///
/// Parser-enforced invariants (§3.1/§3.2): exactly one `from` root;
/// JOINs are FK paths only; sub-select predicates use the closed
/// language (`=`/`!=`/`has`/`AND`/`OR`) plus `in` with a **literal-set
/// RHS only** — `in (subselect)`, `in params.x`, `in <expr>` are not
/// productions and are rejected at parse time. `key` presence makes the
/// read single-row (`not_found` on zero rows, mirroring `query.lookup`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeQueryDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    /// `policy @policy.<name>`.
    pub policy: Option<String>,
    /// RB.S6 — structured policy expression form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_expr: Option<PolicyExprAst>,
    /// `from <Resource>` — the single root resource (required, exactly 1).
    pub root: String,
    /// `params` block (typed slots).
    pub params: Vec<CommandInputSlot>,
    /// `join <fk.path> [as <alias>] [optional]` declarations (0..N).
    pub joins: Vec<ComposeJoinDecl>,
    /// `select` projections (required, ≥1).
    pub projections: Vec<ComposeProjectionDecl>,
    /// `subselect <name> = <kind>` declarations (0..N) — closed catalog.
    pub subselects: Vec<ComposeSubselectDecl>,
    /// `filters` block lines (verbatim; lowered by the analyzer).
    pub filters: Vec<String>,
    /// `key <path> = <expr>` clause. Presence ⇒ single-row ⇒ `not_found`
    /// on zero rows (§3.2 #6). `None` ⇒ list semantics.
    pub key: Option<String>,
    /// `scope` block (without `override`) — verbatim lines.
    pub scope_lines: Vec<String>,
    /// `scope override` flag — when set, opts out of feature default tenancy.
    pub scope_override: bool,
    /// `scope override\n  reason "..."` text.
    pub scope_reason: Option<String>,
    /// `scope override` raw assignments captured for cross-check.
    pub scope_assignments: Vec<String>,
    /// `order <field> <asc|desc>` declarations.
    pub order: Vec<String>,
    /// `paginate <N>` page size.
    pub paginate: Option<u32>,
    /// `returns <Type>` generated record name; defaults to `<Compose>Row`
    /// at lowering when omitted.
    pub returns: Option<String>,
    pub span: Span,
}

/// One `join <fk.path> [as <alias>] [optional]` inside a
/// [`ComposeQueryDecl`]. The path resolves (W2+) against the IR relation
/// graph; never an `ON`-clause string. `optional` ⇒ LEFT JOIN (nullable);
/// default ⇒ INNER.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeJoinDecl {
    /// FK path, e.g. `host.user`. Dotted segments, IDENT_LOWER.
    pub path: String,
    /// `as <alias>` — names the joined node for select/subselect refs.
    /// `None` defaults (W2) to the last path segment.
    pub alias: Option<String>,
    /// `optional` ⇒ LEFT JOIN (nullable). Default ⇒ INNER.
    pub nullable: bool,
    pub span: Span,
}

/// One `<name> = <source>` projection inside a [`ComposeQueryDecl`]'s
/// `select` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeProjectionDecl {
    /// Output field name on the generated record.
    pub name: String,
    /// The projection source — `self.<col>`, `<alias>.<col>`, or a
    /// declared subselect name.
    pub source: ComposeProjectionSourceDecl,
    pub span: Span,
}

/// Where a [`ComposeProjectionDecl`] reads from. Closed three-arm form
/// per §3.1 `projection_source`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ComposeProjectionSourceDecl {
    /// `self.<column>` — a column on the root resource.
    SelfColumn(String),
    /// `<alias>.<column>` — a column on a joined node.
    Joined { alias: String, column: String },
    /// A bare identifier referencing a declared `subselect`.
    Subselect(String),
}

/// One `subselect <name> = <kind>` declaration — the **closed 4-member**
/// per-row sub-select catalog (`count`/`exists`/`latest`/`aggregate`).
/// Adding a kind is a spec edit, not author freedom (§3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeSubselectDecl {
    /// The declared subselect name (referenced from `select`/`filters`).
    pub name: String,
    /// Closed-catalog kind + its target resource.
    pub kind: ComposeSubselectKindDecl,
    /// `related_by <fk.path>` — how the child joins to root (required by
    /// the analyzer; carried as `Option` so the parser can surface a
    /// targeted error rather than a structural one).
    pub related_by: Option<String>,
    /// `where <closed-predicate>` — scalar-literal predicate (and the
    /// literal-set `in [...]` form). Parsed, narrowed.
    pub where_pred: Vec<ComposeSubselectPred>,
    /// `filter <closed-predicate>` — SQL `FILTER (WHERE ...)` on an
    /// aggregate. Same narrowed predicate language as `where`.
    pub filter_pred: Vec<ComposeSubselectPred>,
    /// `order <field> <asc|desc>` — for `latest`.
    pub order: Vec<String>,
    /// `negate` — NOT EXISTS (anti-join); only meaningful for `exists`.
    pub negate: bool,
    pub span: Span,
}

/// The closed 4-member sub-select kind catalog (§3.1 `subselect_kind`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ComposeSubselectKindDecl {
    /// `count <Resource>`.
    Count { resource: String },
    /// `exists <Resource>`.
    Exists { resource: String },
    /// `latest <column> of <Resource>`.
    Latest { column: String, resource: String },
    /// `aggregate <fn> <column> of <Resource>`.
    Aggregate {
        func: ComposeAggFnDecl,
        column: String,
        resource: String,
    },
}

/// The closed 5-member aggregate-function catalog (§3.1 `agg_fn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComposeAggFnDecl {
    Sum,
    Avg,
    Min,
    Max,
    CountDistinct,
}

/// One predicate inside a `subselect` `where`/`filter` (§3.1
/// `subselect_pred`). The closed operator set is `=`/`!=`/`has` plus the
/// `in [literal,...]` literal-set form. `AND`/`OR` between predicates is
/// carried by the surrounding `Vec` + [`ComposeSubselectPred::combinator`].
/// The parser REJECTS `in (subselect)`, `in params.x`, and `in <expr>`
/// before constructing this — only a literal-set RHS produces an `In`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeSubselectPred {
    /// Boolean combinator joining this predicate to the PREVIOUS one.
    /// `None` on the first predicate of the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combinator: Option<ComposePredCombinator>,
    /// Left-hand scalar reference (`self.x` / `<alias>.x` / `ctx.x...`).
    pub left: String,
    /// The operator + right-hand side.
    pub op: ComposeSubselectPredOp,
    pub span: Span,
}

/// The `AND` / `OR` combinator joining two [`ComposeSubselectPred`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ComposePredCombinator {
    And,
    Or,
}

/// Operator + RHS for a [`ComposeSubselectPred`]. The `In` arm is the
/// ONLY set-membership form and carries a literal-set RHS exclusively —
/// the correlated-subquery / dynamic-set backdoor (`in (subselect)`,
/// `in params.x`, `in <expr>`) is rejected at parse time (§3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum ComposeSubselectPredOp {
    /// `<left> = <rhs>`.
    Eq(String),
    /// `<left> != <rhs>`.
    Ne(String),
    /// `<left> has <rhs>` — collection contains element.
    Has(String),
    /// `<left> in [<literal>, ...]` — literal-set membership ONLY.
    In(Vec<String>),
}

/// `search params.<key> over <fields>` clause inside a [`ListQueryDecl`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuerySearch {
    /// `params.search` source path.
    pub source: String,
    /// `over name, email` list.
    pub fields: Vec<String>,
    /// `mode contains` (closed catalog — `contains` only today).
    pub mode: Option<String>,
    pub span: Span,
}

/// `record <Name>` declaration — a typed value-record (no tenancy, no
/// constraints). Lives inside `domain` alongside resources and queries.
/// Cut A.6 used records with a `discriminator` field for tagged-union
/// agent outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_contract: Option<PublicContractDeclAst>,
    pub fields: Vec<ResourceFieldDecl>,
    /// `discriminator` field marker name when authored. Cut A.6 used
    /// `record` types with a discriminator field for tagged-union
    /// agent outputs.
    pub discriminator_field: Option<String>,
    pub span: Span,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_query_kind_default_is_sql() {
        assert_eq!(SqlQueryKind::default(), SqlQueryKind::Sql);
        assert!(SqlQueryKind::default().is_sql());
    }

    #[test]
    fn sql_query_kind_view_serde_snake_case() {
        let v = serde_json::to_value(SqlQueryKind::View).unwrap();
        assert_eq!(v, serde_json::json!("view"));
    }

    #[test]
    fn query_decl_name_dispatches_per_variant() {
        let q = QueryDecl::Sql(SqlQueryDecl {
            name: "monthly_totals".into(),
            kind: SqlQueryKind::Sql,
            public_contract: None,
            policy: None,
            policy_expr: None,
            params: vec![],
            scope_lines: vec![],
            returns: "MonthlyTotal".into(),
            sql_path: "./queries/totals.sql".into(),
            span: Span::new(0, 0),
        });
        assert_eq!(q.name(), "monthly_totals");
    }
}
