//! Wave H3 — assert the LSP keyword catalogs DERIVE from the canonical
//! `lazuli_keywords` registry.
//!
//! The LSP ships several hand-maintained keyword tables:
//!
//! * [`KEYWORDS`] — the global `.lzi` completion vocabulary.
//! * [`DESIGN_KEYWORDS`] — the `*.design.lzi` completion vocabulary.
//! * the typo-diagnostic catalogs `FEATURE_BODY_KINDS`, `APP_BODY_KINDS`,
//!   `REGISTRY_BODY_KINDS`, `VIEW_BODY_KINDS`, `SURFACE_BODY_KINDS`,
//!   `COMMAND_STATEMENT_KINDS`, `QUERY_STATEMENT_KINDS`,
//!   `AUDIENCE_BODY_KINDS` (re-exported `pub(crate)` from
//!   `diagnostics::canonical_kinds`).
//!
//! `lazuli_keywords::ALL` is the single source of truth (proven complete
//! against the parser by Wave H1). These tests make every LSP table a
//! *derivation* of that registry by construction:
//!
//! 1. **Completeness (LSP ⊆ registry).** Every entry the LSP offers must
//!    be a real registry literal — the LSP can never invent a word the
//!    parser doesn't know. A tiny documented allowlist carries the few
//!    LSP-only completion conveniences that are not standalone registry
//!    literals (bare aliases of dotted kinds / AST-field shorthands).
//!
//! 2. **Coverage (registry ⊆ LSP), per block context.** Every registry
//!    literal valid in a block context must appear in that block's typo
//!    catalog, so a parser keyword can never be silently absent from the
//!    "did you mean" surface. One-directional (not equality) because the
//!    curated lists deliberately tolerate cross-context inclusions the
//!    Damerau-Levenshtein detector needs (e.g. `policy` is valid in
//!    command / query / view bodies).
//!
//! The typo catalogs are NOT replaced with generated consts: they feed
//! the typo diagnostics + the `typo_snapshot` gate and deliberately mix
//! contexts. Asserting the derivation relationship surfaces drift as a
//! failure without coupling the diagnostic surface to a single-context
//! projection.

use std::collections::BTreeSet;

use lazuli_keywords::{ALL, Context, Sigil};

use super::{DESIGN_KEYWORDS, KEYWORDS};
use crate::{
    APP_BODY_KINDS, AUDIENCE_BODY_KINDS, COMMAND_STATEMENT_KINDS, FEATURE_BODY_KINDS,
    QUERY_STATEMENT_KINDS, REGISTRY_BODY_KINDS, SESSIONS_BODY_KINDS, SESSIONS_COOKIE_BODY_KINDS,
    SURFACE_BODY_KINDS, VIEW_BODY_KINDS, is_allowed_reference_namespace,
};

/// LSP-completion conveniences that are intentionally NOT standalone
/// `lazuli_keywords` literals. Each is justified; the membership test
/// excludes exactly these and nothing else.
///
/// * `inline_table` / `board` — bare aliases of the dotted registry
///   kinds `view.inline_table` / `view.board`; the LSP offers the bare
///   form in the global list as an ergonomic completion.
/// * `form` — legacy bare projection-column list (`list`/`form`/`detail`,
///   `lazuli_syntax::ast::SurfaceColumns`); not a standalone kind keyword.
/// * `external_calls` — an AST field shorthand surfaced for completion,
///   not a parser keyword (the keyword is `calls`).
const KEYWORDS_REGISTRY_ALLOWLIST: &[&str] = &["inline_table", "board", "form", "external_calls"];

/// `*.design.lzi` color-state names the LSP completes that the registry
/// does not yet carry as Design-surface rows. **Registry gap — flagged
/// to Wave H2** (registry owner): `lazuli_keywords` has the `base` /
/// `foreground` / `dark` color states but is missing `hover` and
/// `active`. When H2 adds those Design rows, drop them from this list.
const DESIGN_REGISTRY_ALLOWLIST: &[&str] = &["hover", "active"];

/// Registry literals whose `context` is **mis-classified** and so are
/// excluded from the per-context coverage assertion. **Flagged to Wave
/// H2** (registry owner — H3 must not edit the crate):
///
/// * `mode` / `service_ready` / `enforce_service_boundaries` — the
///   registry tags these `Context::App` (app-meta scalars), but the
///   parser + `lazuli_lsp::diagnostics::app::architecture` accept them
///   only as children of the `architecture` block, not as direct `app`
///   body lines. Including them in `APP_BODY_KINDS` would suppress the
///   legitimate typo squiggle and falsely accept them at indent-2 under
///   `app`. The registry should re-context these to an `Architecture`
///   context (or a dedicated app-architecture sub-context).
/// * `environment` — registry tags it `Context::App`; the canonical app
///   manifest has `environments` (block) + per-env `environment`
///   selectors nested below, not a bare top-level `environment` app
///   line. Registry should re-context to the environments sub-block.
///
/// When H2 re-contexts these, drop them here and they'll be coverage-
/// asserted against their correct block catalog.
const APP_CONTEXT_FLAGGED_TO_H2: &[&str] = &[
    "mode",
    "service_ready",
    "enforce_service_boundaries",
    "environment",
];

/// Registry literals tagged `Context::CommandBody` that are **NOT**
/// indent-4 `command` statements — **now EMPTY.**
///
/// This list previously grandfathered five mis-contexted registry rows that
/// were tagged `Context::CommandBody` but parse at a deeper indent / inside a
/// sub-block, so the indent-4 kind-head detector correctly never offers them
/// and listing them in `COMMAND_STATEMENT_KINDS` would suppress a legitimate
/// typo squiggle. WT-3 re-filed each to its real context in
/// `lazuli_keywords::registry`, so the `COMMAND_STATEMENT_KINDS`
/// reverse-coverage now passes cleanly with no exclusions:
///
/// * `output` — re-filed to `Context::Api` + `Context::Agent` (it is parsed
///   only on `api`/`operation`/`agent` bodies; a command body has `input`
///   but no `output`).
/// * `materialize` — the duplicate `CommandBody` row was removed; the correct
///   `Context::Audit` row (the `audit` sub-block child
///   `materialize @feature.x.Resource`) already carried it.
/// * `since` / `replacement` / `sunset` — re-filed to the new
///   `Context::Deprecated` (children of the `deprecated` sub-block).
///
/// It stays declared (empty) so the stale-flag hygiene assertion below keeps
/// proving the list is truly empty, and so a future deliberate exception has
/// an obvious documented home (it should essentially never be needed — fix the
/// registry context instead).
const COMMAND_CONTEXT_FLAGGED: &[&str] = &[];

fn registry_literals() -> BTreeSet<&'static str> {
    ALL.iter().map(|c| c.literal).collect()
}

/// All registry literals valid in `ctx` (one-directional coverage base).
fn literals_in_context(ctx: Context) -> BTreeSet<&'static str> {
    ALL.iter()
        .filter(|c| c.context == ctx)
        .map(|c| c.literal)
        .collect()
}

// ── KEYWORDS / DESIGN_KEYWORDS completeness (LSP ⊆ registry) ──────────

include!("group_13_keyword_registry_derivation_p1_tests.rs");
include!("group_13_keyword_registry_derivation_p2_tests.rs");
