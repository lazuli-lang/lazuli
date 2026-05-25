//! `.lzi` parser-owned AST types — the `Lifecycle*` and `Poller*` family.
//!
//! These structs exist in the parser layer (rather than `crate::ast`) because
//! they were introduced incrementally with their feature primitives
//! (`lifecycle` blocks on resources, top-level `poller` declarations) and the
//! workspace re-exports them through `lazuli_syntax::*` for downstream
//! consumers (analyzer, doctor, LSP, codegen).
//!
//! ## Why a separate module
//!
//! The structs are intentionally co-located with the parsers that produce
//! them, but the type definitions stay free of parsing logic so the analyzer
//! can match against them without pulling in the entire parser surface.
//!
//! ## Lifecycle family
//!
//! Resources carry an optional `lifecycle <discriminator>` child. The block
//! enumerates states, transitions (with `from`/`to`/`policy`/`audit`/
//! `emits` slots), and invariants. Closed-catalog parsing — every keyword
//! outside the spec rejects.
//!
//! ## Poller family (proposals/poller-vocab.md §3.1)
//!
//! Top-level kind parallel to `job` / `webhook` / `notification`. The poller
//! drives an external source through a cursor, retry policy, terminal status
//! fields, and `retry_quirks` (closed predicate catalog — `gender_flip_once`
//! is the only v0.1 form).
//!
//! ## See also
//!
//! - `lazuli_ir::nodes::lifecycle` — the typed IR shape the analyzer lowers
//!   `LifecycleBlockAst` into.
//! - `lazuli_ir::nodes::poller` — same, for `PollerBlockAst`.
//! - `docs/canonical-semantics.md` — prose reference for both vocabularies.

use crate::ast::Span;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleBlockAst {
    pub discriminator_field: String,
    pub states: Vec<LifecycleStateAst>,
    pub transitions: Vec<LifecycleTransitionAst>,
    pub invariants: Vec<LifecycleInvariantAst>,
    pub invariant_handlers: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleStateAst {
    pub name: String,
    pub kind_keyword: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleTransitionAst {
    pub name: String,
    pub from: Vec<String>,
    pub to: String,
    pub policy: Option<String>,
    pub audit: Option<String>,
    pub timestamps: Option<String>,
    pub emits: Vec<String>,
    pub requires: Option<String>,
    pub tests: Vec<String>,
    pub previously: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleInvariantAst {
    /// Raw tail after `invariant `; lowering tokenizes the closed catalog.
    pub raw: String,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// L0 #8 — `poller` vocabulary (docs/proposals/poller-vocab.md).
// Top-level feature kind, parallel to `job` / `webhook` / `notification`.
// AST is closed-catalog: only the children listed in §3.1 of the proposal
// are accepted; any other keyword is a parse error.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerBlockAst {
    pub name: String,
    pub source: String,
    pub cursor: Option<PollerCursorAst>,
    pub retry: Option<PollerRetryAst>,
    pub states: Vec<PollerStateAst>,
    pub resolve_handler: Option<String>,
    pub terminal_status_field: Option<String>,
    pub terminal_result_field: Option<String>,
    pub tick: Option<PollerTickAst>,
    pub tenant_from: Option<String>,
    pub idempotency: Vec<String>,
    pub audit: Option<String>,
    pub emits: Vec<String>,
    pub retry_quirks: Vec<PollerRetryQuirkAst>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerCursorAst {
    pub next_at_field: String,
    pub resolved_at_field: String,
    pub attempts_field: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerRetryAst {
    pub max_attempts: u32,
    pub backoff_strategy: String,
    pub backoff_base: Option<String>,
    pub backoff_cap: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerStateAst {
    pub name: String,
    pub kind_keyword: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerTickAst {
    pub every: String,
    pub batch: Option<u32>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerRetryQuirkAst {
    /// Catalog form name (`gender_flip_once` in v0.1).
    pub kind: String,
    /// Raw predicate after `when ` — closed predicate language;
    /// analyzer cross-checks.
    pub when: String,
    /// Counter field on `source`.
    pub counter_field: String,
    /// `mutate <field> = <transform>` raw rhs.
    pub mutate_field: String,
    pub mutate_transform: String,
    pub span: Span,
}
