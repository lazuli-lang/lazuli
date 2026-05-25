//! Per-section (app / registry / view / surface / command-statement /
//! query-statement / audience) closed-kind catalogs and the typo
//! diagnostics that ride on them.
//!
//! 2026-05-15 — Typo-detection asymmetry sweep (R1.C audit follow-up).
//! `feature_unknown_kind_diagnostics` had ONE context: indent-2 lines
//! inside `feature X`. The sweep surfaced 7 other contexts where a
//! typo in a kind keyword silently breaks compilation (parser drops
//! the block, IR loses the declaration, regenerated `dist/` looks
//! like the user simply forgot to write the command/api/view).
//!
//! Each diagnostic shares the same skeleton: detect "inside the
//! context", at the appropriate sub-indent check the first token
//! against a closed catalog, skip decorator/field/assignment lines,
//! and emit ERROR with a `closest_kind` suggestion (Damerau-
//! Levenshtein ≤ 2).
//!
//! Two sub-modules carry the per-context state:
//! - `blocks`: indent-2 children of top-level container blocks
//!   (`app`, `registry`, `view`, `surface`).
//! - `statements`: indent-4 statements inside `command` / `query.*`,
//!   and the small audience-children set.
//!
//! All catalogs are sorted alphabetically for diff hygiene; keep new
//! entries in order.

mod blocks;
mod statements;

pub(crate) use blocks::{
    APP_BODY_KINDS, REGISTRY_BODY_KINDS, SURFACE_BODY_KINDS, VIEW_BODY_KINDS,
    app_unknown_kind_diagnostics, registry_unknown_kind_diagnostics,
    surface_unknown_kind_diagnostics, view_unknown_kind_diagnostics,
};
pub(crate) use statements::{
    AUDIENCE_BODY_KINDS, COMMAND_STATEMENT_KINDS, QUERY_STATEMENT_KINDS,
    audience_unknown_kind_diagnostics, command_statement_unknown_diagnostics,
    query_statement_unknown_diagnostics,
};
