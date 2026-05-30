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

// ── BUG-1: app-manifest block child-key validation (fail-closed gate) ─

use lazuli_keywords::{manifest_block_name, manifest_child_keys};

use super::{Diagnostic, DiagnosticSeverity, diagnostics_for};
use crate::diagnostics::app::VALIDATED_APP_BLOCKS;

/// The indent depth (in spaces) each validated app block's child keys sit
/// at, so the behavioural repro feeds a bogus child at the level the walker
/// actually inspects. Flat blocks validate their indent-4 line; the two
/// binding-header blocks (`cookie` profile / `encryption` key) validate the
/// indent-6 body. Mirrors the dispatch in
/// `app::operational::app_operational_contract_diagnostics`.
fn child_indent(block: &str) -> &'static str {
    match block {
        // Indent-6 bodies of an indent-4 binding header.
        "cookie" | "encryption" => "      ",
        // Flat indent-4 child keys.
        _ => "    ",
    }
}

/// The indent-4 opener line a binding-header block needs before its
/// indent-6 child is in scope (the walker only validates the body once a
/// profile / key binding is open). Empty for flat blocks.
fn block_opener(block: &str) -> Option<&'static str> {
    match block {
        "cookie" => Some("  cookie\n    default\n"),
        "encryption" => Some("  encryption\n    key @key.tenant\n"),
        // `error_page` is opened by an `error_page <NNN>` header at indent 2.
        "error_page" => None,
        _ => None,
    }
}

/// The indent-2 block header that opens `block` under `app`.
fn block_header(block: &str) -> String {
    match block {
        "error_page" => "  error_page 404\n".to_string(),
        // `cookie` / `encryption` carry their opener inline via
        // `block_opener`; everything else is a bare `  <block>` header.
        "cookie" | "encryption" => String::new(),
        _ => format!("  {block}\n"),
    }
}

/// (1) Single-source + (2) every validated block has a non-empty registry
/// child catalog. A block listed in `VALIDATED_APP_BLOCKS` whose
/// `manifest_child_keys` is empty would silently NO-OP the walker (the
/// helper returns early on an empty catalog), re-opening the BUG-1
/// silent-drop hole — so this FAILS CLOSED if a block loses its rows.
#[test]
fn every_validated_app_block_has_a_nonempty_child_catalog() {
    let mut empty = Vec::new();
    for &block in VALIDATED_APP_BLOCKS {
        if manifest_child_keys(block).next().is_none() {
            empty.push(block);
        }
    }
    assert!(
        empty.is_empty(),
        "VALIDATED_APP_BLOCKS contains block(s) with NO registry child rows — the walker would \
         silently no-op on them, re-opening the BUG-1 unknown-child drop. Add child-key rows to \
         `lazuli_keywords::registry` (with the matching `Context`) or remove the block: {empty:?}"
    );
}

/// (3) registry == walker. Every `lazuli_keywords::Context` that maps to an
/// app-manifest block name via `manifest_block_name` MUST be listed in
/// `VALIDATED_APP_BLOCKS`. Adding a `Context` + `manifest_block_name` row
/// without wiring the walker arm would leave that block's children
/// unvalidated — this FAILS CLOSED until the walker catches up.
#[test]
fn every_registry_manifest_block_is_validated_by_the_walker() {
    let validated: BTreeSet<&str> = VALIDATED_APP_BLOCKS.iter().copied().collect();
    let mut unwired: Vec<&'static str> = ALL
        .iter()
        .filter_map(|c| manifest_block_name(c.context))
        .filter(|block| !validated.contains(block))
        .collect();
    unwired.sort_unstable();
    unwired.dedup();
    assert!(
        unwired.is_empty(),
        "registry block(s) carry `manifest_block_name` child rows but are NOT in \
         VALIDATED_APP_BLOCKS — their children are unvalidated (BUG-1 hole). Wire a dispatch arm \
         in `app::operational` and add them to VALIDATED_APP_BLOCKS: {unwired:?}"
    );
}
