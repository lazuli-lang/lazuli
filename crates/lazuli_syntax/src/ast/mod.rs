//! Surface AST for every Lazuli source artifact.
//!
//! The AST is the **typed bridge** between the indentation-based parser
//! and the analyzer that lowers into `lazuli_ir`. Each per-domain
//! sub-file mirrors one slice of the source language:
//!
//! - `agent` — LLM agent declarations (`agent <name>`, including evals
//!   and `expose http`).
//! - `api` — explicit HTTP endpoints (`api <name>`).
//! - `auth` — feature-scoped identity contract (`auth` block).
//! - `command` — the full Phase L Tier 4b command surface + the
//!   declarative spine (`target` / `let` / `creates|updates|deletes` /
//!   `emits` / `invalidates`) reused by Job.
//! - `design` — `design.lzi` token surface.
//! - `event` — pattern-grouped event declarations (`event_group`) and
//!   per-event-variant payload arrays.
//! - `feature` — `FeatureSkeleton` container + every feature-scoped
//!   cross-cutting block (`policies`, `errors`, `defaults`,
//!   `translation`, `enum`, RBAC catalog, iron-hand context vocab,
//!   cross-feature `uses` clauses, `PolicyAtomAst` / `PolicyExprAst`).
//! - `job` — declarative + handler-backed jobs.
//! - `lzx` — `.lzx` UI shell tree (app, routes, experiences, surfaces).
//! - `mcp` — Model Context Protocol server block.
//! - `notification` — outbound notifications + realtime channels +
//!   cache profiles.
//! - `plan` — plan/gate vocabulary (PG.A side-channel map).
//! - `query` — query.list / query.lookup / query.sql + records.
//! - `report` — declarative tabular export (`report <name>`).
//! - `resource` — `resource <Name>` + invariants + lifecycle + lock +
//!   composite_key + conventions + field constraints.
//! - `surface` — modern hand-written ViewModel AST for the per-target
//!   lzx files (`features/<feat>/<feat>.{web,mobile}.lzx`).
//! - `tenant_migration` — tenant-axis schema migrations.
//! - `webhook` — inbound webhook surface (verify, replay, dlq, scope
//!   global, branch-dispatch emits).
//!
//! Everything in this file is the shared core: `Span`,
//! `RateLimitSpecAst` (used by command/query/api/agent/auth.password/
//! notification/report), the legacy brace-MVP `Document` /
//! `Aggregate` / `Field` / `FieldModifier` / `Command` / `Query` /
//! `Surface` types kept for back-compat with the pre-canonical-indent
//! parser.

use serde::{Deserialize, Serialize};

mod agent;
mod api;
mod auth;
mod command;
mod design;
mod event;
mod feature;
mod job;
mod lzx;
mod mcp;
mod notification;
mod plan;
mod query;
mod report;
mod resource;
mod surface;
mod tenant_migration;
mod webhook;
pub use agent::*;
pub use api::*;
pub use auth::*;
pub use command::*;
pub use design::*;
pub use event::*;
pub use feature::*;
pub use job::*;
pub use lzx::*;
pub use mcp::*;
pub use notification::*;
pub use plan::*;
pub use query::*;
pub use report::*;
pub use resource::*;
pub use surface::*;
pub use tenant_migration::*;
pub use webhook::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// `ir-rate-limit-env-aware` cell 1 — AST analog of `ir::RateLimitSpec`.
///
/// Aggregates one optional unqualified default + any number of
/// env-qualified override entries. The parser builds this from
/// consecutive `rate_limit "..."` / `rate_limit "..." in <envs>` lines
/// (proposal §4.2). The single-line source shape produces a spec with
/// `default = Some("X")` and `by_env = []`; multi-line shapes populate
/// `by_env` in source order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitSpecAst {
    /// Unqualified `rate_limit "X"` line — at most one per declaration.
    /// `None` when the source authored only env-qualified lines (Cell 3
    /// doctor emits `rate_limit_no_default_with_qualifications`).
    pub default: Option<String>,
    /// `rate_limit "X" in <env_list>` entries in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub by_env: Vec<RateLimitByEnvAst>,
    pub span: Span,
}

/// AST analog of `ir::RateLimitByEnv`. Envs are kept as raw identifiers
/// here; the analyzer normalises them into the closed `EnvName` catalog
/// at lowering time (`crates/lazuli_analyzer/src/lib.rs`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitByEnvAst {
    /// Limit string verbatim (quotes stripped). The proposal-defined
    /// keyword `"unlimited"` (§4.4) is preserved as-is; the analyzer
    /// lowers it to the empty-string sentinel in `RateLimitByEnv.limit`.
    pub limit: String,
    /// Raw env identifiers as written. Non-empty by construction — the
    /// parser rejects `rate_limit "X" in` (empty tail).
    pub envs: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub app: Option<String>,
    pub aggregates: Vec<Aggregate>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aggregate {
    pub name: String,
    pub fields: Vec<Field>,
    pub commands: Vec<Command>,
    pub queries: Vec<Query>,
    pub surfaces: Vec<Surface>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub ty: String,
    pub modifiers: Vec<FieldModifier>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum FieldModifier {
    Required,
    Unique,
    Default(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub name: String,
    pub input: Vec<String>,
    pub policy: Option<String>,
    pub emits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub name: String,
    pub search: Vec<String>,
    pub filters: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    pub name: String,
    pub list_columns: Vec<String>,
    pub form_fields: Vec<String>,
    pub detail_fields: Vec<String>,
    pub span: Span,
}
