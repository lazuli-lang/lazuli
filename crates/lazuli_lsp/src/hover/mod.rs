//! Keyword-level hover content for the LSP.
//!
//! Two layers, in order of preference:
//!
//! 1. **`rich_keyword_hover`** — Markdown-rendered hover for the closed
//!    catalog of "first-class" kinds (`command`, `query.list`, `policy`,
//!    `agent`, `errors`, `conventions`, ...). Each entry packages a
//!    one-line summary, required-children bullets, optional-children
//!    bullets, a worked example, and a doc anchor. This is what the IDE
//!    surfaces on hover when the word matches one of these kinds.
//!
//! 2. **`keyword_description`** — plain `&'static str` one-liner for every
//!    other DSL keyword. Used as the **fallback** when the rich layer
//!    returns `None`, and as the `detail:` field on completion items so
//!    the LLM/human sees the contract inline in the completion popup.
//!
//! ## ABI guarantee
//!
//! Both functions are re-exported from the crate root via `pub use
//! hover::*;` so external consumers (the canonical pilot VSCode extension,
//! `lazuli_cli::doctor`) keep importing them from the same path
//! (`lazuli_lsp::keyword_description`, `lazuli_lsp::rich_keyword_hover`).
//!
//! ## Sub-modules
//!
//! `keyword_description` is partitioned into three concern-shaped
//! sub-modules:
//!
//! | Sub-module | Concern |
//! |---|---|
//! | [`manifest`] | workspace / app / registry / runtime / deploy / auth / agent / notification |
//! | [`domain`] | aggregates / commands / queries / policies / jobs / pollers / i18n / encryption / security headers |
//! | [`surface`] | cookie / proxy / surface projections / `@cap.File` / report / observability / RBAC / error-vocab |
//!
//! The dispatcher below tries each in order; arms are exclusive so the
//! first-match-wins chain is deterministic.
//!
//! `rich_keyword_hover` lives in its own [`rich`] sub-module — a single
//! function with one large `match` over the rich catalog.
//!
//! ## Cross-module references
//!
//! `keyword_description` aliases 12 error-code descriptions to
//! `crate::error_vocab_code_detail` (lives in `catalogs.rs` — the
//! resolved-text catalog) so the closed-catalog error codes share the
//! same hover phrasing whether the cursor is on the code itself or on a
//! `when_denied` arm referencing it. Three `crate::LIFECYCLE_*_HOVER`
//! constants (declared `pub(crate)` in lib.rs and also consumed by
//! `lifecycle_gate_hover`) carry the routed lifecycle-state copy.

mod domain;
mod manifest;
mod rich;
mod surface;

pub use rich::rich_keyword_hover;

/// One-line hover description for a DSL keyword.
///
/// Used as the **fallback** when [`rich_keyword_hover`] returns `None`
/// and as the `detail:` field on completion items so the LLM/human
/// sees the contract inline in the completion popup. First-match-wins
/// across the three concern-shaped sub-modules
/// (`manifest`, `domain`, `surface`) — arms are exclusive.
///
/// Returns `None` for unrecognized tokens; callers should usually fall
/// through to whatever generic hover the LSP would have otherwise
/// emitted.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::keyword_description;
/// assert!(keyword_description("workspace").is_some());
/// assert!(keyword_description("definitely_not_a_keyword").is_none());
/// ```
pub fn keyword_description(keyword: &str) -> Option<&'static str> {
    manifest::keyword_description(keyword)
        .or_else(|| domain::keyword_description(keyword))
        .or_else(|| surface::keyword_description(keyword))
        // Wave H3: final fallback to the canonical `lazuli_keywords`
        // registry one-liner. The three concern-shaped modules above own
        // the *rich* curated copy (they win first-match so nothing
        // regresses); this arm backfills any keyword the registry knows
        // but the hand modules never described — so a registry-only
        // keyword still hovers with at least its one-line summary. We
        // skip the registry's empty-string hovers (values / operators
        // carry `hover: ""`) so a blank tooltip never shadows a generic
        // hover the caller would otherwise emit.
        .or_else(|| registry_hover(keyword))
}

/// One-line hover sourced from the [`lazuli_keywords`] registry, or
/// `None` when the literal is unknown or its registry `hover` is the
/// empty placeholder. Context-insensitive: returns the first row's
/// hover that is non-empty for this literal.
fn registry_hover(keyword: &str) -> Option<&'static str> {
    lazuli_keywords::ALL
        .iter()
        .find(|c| c.literal == keyword && !c.hover.is_empty())
        .map(|c| c.hover)
}
