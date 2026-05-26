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

/// Three-arm catalog of query shapes — `query.list` / `query.lookup` /
/// `query.sql`. See module-level docs for the surface of each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum QueryDecl {
    /// `query.list <name>` — paginated/searchable list (collection result).
    List(ListQueryDecl),
    /// `query.lookup <name>` — singular fetch by typed key(s).
    Lookup(LookupQueryDecl),
    /// `query.sql <name>` — SQL-backed query with `returns <Type>`.
    Sql(SqlQueryDecl),
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
