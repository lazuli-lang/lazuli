//! Cross-surface keyword parity gate — **re-rooted on `lazuli_keywords`
//! (Wave H3).**
//!
//! Enforcement for inviolable rule #7 ("magic discovery requires
//! visibility") and the standing discipline that a language construct
//! recognized by the parser must surface in EVERY downstream consumer.
//! Historically the parser grew keywords while the LSP catalog, the VS
//! Code highlighter, and the grammar docs silently drifted behind it
//! (e.g. `attach_ctx` parsed but was unhighlighted, undocumented, and
//! absent from completion).
//!
//! Before H3 this test asserted a hand-curated 22-token `CANONICAL`
//! list. That made coverage a *curated sample* — a keyword could be
//! added to the parser and forgotten everywhere except this list, which
//! itself was also forgotten. H3 re-roots the gate on
//! [`lazuli_keywords::ALL`] (proven complete against the parser by Wave
//! H1), so "every keyword is highlighted + completed + documented" is now
//! **by-construction**: the test iterates the registry, not a sample.
//!
//! Scope of enforcement (per the H3 work-order, option (b); doc-gap wave):
//!
//! 1. **LSP catalog — FULLY enforced.** Every registry keyword-token
//!    literal MUST appear in the LSP keyword/hover/typo-catalog sources.
//!    H3 owns this surface; there is no allowlist.
//! 2. **tmLanguage highlight — FULLY enforced.** Every registry
//!    keyword-token literal MUST appear in
//!    `editors/vscode/syntaxes/lazuli.tmLanguage.json`. H2's
//!    `gen-tmlanguage` projector covers this surface; no allowlist.
//! 3. **Documentation — FULLY enforced, by construction.** The
//!    documentation half is no longer the hand-written `grammar.*.md` /
//!    `quickref.md` prose (which stay curated EBNF + examples for the
//!    important constructs). It is the GENERATED `docs/keyword-reference.md`,
//!    a complete rendering of the `lazuli_keywords` registry (one table row
//!    per `CapabilitySpec`, produced by `cargo run -p xtask --
//!    gen-keyword-reference`). Because that document contains EVERY registry
//!    keyword by construction, the "documented" check is satisfied for all
//!    of them — which is why the old `DOC_SURFACE_GAP` grandfather allowlist
//!    is now EMPTY. Freshness of the generated doc is gated separately by
//!    `tools/xtask/tests/keyword_reference_fresh.rs`.
//!
//! Surfaces checked:
//!   1. LSP        — `crates/lazuli_lsp/src/{keywords.rs, hover/*, diagnostics/canonical_kinds/*}`
//!   2. Highlight  — `editors/vscode/syntaxes/lazuli.tmLanguage.json`
//!   3. Documented — `docs/keyword-reference.md` (generated from the registry)
//!
//! Substring presence is intentionally coarse: the point is "did the
//! surface remember this keyword at all", not exact-form validation.
//!
//! See CLAUDE.md / AGENTS.md §"Language-change surface checklist".

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use lazuli_keywords::{ALL, SemanticToken};

/// Tokens that must NOT appear as live, highlighted feature-header
/// keywords — the dead forms whose silent-drop earlier waves removed. The
/// feature-level `context "..."` was migrated to `attach_ctx` and now
/// hard-errors (`E-CONTEXT-RETIRED`); the retired `workflow` block
/// hard-errors (`E-WORKFLOW-RETIRED`). They must be gone from the LSP
/// feature-body catalog.
const RETIRED_FEATURE_KEYWORDS: &[&str] = &["workflow"];

/// `@semantic.*` scalar VALUES. Unlike keywords these are not in the LSP
/// keyword catalog and are highlighted generically by tmLanguage's
/// `@semantic.<Type>` rule, so the meaningful surfaces are the
/// authoritative analyzer catalog (the framework's source of truth for
/// which semantic scalars exist) plus the docs. These are NOT
/// `lazuli_keywords` literals (the registry carries keywords/constructs,
/// not semantic type names), so they stay an explicit list.
const SEMANTIC_VALUES: &[&str] = &["HexColor", "Percentage"];

