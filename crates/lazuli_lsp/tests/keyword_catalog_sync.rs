//! Surface-sync WT-2 — enforcement test for the keyword-catalog ⇄ parser
//! sync. The hand-curated `KEYWORDS` catalog (`crate::keywords`) drives the
//! global completion list and the `keyword_description` hover one-liners.
//! When the parser learns a new primitive, the catalog must learn it too,
//! or authors get no completion / no hover for valid syntax.
//!
//! This test pins the exact set of primitives added in WT-2 and asserts
//! each carries a `keyword_description` hover. `keyword_description` is the
//! public surface (`pub fn`) the LSP server exposes to clients; asserting
//! against it transitively proves the keyword is in the catalog with
//! curated detail. If a future cut adds a parser primitive, append it here
//! AND to `crate::keywords::KEYWORDS` together.

use lazuli_lsp::keyword_description;

/// Every primitive Surface-sync WT-2 added to the catalog, grouped by the
/// concern documented in `crates/lazuli_lsp/src/keywords.rs`. Decorators
/// keep their `@` prefix exactly as the catalog stores them (mirrors
/// `@full_text` / `@pii`).
const WT2_KEYWORDS: &[&str] = &[
    // Domain / resource decorators + relations.
    "append_only",
    "many_through",
    "polymorphic_ref",
    "computed_date",
    "schedule_rule",
    "@slug",
    "@owner_axis",
    // Command-side verbs + approval-chain vocab.
    "reorder",
    "materialize",
    "approval",
    "chain",
    "sequential",
    // Feature kinds + iron-hand context vocabulary.
    "report",
    "poller",
    "attach_ctx",
    "purpose",
    "non_goals",
    // Surface (`.lzx`) view / create / UX primitives.
    "source",
    "submit",
    "on_success",
    "drawer",
    "sort",
    "selection",
    "bulk_actions",
    "settings",
    "persist",
    "flash",
    "replace",
    "back",
    "redirect",
    "tabs",
    "tab",
    "tab_group",
    "wizard",
    "wizard_steps",
    "view_mode",
    "inline_table",
    "date_range",
    "board",
    "lanes",
    "repeatable",
    "step",
];

#[test]
fn wt2_keywords_each_have_a_hover() {
    let mut missing = Vec::new();
    for &keyword in WT2_KEYWORDS {
        if keyword_description(keyword).is_none() {
            missing.push(keyword);
        }
    }
    assert!(
        missing.is_empty(),
        "Surface-sync WT-2 keywords missing a `keyword_description` hover: {missing:?}. \
         Add the curated one-liner in `crate::hover::{{domain,surface}}` and the entry to \
         `crate::keywords::KEYWORDS`.",
    );
}

#[test]
fn wt2_relation_hovers_carry_their_meaning() {
    // Spot-check that the curated copy actually describes the primitive
    // (not a stray placeholder) for the relation/decorator keywords whose
    // semantics the work-order pinned.
    assert!(
        keyword_description("many_through").unwrap().contains("M:N")
            || keyword_description("many_through")
                .unwrap()
                .contains("payload"),
        "`many_through` hover must describe the M:N-with-payload junction",
    );
    assert!(
        keyword_description("append_only")
            .unwrap()
            .contains("insert-only"),
        "`append_only` hover must describe the insert-only (no update/delete) contract",
    );
    assert!(
        keyword_description("materialize")
            .unwrap()
            .contains("append_only"),
        "`materialize` hover must describe routing the audit sink to an append_only resource",
    );
    assert!(
        keyword_description("schedule_rule")
            .unwrap()
            .contains("@fn."),
        "`schedule_rule` hover must describe the rule-driven (@fn) computed date",
    );
}
