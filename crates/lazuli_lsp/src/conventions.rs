//! Closed-catalog completion provider for `conventions [..]` lists on
//! resources.
//!
//! Surfaces the `crud` / `me` closed catalog (and future entries) when
//! the cursor sits inside an unclosed `[` after a `conventions` keyword
//! on the same line. Multi-line conventions are not a supported
//! authoring shape, so this provider stays single-line by design.
//!
//! ## ABI guarantee
//!
//! `conventions_list_completions` is re-exported from the crate root
//! via `pub use conventions::*;` so external consumers keep importing
//! it from the same path (`lazuli_lsp::conventions_list_completions`).
//!
//! ## Why not unified with `rate_limit.rs`?
//!
//! Both `rate_limit.rs` and this module are single-token bracket /
//! quoted-list completion providers. They share no helpers and have
//! disjoint vocabularies; keeping them in sibling files mirrors the
//! "one concern per file" axis of the rails-style refactor. A future
//! `completions/` parent module can roll them up if more single-token
//! providers land.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};

/// Closed-catalog completion inside `conventions [..]`. Fires when the
/// line prefix matches `<indent>conventions [<partial-or-empty>` with
/// no closing `]` before the cursor. Returns the closed catalog
/// (`crud`, `me`) plus future entries as they land. Returns `None`
/// outside the bracket-list so falls through to the generic dispatcher.
///
/// `docs/proposals/ir-resource-conventions-crud.md` §4.4 + §12 row C4
/// and `docs/proposals/ir-resource-conventions-me.md` §4.4 + §12 row M3.
pub fn conventions_list_completions(before: &str) -> Option<Vec<CompletionItem>> {
    // Locate the most recent unclosed `[` after a `conventions` keyword
    // on the same line. The slot is single-line per the parser spec
    // (§4.1); multi-line `conventions` would not register here, and
    // that's the intended behavior — multi-line is not a supported
    // authoring shape.
    let conv_idx = before.rfind("conventions")?;
    let after_kw = &before[conv_idx + "conventions".len()..];
    let open_idx = after_kw.find('[')?;
    let inside = &after_kw[open_idx + 1..];
    if inside.contains(']') {
        // Cursor sits past the closing bracket — not inside the list.
        return None;
    }

    let catalog: &[(&str, &str)] = &[
        (
            "crud",
            "Auto-synthesizes 5 CRUD entries (`create_<r>`, `update_<r>`, `delete_<r>`, `lookup_<r>`, `list_<r>s`). See `docs/proposals/ir-resource-conventions-crud.md` §5.",
        ),
        (
            "me",
            "Auto-synthesizes one `lookup_my_<r>` query keyed by the active actor (`ctx.User.ID` / `ctx.User.OrgID`). See `docs/proposals/ir-resource-conventions-me.md` §5.",
        ),
    ];

    Some(
        catalog
            .iter()
            .map(|(label, detail)| CompletionItem {
                label: (*label).to_owned(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some((*detail).to_owned()),
                ..CompletionItem::default()
            })
            .collect(),
    )
}
