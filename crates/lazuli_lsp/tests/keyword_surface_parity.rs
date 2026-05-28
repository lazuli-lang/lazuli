//! Cross-surface keyword parity gate.
//!
//! Enforcement for inviolable rule #7 ("magic discovery requires visibility")
//! and the standing discipline that a language construct recognized by the
//! parser must surface in EVERY downstream consumer. Historically the parser
//! grew keywords while the LSP catalog, the VS Code highlighter, and the
//! grammar docs silently drifted behind it (e.g. `attach_ctx` parsed but was
//! unhighlighted, undocumented, and absent from completion). This test makes
//! that drift a build failure.
//!
//! For every canonical keyword/construct below, assert it appears in all four
//! surfaces:
//!   1. LSP        — `crates/lazuli_lsp/src/{keywords.rs, hover/*, diagnostics/canonical_kinds/*}`
//!   2. Highlight  — `editors/vscode/syntaxes/lazuli.tmLanguage.json`
//!   3. Grammar    — `docs/grammar.lzi.md` or `docs/grammar.lzx.md`
//!   4. Quickref   — `docs/quickref.md`
//!
//! Substring presence is intentionally coarse: the point is "did the author
//! remember this surface at all", not exact-form validation. When you add a
//! new keyword to the parser, add its token here and to the four surfaces, or
//! this test fails and names the gap.
//!
//! See CLAUDE.md / AGENTS.md §"Language-change surface checklist".

use std::fs;
use std::path::PathBuf;

/// Canonical token for each keyword/construct that must be mirrored across
/// surfaces. `(token, lzx_surface)` — when `lzx_surface` is true the grammar
/// check looks in `grammar.lzx.md`, otherwise `grammar.lzi.md`. Quickref and
/// highlighting and LSP are required for all.
const CANONICAL: &[(&str, bool)] = &[
    // ── iron-hand feature-header vocabulary ──
    ("attach_ctx", false),
    ("purpose", false),
    ("non_goals", false),
    // ── resource modifiers / field constructs (.lzi) ──
    ("append_only", false),
    ("many_through", false),
    ("polymorphic_ref", false),
    ("computed_date", false),
    ("schedule_rule", false),
    // Field decorators: stored `@`-prefixed in the LSP/grammar, but bare in the
    // tmLanguage `@(?:...|slug|owner_axis|...)` namespace alternation — match the
    // bare token so the check is consistent across surfaces.
    ("slug", false),
    ("owner_axis", false),
    // ── command / audit / approval / report (.lzi) ──
    ("reorder", false),
    ("materialize", false),
    ("chain", false),
    ("sequential", false),
    // ── surface primitives (.lzx) ──
    ("tab_group", true),
    ("wizard_steps", true),
    ("view_mode", true),
    ("inline_table", true),
    ("date_range", true),
    ("repeatable", true),
];

/// Tokens that must NOT appear as live, highlighted feature-header keywords —
/// the dead forms whose silent-drop this effort removed. The feature-level
/// `context "..."` was migrated to `attach_ctx` and now hard-errors
/// (`E-CONTEXT-RETIRED`); the retired `workflow` block hard-errors
/// (`E-WORKFLOW-RETIRED`). They must be gone from the LSP feature-body catalog.
const RETIRED_FEATURE_KEYWORDS: &[&str] = &["workflow"];

/// `@semantic.*` scalar VALUES. Unlike keywords these are not in the LSP keyword
/// catalog and are highlighted generically by tmLanguage's `@semantic.<Type>`
/// rule, so the meaningful surfaces are the authoritative analyzer catalog (the
/// framework's source of truth for which semantic scalars exist) plus the docs.
/// When you add a `@semantic.X` scalar, add it to the analyzer catalog AND the
/// grammar docs / quickref, or this fails.
const SEMANTIC_VALUES: &[&str] = &["HexColor", "Percentage"];

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

#[test]
fn every_canonical_keyword_is_mirrored_across_surfaces() {
    let lsp = lsp_sources();
    let tm = read("editors/vscode/syntaxes/lazuli.tmLanguage.json");
    let g_lzi = read("docs/grammar.lzi.md");
    let g_lzx = read("docs/grammar.lzx.md");
    let quickref = read("docs/quickref.md");

    let mut failures: Vec<String> = Vec::new();
    for (kw, lzx_surface) in CANONICAL {
        if !lsp.contains(kw) {
            failures.push(format!(
                "`{kw}` missing from LSP catalog (keywords.rs / hover / canonical_kinds)"
            ));
        }
        if !tm.contains(kw) {
            failures.push(format!("`{kw}` missing from tmLanguage.json (highlighting)"));
        }
        let grammar = if *lzx_surface { &g_lzx } else { &g_lzi };
        let grammar_name = if *lzx_surface {
            "grammar.lzx.md"
        } else {
            "grammar.lzi.md"
        };
        if !grammar.contains(kw) {
            failures.push(format!("`{kw}` missing from {grammar_name} (grammar docs)"));
        }
        if !quickref.contains(kw) {
            failures.push(format!("`{kw}` missing from quickref.md"));
        }
    }

    assert!(
        failures.is_empty(),
        "keyword surface-parity drift detected — a parser keyword is missing from a downstream \
         surface. Update the surface(s) below (or remove the token from CANONICAL if the keyword \
         was retired). See CLAUDE.md §\"Language-change surface checklist\".\n  - {}",
        failures.join("\n  - ")
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
