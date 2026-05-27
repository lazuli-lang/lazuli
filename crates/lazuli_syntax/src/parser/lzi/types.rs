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

/// `lifecycle <discriminator>` child block on a `ResourceDecl`.
///
/// Owns the state set, the transition graph, and any cross-cutting
/// invariants for the lifecycle machine. Lowered to
/// `ir::nodes::lifecycle::Lifecycle`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleBlockAst {
    /// Resource field name that stores the lifecycle state.
    pub discriminator_field: String,
    pub states: Vec<LifecycleStateAst>,
    pub transitions: Vec<LifecycleTransitionAst>,
    pub invariants: Vec<LifecycleInvariantAst>,
    /// Verbatim `invariant_handler @fn.<name>` references.
    pub invariant_handlers: Vec<String>,
    pub span: Span,
}

/// One `state <name>` row inside a [`LifecycleBlockAst`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleStateAst {
    pub name: String,
    /// Closed-catalog kind keyword (`initial` / `terminal` / ...), if authored.
    pub kind_keyword: Option<String>,
    pub span: Span,
}

/// One `transition <name>` block inside a [`LifecycleBlockAst`].
///
/// Carries the source states (`from`), target state (`to`), policy
/// atom, audit hook, timestamps clause, emitted events, requires
/// predicate, inline tests, and `previously` rename references.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleTransitionAst {
    pub name: String,
    /// `from <state>, <state>` source set.
    pub from: Vec<String>,
    /// `to <state>` target state.
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

/// One `invariant ...` row inside a [`LifecycleBlockAst`]. Parsed at
/// parse time against the closed catalog (`docs/proposals/lifecycle-vocab.md`
/// §3.4) so unknown forms reject at the parser boundary — never silently
/// coerced downstream.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LifecycleInvariantAst {
    /// Typed closed-catalog form; the parser rejects everything outside
    /// the catalog before this AST is constructed.
    pub form: LifecycleInvariantForm,
    pub span: Span,
}

/// Closed catalog of lifecycle invariant forms (`docs/proposals/lifecycle-vocab.md`
/// §3.4). The variants mirror `ir::nodes::lifecycle::LifecycleInvariant`
/// 1:1 so the analyzer can map without inspecting raw text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum LifecycleInvariantForm {
    /// `invariant terminal_immutable`
    TerminalImmutable,
    /// `invariant single <state> per <scope_field>`
    SingleStatePerScope {
        state: String,
        scope_field: String,
    },
    /// `invariant no_jump_more_than_one`
    NoJumpMoreThanOne,
}

// ---------------------------------------------------------------------------
// L0 #8 — `poller` vocabulary (docs/proposals/poller-vocab.md).
// Top-level feature kind, parallel to `job` / `webhook` / `notification`.
// AST is closed-catalog: only the children listed in §3.1 of the proposal
// are accepted; any other keyword is a parse error.
// ---------------------------------------------------------------------------

/// `poller <name>` block — top-level feature kind parallel to `job` /
/// `webhook` / `notification`. Drives an external source through a
/// cursor + retry policy + terminal status fields. See module-level docs
/// and `docs/proposals/poller-vocab.md` §3.1.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerBlockAst {
    pub name: String,
    /// `source <resource_ref>` — entity being polled.
    pub source: String,
    pub cursor: Option<PollerCursorAst>,
    pub retry: Option<PollerRetryAst>,
    pub states: Vec<PollerStateAst>,
    /// `resolve_handler @fn.<name>` — adapter that resolves cursor ticks.
    pub resolve_handler: Option<String>,
    /// Terminal status field name on `source`.
    pub terminal_status_field: Option<String>,
    /// Terminal result field name on `source`.
    pub terminal_result_field: Option<String>,
    pub tick: Option<PollerTickAst>,
    pub tenant_from: Option<String>,
    pub idempotency: Vec<String>,
    pub audit: Option<String>,
    pub emits: Vec<String>,
    pub retry_quirks: Vec<PollerRetryQuirkAst>,
    pub span: Span,
}

/// `cursor` sub-block on a [`PollerBlockAst`] — names the source fields
/// that store next-poll-at, resolved-at, and attempts counter.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerCursorAst {
    pub next_at_field: String,
    pub resolved_at_field: String,
    pub attempts_field: String,
    pub span: Span,
}

/// `retry` sub-block on a [`PollerBlockAst`] — max attempts + backoff
/// strategy (with optional base/cap overrides).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerRetryAst {
    pub max_attempts: u32,
    /// Closed strategy catalog (`fixed` / `exponential`).
    pub backoff_strategy: String,
    /// `base "<duration>"` — duration literal verbatim, if authored.
    pub backoff_base: Option<String>,
    /// `cap "<duration>"` — duration literal verbatim, if authored.
    pub backoff_cap: Option<String>,
    pub span: Span,
}

/// One `state <name>` row inside a [`PollerBlockAst`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerStateAst {
    pub name: String,
    /// Closed-catalog kind keyword (`initial` / `terminal` / ...), if authored.
    pub kind_keyword: Option<String>,
    pub span: Span,
}

/// `tick` sub-block on a [`PollerBlockAst`] — schedule cadence + batch size.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PollerTickAst {
    /// `every "<duration>"` literal verbatim.
    pub every: String,
    /// `batch <N>` — per-tick fetch limit.
    pub batch: Option<u32>,
    pub span: Span,
}

/// One `retry_quirks <kind>` row inside a [`PollerBlockAst`].
///
/// `kind` is the closed catalog form name (only `gender_flip_once` in
/// v0.1); `when` is the raw predicate text the analyzer cross-checks;
/// `mutate_field` / `mutate_transform` describe the side-effect on the
/// counter / source field.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_state_kind_keyword_optional() {
        let s = LifecycleStateAst {
            name: "draft".into(),
            kind_keyword: None,
            span: Span::new(0, 0),
        };
        assert!(s.kind_keyword.is_none());
    }

    #[test]
    fn poller_tick_batch_optional() {
        let t = PollerTickAst {
            every: "30s".into(),
            batch: Some(50),
            span: Span::new(0, 0),
        };
        assert_eq!(t.batch, Some(50));
    }
}
