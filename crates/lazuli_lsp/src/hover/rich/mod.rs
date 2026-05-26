//! Rich Markdown hover for the closed-catalog DSL kinds the LSP knows
//! best. Each entry renders a one-line summary, required-children
//! bullets, optional-children bullets, a worked example, and a doc
//! anchor link. Markdown intentionally uses only the conservative
//! subset (headings via `**bold**`, bullet lists, fenced code blocks,
//! inline `[label](path)` links) so VS Code and Helix both render it
//! the same way; we don't use VS Code-only renderer features.
//!
//! Falls back to `keyword_description` (one-liner) when no rich
//! template exists, so adding a kind here is strictly additive and
//! cannot regress unrelated hover output.
//!
//! The canonical kinds covered today live in the sibling sub-modules:
//!
//! | Module | Keywords |
//! |---|---|
//! | [`canonical_kinds`] | `command`, `query.list`, `query.lookup`, `query.sql`, `query.view`, `api`, `policy`, `effect` |
//! | [`security`] | `audit`, `rate_limit`, `error_page` |
//! | [`error_vocab`] | `errors`, `when_denied`, `message_key` |
//! | [`conventions`] | `conventions`, `@owner_axis` / `owner_axis` |

mod canonical_kinds;
mod conventions;
mod error_vocab;
mod security;

/// Rich Markdown hover for the closed-catalog DSL kinds the LSP knows
/// best (`command`, `query.*`, `policy`, `audit`, `errors`,
/// `conventions`, ...).
///
/// Returns `None` for tokens without a rich entry; the caller falls
/// back to [`crate::keyword_description`] (one-liner). First-match-wins
/// across the four sub-modules — arms are exclusive.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::rich_keyword_hover;
/// assert!(rich_keyword_hover("definitely_not_a_kind").is_none());
/// ```
pub fn rich_keyword_hover(keyword: &str) -> Option<String> {
    canonical_kinds::rich_canonical_kind_hover(keyword)
        .or_else(|| security::rich_security_hover(keyword))
        .or_else(|| error_vocab::rich_error_vocab_hover(keyword))
        .or_else(|| conventions::rich_conventions_hover(keyword))
}
