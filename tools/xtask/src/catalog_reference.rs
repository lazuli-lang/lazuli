//! Generates `docs/closed-catalogs.md` from the single-source closed catalogs
//! in `lazuli_keywords` (`REFERENCE_NAMESPACES` / `SCALAR_TYPES` /
//! `SEMANTIC_TYPES` / `SCALAR_ALIASES`).
//!
//! This is the anti-drift mechanism for the language's closed catalogs: there
//! is ONE edit point (the consts in `lazuli_keywords`), every consumer (LSP,
//! doctor, analyzer) derives from it, and this doc is *generated* — never
//! hand-edited. The freshness gate `tools/xtask/tests/catalog_reference_fresh.rs`
//! fails the build if the committed doc drifts from the source, so the
//! three-way prose disagreement that used to publish 8 / 17 / 23 namespaces
//! can never recur.

use std::path::PathBuf;

/// Path to the committed catalog reference, relative to the workspace root.
const DOC_REL: &str = "docs/closed-catalogs.md";

/// Render the full generated document from the `lazuli_keywords` catalogs.
pub fn render() -> String {
    let mut s = String::new();
    s.push_str(
        "<!-- GENERATED FILE — DO NOT EDIT BY HAND.\n     \
Source of truth: crates/lazuli_keywords (REFERENCE_NAMESPACES / SCALAR_TYPES /\n     \
SEMANTIC_TYPES / SCALAR_ALIASES). Regenerate with:\n     \
cargo run -p xtask -- gen-catalog-reference\n     \
Freshness is gated by tools/xtask/tests/catalog_reference_fresh.rs. -->\n\n",
    );
    s.push_str("# Lazuli Closed Catalogs\n\n");
    s.push_str(
        "The single source of truth for the language's closed catalogs. The LSP, \
doctor, analyzer, and parser all derive from `lazuli_keywords`; this document is \
generated from the same source and gated for freshness, so it can never drift \
(the bug it replaces: three docs publishing 8 / 17 / 23 reference namespaces).\n\n",
    );

    s.push_str("## `@`-reference namespaces\n\n");
    s.push_str("A reference `@<ns>.<target>` is valid only when `<ns>` is one of:\n\n");
    for ns in lazuli_keywords::REFERENCE_NAMESPACES {
        s.push_str(&format!("- `@{ns}`\n"));
    }
    s.push('\n');

    s.push_str("## Scalar types\n\n");
    s.push_str("Canonical bare PascalCase scalar type names:\n\n");
    for t in lazuli_keywords::SCALAR_TYPES {
        s.push_str(&format!("- `{t}`\n"));
    }
    s.push('\n');

    s.push_str("## Semantic-scalar types\n\n");
    s.push_str(
        "Validated/formatted scalars (spelled `@semantic.<X>` today; bare after the \
`@`-off-types cut):\n\n",
    );
    for t in lazuli_keywords::SEMANTIC_TYPES {
        s.push_str(&format!("- `{t}`\n"));
    }
    s.push('\n');

    s.push_str("## Non-canonical scalar aliases\n\n");
    s.push_str(
        "These resolve for back-compat but are NOT canonical: `lazuli fmt` normalizes \
them and `VOCAB-SCALAR-ALIAS-001` flags them (reject + autocorrect, never silently \
tolerate).\n\n| alias | canonical |\n|---|---|\n",
    );
    for (alias, canonical) in lazuli_keywords::SCALAR_ALIASES {
        s.push_str(&format!("| `{alias}` | `{canonical}` |\n"));
    }
    s
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../tools/xtask; climb two levels to the root.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // xtask
    p.pop(); // tools
    p
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Entry point for the `gen-catalog-reference` subcommand.
pub fn run(check: bool) -> Result<(), String> {
    let path = workspace_root().join(DOC_REL);
    let fresh = render();
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    let same = normalize(&committed) == normalize(&fresh);

    if check {
        if same {
            println!("gen-catalog-reference --check: {DOC_REL} is fresh.");
            Ok(())
        } else {
            Err(format!(
                "stale {DOC_REL} — run `cargo run -p xtask -- gen-catalog-reference`."
            ))
        }
    } else if same {
        println!("gen-catalog-reference: {DOC_REL} already fresh, no write.");
        Ok(())
    } else {
        std::fs::write(&path, &fresh).map_err(|e| format!("writing {}: {e}", path.display()))?;
        println!("gen-catalog-reference: wrote {DOC_REL}.");
        Ok(())
    }
}