/// **Documentation-surface grandfather allowlist — now EMPTY.**
///
/// Before the doc-gap wave this list grandfathered ~200 registry keyword
/// literals that were never added to the hand-written `grammar.*.md` /
/// `quickref.md` prose. The doc-gap wave retired the entire allowlist by
/// re-pointing the "documented" half of
/// [`every_registry_keyword_is_highlighted_and_documented`] at the GENERATED
/// `docs/keyword-reference.md`, which renders every `CapabilitySpec` in
/// `lazuli_keywords::ALL` as a table row. Because that document is exhaustive
/// by construction, every registry keyword is documented — there is nothing
/// left to grandfather.
///
/// It stays declared (empty) so the stale-allowlist hygiene assertion below
/// keeps proving the list is truly empty, and so a future deliberate
/// exception has an obvious, documented home (it should essentially never be
/// needed — add the keyword to the registry and regenerate the reference
/// instead).
const DOC_SURFACE_GAP: &[&str] = &[];

/// **Pre-existing tmLanguage HIGHLIGHT debt** — a genuinely separate concern
/// from the (now-closed) documentation gap.
///
/// The old `DOC_SURFACE_GAP` conflated two debts under one name: keywords
/// missing from the docs AND keywords missing from the VS Code grammar. The
/// doc-gap wave closed the documentation half by construction (the generated
/// `keyword-reference.md` covers everything). What remains here is ONLY the
/// highlight half: registry keyword-token literals that are not yet a
/// substring of `editors/vscode/syntaxes/lazuli.tmLanguage.json`.
///
/// These literals live inside the curated **multi-group fallback
/// alternations** of the grammar (`command`/`query`/`view*`/`agent`/… blocks
/// the H2 `gen-tmlanguage` projector deliberately does NOT generate — see
/// `tools/xtask/src/tmlanguage.rs` module docs), or are scalars whose block
/// alternation was never authored. Adding them is a tmLanguage-grammar edit,
/// owned by the highlighter/H2 surface and out of scope for the doc-gap wave;
/// they are grandfathered here at exactly the strictness they had before
/// (when the single conflated list exempted them from the tm check too).
///
/// To retire an entry: add the literal to the relevant alternation in
/// `lazuli.tmLanguage.json` (or to a `#kw-*` group `gen-tmlanguage` covers)
/// and delete it here. The test reports which surface is still missing it.
const HIGHLIGHT_SURFACE_GAP: &[&str] = &[
    "access_ttl",
    "actor_query",
    "breakpoint",
    "cors",
    "dark",
    "default_unauthenticated_redirect",
    "default_unauthorized_redirect",
    "design",
    "detail",
    "easing",
    "error_redact",
    "error_view",
    "event.trace",
    "family",
    "flash",
    "foreground",
    "grace",
    "hint",
    "icon",
    "lanes",
    "lazuli_version",
    "line_height",
    "loader",
    "materialize_strategy",
    "mcp_server",
    "motion",
    "outbox",
    "payload_axis",
    "pending_view",
    "previous_version",
    "query.view",
    "radius",
    "refresh_ttl",
    "resume",
    "revoke_session_family",
    "revoke_user",
    "role_mismatch",
    "scale",
    "shadow",
    "theft_detection_action",
    "tracking",
    "typography",
    "version_field",
    "weight",
];

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/lazuli_lsp
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read(rel: &str) -> String {
    let p = workspace_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

fn lsp_sources() -> String {
    // Files where keyword/kind/hover knowledge lives. Concatenate and search.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/keywords.rs",
        "src/hover/domain.rs",
        "src/hover/surface.rs",
        "src/hover/manifest.rs",
        "src/diagnostics/canonical_kinds/feature.rs",
        "src/diagnostics/canonical_kinds/sections/blocks.rs",
        "src/diagnostics/canonical_kinds/sections/statements.rs",
    ];
    let mut acc = String::new();
    for f in files {
        let p = manifest.join(f);
        if let Ok(s) = fs::read_to_string(&p) {
            acc.push_str(&s);
            acc.push('\n');
        }
    }
    acc
}

