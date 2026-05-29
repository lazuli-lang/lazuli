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

#[test]
fn keywords_catalog_is_a_subset_of_the_registry() {
    let registry = registry_literals();
    let allow: BTreeSet<&str> = KEYWORDS_REGISTRY_ALLOWLIST.iter().copied().collect();

    let strays: Vec<&str> = KEYWORDS
        .iter()
        .copied()
        .filter(|kw| !registry.contains(kw) && !allow.contains(kw))
        .collect();

    assert!(
        strays.is_empty(),
        "`KEYWORDS` offers literals absent from `lazuli_keywords::ALL`: {strays:?}. \
         Either add the keyword to the registry (flag Wave H2 — do not edit the crate here) \
         or, if it is a deliberate LSP-only completion convenience, add it to \
         KEYWORDS_REGISTRY_ALLOWLIST with a justification."
    );
}

#[test]
fn design_keywords_catalog_is_a_subset_of_the_registry() {
    let registry = registry_literals();
    let allow: BTreeSet<&str> = DESIGN_REGISTRY_ALLOWLIST.iter().copied().collect();

    let strays: Vec<&str> = DESIGN_KEYWORDS
        .iter()
        .copied()
        .filter(|kw| !registry.contains(kw) && !allow.contains(kw))
        .collect();

    assert!(
        strays.is_empty(),
        "`DESIGN_KEYWORDS` offers literals absent from `lazuli_keywords::ALL`: {strays:?}. \
         Add the Design row to the registry (flag Wave H2) or extend DESIGN_REGISTRY_ALLOWLIST."
    );
}

// ── typo catalogs: completeness (LSP ⊆ registry) ─────────────────────

fn typo_catalogs() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("FEATURE_BODY_KINDS", FEATURE_BODY_KINDS),
        ("APP_BODY_KINDS", APP_BODY_KINDS),
        ("REGISTRY_BODY_KINDS", REGISTRY_BODY_KINDS),
        ("VIEW_BODY_KINDS", VIEW_BODY_KINDS),
        ("SURFACE_BODY_KINDS", SURFACE_BODY_KINDS),
        ("COMMAND_STATEMENT_KINDS", COMMAND_STATEMENT_KINDS),
        ("QUERY_STATEMENT_KINDS", QUERY_STATEMENT_KINDS),
        ("AUDIENCE_BODY_KINDS", AUDIENCE_BODY_KINDS),
        ("SESSIONS_BODY_KINDS", SESSIONS_BODY_KINDS),
        ("SESSIONS_COOKIE_BODY_KINDS", SESSIONS_COOKIE_BODY_KINDS),
    ]
}

