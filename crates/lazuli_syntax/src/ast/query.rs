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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum QueryDecl {
    List(ListQueryDecl),
    Lookup(LookupQueryDecl),
    Sql(SqlQueryDecl),
}

impl QueryDecl {
    pub fn name(&self) -> &str {
        match self {
            QueryDecl::List(q) => &q.name,
            QueryDecl::Lookup(q) => &q.name,
            QueryDecl::Sql(q) => &q.name,
        }
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupKey {
    pub name: String,
    pub type_text: String,
    pub span: Span,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SqlQueryKind {
    #[default]
    Sql,
    View,
}

impl SqlQueryKind {
    pub fn is_sql(&self) -> bool {
        matches!(self, Self::Sql)
    }
}

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