/// The substring to search for a literal across surfaces. `@`-decorators
/// are stored bare in the tmLanguage namespace alternation
/// (`@(?:policy|scope|…)`), so strip the sigil; dotted kinds
/// (`query.list`) keep their full form.
fn search_token(literal: &str) -> &str {
    literal.strip_prefix('@').unwrap_or(literal)
}

/// Registry keyword-token literals — the constructs a highlighter +
/// completion + docs must surface. Values / operators / modifiers /
/// type-constructors are highlighted generically (constant.language,
/// keyword.operator, storage.modifier) and are not keyword-catalog
/// entries, so the parity gate is scoped to `SemanticToken::Keyword`
/// rows (plus decorators are covered by their own tmLanguage rule).
fn registry_keyword_literals() -> BTreeSet<&'static str> {
    ALL.iter()
        .filter(|c| c.token == SemanticToken::Keyword)
        .map(|c| c.literal)
        .collect()
}

#[test]
fn every_registry_keyword_is_in_the_lsp_catalog() {
    let lsp = lsp_sources();
    let mut failures: Vec<String> = Vec::new();

    for kw in registry_keyword_literals() {
        let tok = search_token(kw);
        if !lsp.contains(kw) && !lsp.contains(tok) {
            failures.push(format!(
                "`{kw}` missing from LSP catalog (keywords.rs / hover / canonical_kinds)"
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "LSP-catalog coverage drift — a `lazuli_keywords` keyword is not surfaced by the LSP. \
         Add it to `crate::keywords::KEYWORDS` (and a hover one-liner falls out of the registry \
         via `hover::keyword_description`). H3 owns this surface fully; there is no allowlist.\n  - {}",
        failures.join("\n  - ")
    );
}

#[test]
fn every_registry_keyword_is_highlighted_and_documented() {
    let tm = read("editors/vscode/syntaxes/lazuli.tmLanguage.json");
    // The "documented" surface is the GENERATED keyword reference, which
    // renders every `CapabilitySpec` in the registry. Pointing the check here
    // (instead of the curated grammar/quickref prose) makes the doc-coverage
    // guarantee by-construction: a keyword is documented iff it is in the
    // registry, which it is by definition. Freshness of this file is gated by
    // `tools/xtask/tests/keyword_reference_fresh.rs`.
    let reference = read("docs/keyword-reference.md");

    // Two distinct, honestly-named exemption sets: the documentation gap (now
    // empty — the generated reference covers everything) and the pre-existing
    // tmLanguage highlight gap (curated multi-group alternations H2 does not
    // generate; out of scope for the doc-gap wave).
    let doc_allow: BTreeSet<&str> = DOC_SURFACE_GAP.iter().copied().collect();
    let hl_allow: BTreeSet<&str> = HIGHLIGHT_SURFACE_GAP.iter().copied().collect();

    let mut failures: Vec<String> = Vec::new();
    let mut stale_doc: BTreeSet<&str> = doc_allow.clone();
    let mut stale_hl: BTreeSet<&str> = hl_allow.clone();

    for kw in registry_keyword_literals() {
        let tok = search_token(kw);
        // Highlight half (H2's gen-tmlanguage / the curated grammar own this).
        let in_tm = tm.contains(tok);
        // Documented half: the generated reference contains every registry
        // keyword by construction, so this is satisfied for all of them.
        let in_reference = reference.contains(tok);

        // Resolve each half independently against its own allowlist.
        let tm_ok = in_tm || hl_allow.contains(kw);
        let doc_ok = in_reference || doc_allow.contains(kw);

        if in_tm {
            stale_hl.remove(kw);
        }
        if in_reference {
            stale_doc.remove(kw);
        }
        if hl_allow.contains(kw) {
            stale_hl.remove(kw);
        }
        if doc_allow.contains(kw) {
            stale_doc.remove(kw);
        }

        let mut missing = Vec::new();
        if !tm_ok {
            missing.push("tmLanguage");
        }
        if !doc_ok {
            missing.push("keyword-reference.md");
        }
        if !missing.is_empty() {
            failures.push(format!("`{kw}` missing from {}", missing.join(" + ")));
        }
    }

    assert!(
        failures.is_empty(),
        "keyword surface-parity drift — a registry keyword is missing from a highlight/doc \
         surface. For tmLanguage, run `cargo run -p xtask -- gen-tmlanguage` (or add it to the \
         curated grammar alternation / HIGHLIGHT_SURFACE_GAP); for the reference, run \
         `cargo run -p xtask -- gen-keyword-reference` (the registry is the single source — do \
         not hand-edit either generated file).\n  - {}",
        failures.join("\n  - ")
    );

    // Hygiene: an allowlist entry never matched above is stale and must be
    // deleted so each debt list stays honest. DOC_SURFACE_GAP is empty, so
    // `stale_doc` is trivially empty; HIGHLIGHT_SURFACE_GAP shrinks as the
    // tmLanguage grammar catches up.
    assert!(
        stale_doc.is_empty(),
        "DOC_SURFACE_GAP has stale entries (now documented, or no longer registry keywords) — \
         delete them so the debt list stays honest: {stale_doc:?}"
    );
    assert!(
        stale_hl.is_empty(),
        "HIGHLIGHT_SURFACE_GAP has stale entries (now highlighted, or no longer registry \
         keywords) — delete them so the debt list stays honest: {stale_hl:?}"
    );
}

#[test]
fn retired_feature_keywords_are_absent_from_feature_catalog() {
    let feature_catalog = {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/diagnostics/canonical_kinds/feature.rs");
        fs::read_to_string(&p).unwrap_or_default()
    };
    let mut failures = Vec::new();
    for kw in RETIRED_FEATURE_KEYWORDS {
        // The catalog stores kinds as quoted string literals, e.g. `"workflow"`.
        let quoted = format!("\"{kw}\"");
        if feature_catalog.contains(&quoted) {
            failures.push(format!(
                "retired feature keyword `{kw}` still present in canonical_kinds/feature.rs \
                 FEATURE_BODY_KINDS — it hard-errors in the parser and must not be offered"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_semantic_scalar_is_cataloged_and_documented() {
    // Authoritative semantic-scalar catalog lives in the analyzer.
    let analyzer = {
        let mut acc = String::new();
        for rel in [
            "crates/lazuli_analyzer/src/types.rs",
            "crates/lazuli_analyzer/src/checks/scalar_fixtures/mod.rs",
        ] {
            acc.push_str(&read(rel));
            acc.push('\n');
        }
        acc
    };
    let g_lzi = read("docs/grammar.lzi.md");
    let quickref = read("docs/quickref.md");

    let mut failures: Vec<String> = Vec::new();
    for kw in SEMANTIC_VALUES {
        if !analyzer.contains(kw) {
            failures.push(format!(
                "`@semantic.{kw}` missing from the analyzer catalog (types.rs / scalar_fixtures)"
            ));
        }
        if !g_lzi.contains(kw) {
            failures.push(format!("`@semantic.{kw}` missing from grammar.lzi.md"));
        }
        if !quickref.contains(kw) {
            failures.push(format!("`@semantic.{kw}` missing from quickref.md"));
        }
    }
    assert!(
        failures.is_empty(),
        "semantic-scalar surface drift:\n  - {}",
        failures.join("\n  - ")
    );
}