/// Every typo-catalog entry must be a real registry literal. (No
/// allowlist needed — all ten catalogs are clean against the registry,
/// including the `cookie-sessions-child` `SESSIONS_BODY_KINDS` /
/// `SESSIONS_COOKIE_BODY_KINDS` pair.)
#[test]
fn typo_catalogs_are_subsets_of_the_registry() {
    let registry = registry_literals();
    let mut failures = Vec::new();
    for (name, list) in typo_catalogs() {
        for kw in list {
            if !registry.contains(kw) {
                failures.push(format!(
                    "`{kw}` in {name} is not a `lazuli_keywords` literal"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "typo catalog drift — an entry is not a known registry literal:\n  - {}",
        failures.join("\n  - ")
    );
}

// ── typo catalogs: coverage (registry ⊆ LSP), per block context ──────

/// Every registry literal valid in a block context must be offered by
/// that block's typo catalog. By-construction guarantee that a parser
/// keyword can never silently vanish from the "did you mean" surface.
/// One-directional: the curated lists may additionally include
/// cross-context inclusions (asserted as subsets, not equality).
///
/// **Re-enabled for `COMMAND_STATEMENT_KINDS` (Guard B — close the F1
/// drift hole).** The F1-F5 root-cause triage found this reverse direction
/// was DISABLED for the command-body context, which is exactly the hole F1
/// fell through: `triggers` was a `Context::CommandBody` registry row but
/// absent from `COMMAND_STATEMENT_KINDS`, so a typo'd / missing `triggers`
/// produced no squiggle. F1 added `triggers` to the catalog; this gate now
/// asserts that direction by construction. Mentally deleting `triggers`
/// from `COMMAND_STATEMENT_KINDS` re-reddens this test.
///
/// `COMMAND_CONTEXT_FLAGGED` is now EMPTY: the five previously-flagged
/// literals (`output` / `materialize` / `since` / `replacement` / `sunset`)
/// were re-filed to their real registry contexts in WT-3 (`output` → Api +
/// Agent; `materialize` duplicate removed in favour of the Audit row;
/// `since`/`replacement`/`sunset` → `Context::Deprecated`), so the
/// `COMMAND_STATEMENT_KINDS` reverse-coverage now passes with no exclusions.
/// A stale-flag hygiene assertion keeps the (empty) list honest.
///
/// Scoped to the contexts where the registry's single-context projection
/// (minus the documented mis-context flags) is a clean subset of the
/// curated list. FEATURE_BODY_KINDS / VIEW_BODY_KINDS remain excluded: the
/// registry tags some statement-level literals to those same indent-2
/// *kind*-head contexts that the kind-head typo detector does not guard —
/// exact coverage there awaits a registry context-split (tracked for a
/// follow-up wave). Completeness for all ten catalogs is still enforced by
/// `typo_catalogs_are_subsets_of_the_registry`.
#[test]
fn typo_catalogs_cover_their_registry_context() {
    // Per-case mis-context flag sets (registry literals that genuinely do
    // not belong in that block's typo catalog despite their `context` tag).
    let app_flags: BTreeSet<&str> = APP_CONTEXT_FLAGGED_TO_H2.iter().copied().collect();
    let cmd_flags: BTreeSet<&str> = COMMAND_CONTEXT_FLAGGED.iter().copied().collect();
    let none: BTreeSet<&str> = BTreeSet::new();

    let cases: &[(&str, Context, &[&str], &BTreeSet<&str>)] = &[
        ("APP_BODY_KINDS", Context::App, APP_BODY_KINDS, &app_flags),
        (
            "REGISTRY_BODY_KINDS",
            Context::Registry,
            REGISTRY_BODY_KINDS,
            &none,
        ),
        (
            "SURFACE_BODY_KINDS",
            Context::Surface,
            SURFACE_BODY_KINDS,
            &none,
        ),
        (
            "QUERY_STATEMENT_KINDS",
            Context::Query,
            QUERY_STATEMENT_KINDS,
            &none,
        ),
        (
            "AUDIENCE_BODY_KINDS",
            Context::SurfaceAudience,
            AUDIENCE_BODY_KINDS,
            &none,
        ),
        (
            "COMMAND_STATEMENT_KINDS",
            Context::CommandBody,
            COMMAND_STATEMENT_KINDS,
            &cmd_flags,
        ),
    ];

    let mut failures = Vec::new();
    for (name, ctx, list, flagged) in cases {
        let list_set: BTreeSet<&str> = list.iter().copied().collect();
        for lit in literals_in_context(*ctx) {
            if !list_set.contains(lit) && !flagged.contains(lit) {
                failures.push(format!(
                    "registry literal `{lit}` (context {ctx:?}) is missing from {name}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "typo catalog coverage gap — a registry keyword for this block context is not \
         offered by the LSP typo catalog:\n  - {}",
        failures.join("\n  - ")
    );

    // Hygiene: a `COMMAND_CONTEXT_FLAGGED` entry is only meaningful if it is
    // still a `Context::CommandBody` registry literal that is absent from
    // `COMMAND_STATEMENT_KINDS` (otherwise the exclusion is dead). Now that the
    // list is empty this is trivially satisfied; the assertion exists so a
    // future stale flag (a literal that was re-contexted away from CommandBody,
    // or added to the catalog) is caught and must be deleted.
    let cmd_body = literals_in_context(Context::CommandBody);
    let cmd_kinds: BTreeSet<&str> = COMMAND_STATEMENT_KINDS.iter().copied().collect();
    let stale_cmd_flags: Vec<&str> = COMMAND_CONTEXT_FLAGGED
        .iter()
        .copied()
        .filter(|lit| !cmd_body.contains(lit) || cmd_kinds.contains(lit))
        .collect();
    assert!(
        stale_cmd_flags.is_empty(),
        "COMMAND_CONTEXT_FLAGGED has stale entries (re-contexted away from CommandBody, or now \
         present in COMMAND_STATEMENT_KINDS) — delete them so the exclusion list stays honest: \
         {stale_cmd_flags:?}"
    );
}

// ── hover one-liners derive from the registry ────────────────────────

/// Every bare-keyword registry literal that carries a non-empty `hover`
/// one-liner must resolve through `keyword_description` — either via the
/// rich/curated hand modules (which win first-match) or the Wave-H3
/// registry fallback wired into `hover::keyword_description`. This makes
/// "the registry's one-liner is surfaced" a by-construction guarantee.
#[test]
fn registry_hover_one_liners_are_reachable_via_keyword_description() {
    use super::keyword_description;

    let mut missing = Vec::new();
    for spec in ALL.iter() {
        // Only bare keywords carry curated hovers; values / operators /
        // modifiers either have empty hovers or are not keyword-hover
        // surfaces. Gate on a non-empty hover + bare-keyword shape.
        if spec.hover.is_empty() || !spec.is_bare_keyword() {
            continue;
        }
        if keyword_description(spec.literal).is_none() {
            missing.push(spec.literal);
        }
    }
    missing.sort_unstable();
    missing.dedup();
    assert!(
        missing.is_empty(),
        "registry keywords carry a `hover` one-liner but `keyword_description` returns None \
         for them — the H3 registry fallback should cover these: {missing:?}"
    );
}

// ── registry → namespace parity (Guard B — close the F3 drift hole) ──

/// `@`-decorator literals in the registry that are **bare ATTRIBUTE
/// markers**, not reference namespaces of the `@<ns>.<target>` form. The
/// `namespace_reference_diagnostics` rule only scans `@<ns>.<x>` references
/// (it requires a `.`), so a bare `@slug` / `@full_text` / `@owner_axis` /
/// `@resume` decorator is never validated against
/// `is_allowed_reference_namespace` and must NOT be required to live there.
///
/// (`@pii` / `@key` / `@cap` / `@semantic` DO carry a dotted target — e.g.
/// `@semantic.HexColor`, `@cap.File` — and so are genuine reference
/// namespaces already present in the allowlist; they are NOT excluded.)
///
/// Each excluded literal is a standalone field/relation decorator with no
/// namespace target:
/// * `@slug` — slug-field marker.
/// * `@full_text` — full-text-index marker.
/// * `@owner_axis` — ownership-axis marker.
/// * `@resume` — lifecycle resume marker (`on_lifecycle_pending @resume f`).
const ATTRIBUTE_DECORATORS: &[&str] = &["@slug", "@full_text", "@owner_axis", "@resume"];

/// **Registry → namespace parity (the F3 class).** Every `@`-sigil
/// reference-namespace decorator the registry carries MUST be accepted by
/// the LSP `is_allowed_reference_namespace` catalog — otherwise a primitive
/// the parser/registry knows uses a namespace the LSP rejects with a
/// spurious `namespace-catalog` warning. This is exactly the F3 failure
/// class (a primitive using a namespace the LSP catalog doesn't allow); F3
/// added `@feature`, and this gate asserts that direction by construction
/// so no future decorator can drift the two surfaces apart again.
///
/// Bare attribute decorators (`ATTRIBUTE_DECORATORS`) are excluded: they
/// have no `@<ns>.<target>` form, so the namespace catalog never validates
/// them. Everything else (`@semantic`, `@cap`, `@policy`, `@scope`,
/// `@fn`, `@command`, `@file`, `@audience`, …) is reference-form and must
/// be allowed.
///
/// Spot-check that this goes RED if drift reopens: removing `feature` from
/// `is_allowed_reference_namespace` (the F3 fix) re-reddens this test with
/// "`@feature` ... not accepted".
#[test]
fn registry_decorator_namespaces_are_allowed_references() {
    let attr: BTreeSet<&str> = ATTRIBUTE_DECORATORS.iter().copied().collect();

    let mut failures = Vec::new();
    for spec in ALL.iter() {
        if spec.sigil != Some(Sigil::At) {
            continue;
        }
        if attr.contains(spec.literal) {
            continue;
        }
        // The namespace is the decorator literal minus its `@` sigil and
        // any dotted-target tail (registry decorator rows are bare, e.g.
        // `@semantic`, so there is no tail — but strip defensively).
        let namespace = spec
            .literal
            .strip_prefix('@')
            .unwrap_or(spec.literal)
            .split('.')
            .next()
            .unwrap_or("");
        if !is_allowed_reference_namespace(namespace) {
            failures.push(format!(
                "registry decorator `{}` (namespace `{namespace}`) is not accepted by \
                 `is_allowed_reference_namespace` — add it to the LSP namespace catalog in \
                 `diagnostics::vocab`, or, if it is a bare attribute marker with no \
                 `@<ns>.<target>` form, add it to ATTRIBUTE_DECORATORS with a justification",
                spec.literal
            ));
        }
    }
    failures.sort_unstable();
    failures.dedup();
    assert!(
        failures.is_empty(),
        "registry → namespace parity drift (F3 class) — a registry `@`-decorator uses a \
         namespace the LSP `is_allowed_reference_namespace` catalog rejects:\n  - {}",
        failures.join("\n  - ")
    );
}
