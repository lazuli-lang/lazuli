//! `docs/keyword-reference.md` generation from `lazuli_keywords::ALL`.
//!
//! # What this generates
//!
//! The complete, human-readable rendering of the capability registry: a
//! Markdown document with one table row per [`CapabilitySpec`], grouped by
//! [`Surface`] then [`Context`]. Every keyword/literal in the registry is
//! documented **by construction** — there is no hand-curated subset, so the
//! `keyword_surface_parity` doc-coverage gate can re-point its "documented"
//! check at this file and find every keyword present.
//!
//! Each row renders six facets straight off the [`CapabilitySpec`]:
//!
//! 1. **keyword / literal** — `CapabilitySpec::literal` (in backticks);
//! 2. **scope** — the TextMate scope leaf (`CapabilitySpec::scope`);
//! 3. **semantic-token** — the LSP `SemanticToken` projection;
//! 4. **sigil** — `@` / dotted-kind / none;
//! 5. **hover** — the one-line description (`CapabilitySpec::hover`);
//! 6. **produces[]** — the diagnostic codes (Wave C1) the capability guards.
//!
//! # Boundary
//!
//! Like the tmLanguage generator, this is a **pure projector**: it deps
//! `lazuli_keywords` + `std` only (no `serde_json` needed — the output is
//! Markdown text, assembled by hand for byte-exact determinism). The registry
//! is the single input; ordering is fully deterministic (stable Surface order,
//! stable Context order, then literal-then-scope within a group).
//!
//! # Freshness
//!
//! `gen-keyword-reference --check` regenerates the document in memory and
//! asserts it is byte-identical to the committed `docs/keyword-reference.md`.
//! `tools/xtask/tests/keyword_reference_fresh.rs` wires this into
//! `cargo test --workspace` so a hand-edit or a forgotten regen fails loudly.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lazuli_keywords::{ALL, CapabilitySpec, Context, Sigil, Surface};

/// Path to the committed reference, relative to the workspace root.
const DOC_REL: &str = "docs/keyword-reference.md";

/// Header banner written at the top of the generated document. Anything
/// before [`BODY_SENTINEL`] is prose the generator owns and rewrites every
/// run, so the whole file is machine-owned and the `--check` gate is total.
const HEADER: &str = "\
<!-- GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: crates/lazuli_keywords (the `ALL` capability registry).
     Regenerate with: cargo run -p xtask -- gen-keyword-reference
     Freshness is gated by tools/xtask/tests/keyword_reference_fresh.rs and by
     the `keyword_surface_parity` doc-coverage test. -->

# Lazuli keyword reference

This is the **complete** catalog of every keyword/construct the Lazuli parser
recognizes, rendered directly from the `lazuli_keywords` capability registry.
Every row is one `CapabilitySpec`; a literal valid in N contexts with distinct
scopes appears as N rows (context-as-data). Because this document is generated
from the registry, it is exhaustive by construction — if the parser knows a
keyword, it is here.

Prose explanations, EBNF, and worked examples live in the hand-written
`docs/grammar.*.md` and `docs/quickref.md`. This file is the flat, exhaustive
index those documents are checked against.

Columns:

- **keyword / literal** — the exact literal the parser recognizes.
- **scope** — the TextMate scope leaf the highlighter assigns (see
  `editors/vscode/SCOPES.md`).
- **semantic token** — the LSP semantic-token type.
- **sigil** — `@` for decorators, `.` for dotted kinds, `—` for bare keywords.
- **hover** — the one-line description surfaced on hover/completion.
- **produces** — diagnostic codes (`lazuli doctor` rules) this construct gates,
  if any.";

/// Sentinel marking the start of the generated body (the per-Surface tables).
/// Everything from this line to EOF is rebuilt verbatim each run.
const BODY_SENTINEL: &str = "<!-- BEGIN GENERATED BODY -->";

/// One capability rendered as a table row, pre-grouped by (surface, context).
struct Row {
    surface: Surface,
    context: Context,
    literal: &'static str,
    scope: &'static str,
    token: &'static str,
    sigil: &'static str,
    hover: String,
    produces: String,
}

/// Stable display + ordering label for a [`Surface`]. The numeric prefix fixes
/// section order independent of enum declaration order.
fn surface_order(s: Surface) -> (u8, &'static str) {
    match s {
        Surface::Lzi => (0, "`.lzi` — feature source"),
        Surface::Lzx => (1, "`.lzx` — surface source"),
        Surface::App => (2, "App manifest / orchestration"),
        Surface::Registry => (3, "Registry source"),
        Surface::Manifest => (4, "`Lazurite.toml` manifest overlay"),
        Surface::Design => (5, "`design.lzi` token catalog"),
    }
}

/// Stable ordering rank for a [`Context`], mirroring the declaration order in
/// `lazuli_keywords::Context` so the reference reads top-down the way the
/// grammar is organized. `Context` is `#[non_exhaustive]`, so a new variant
/// (one not yet ranked here) falls through to a sentinel rank and is ordered
/// last by its `Debug` name — still deterministic, never a build break, and
/// the missing rank is the only thing to add when a context lands.
fn context_order(c: Context) -> u16 {
    use Context::*;
    match c {
        TopLevel => 0,
        App => 1,
        Registry => 2,
        FeatureHeader => 3,
        ResourceBody => 4,
        EnumBody => 5,
        CommandBody => 6,
        Query => 7,
        Job => 8,
        Webhook => 9,
        Agent => 10,
        Notification => 11,
        Poller => 12,
        Report => 13,
        Channel => 14,
        TenantMigration => 15,
        Api => 16,
        McpServer => 17,
        Lifecycle => 18,
        Audit => 19,
        Approval => 20,
        Policy => 21,
        Errors => 22,
        Defaults => 23,
        Invariants => 24,
        Emits => 25,
        EventGroup => 26,
        Cache => 27,
        Tests => 28,
        Extensions => 29,
        Translation => 30,
        Auth => 31,
        Cookie => 32,
        Headers => 33,
        Limits => 34,
        Proxy => 35,
        Cors => 351,
        RouteGuard => 352,
        Encryption => 36,
        Locale => 37,
        Logging => 38,
        Tracing => 39,
        Runtime => 40,
        Deploy => 41,
        Services => 42,
        Communication => 43,
        Urls => 44,
        Env => 45,
        Integrations => 46,
        Capabilities => 47,
        Bindings => 48,
        Packs => 49,
        Architecture => 50,
        SecretRotation => 51,
        ErrorPage => 52,
        Surface => 53,
        SurfaceAudience => 54,
        SurfaceView => 55,
        Extends => 56,
        Plan => 57,
        Design => 58,
        Value => 59,
        Modifier => 60,
        Expression => 61,
        _ => u16::MAX,
    }
}

/// Map a [`SemanticToken`] to its lower-case LSP standard token-type name.
fn token_name(c: &CapabilitySpec) -> &'static str {
    use lazuli_keywords::SemanticToken::*;
    match c.token {
        Keyword => "keyword",
        Type => "type",
        Function => "function",
        Namespace => "namespace",
        Decorator => "decorator",
        Modifier => "modifier",
        EnumMember => "enumMember",
        String => "string",
        Comment => "comment",
        Operator => "operator",
        Variable => "variable",
        Property => "property",
    }
}

/// Render the sigil facet for a table cell.
fn sigil_name(c: &CapabilitySpec) -> &'static str {
    match c.sigil {
        Some(Sigil::At) => "`@`",
        Some(Sigil::DottedKind) => "`.` (dotted kind)",
        None => "—",
    }
}

/// Escape a value for safe embedding inside a Markdown table cell: `|` would
/// break the column, and the cell content is otherwise plain text.
fn escape_cell(s: &str) -> String {
    if s.is_empty() {
        return "—".to_string();
    }
    s.replace('\\', "\\\\").replace('|', "\\|")
}

/// Render the `produces[]` cell: the diagnostic codes joined, each in
/// backticks. Empty → an em-dash.
fn render_produces(c: &CapabilitySpec) -> String {
    if c.produces.is_empty() {
        return "—".to_string();
    }
    let mut codes: Vec<&str> = c.produces.iter().map(|f| f.code).collect();
    codes.sort_unstable();
    codes.dedup();
    codes
        .iter()
        .map(|code| format!("`{}`", escape_cell(code)))
        .collect::<Vec<_>>()
        .join("<br>")
}

/// Project the registry to ordered [`Row`]s, sorted by (surface, context,
/// literal, scope) for full determinism.
fn rows() -> Vec<Row> {
    let mut rows: Vec<Row> = ALL
        .iter()
        .map(|c| Row {
            surface: c.surface,
            context: c.context,
            literal: c.literal,
            scope: c.scope,
            token: token_name(c),
            sigil: sigil_name(c),
            hover: escape_cell(c.hover),
            produces: render_produces(c),
        })
        .collect();
    rows.sort_by(|a, b| {
        surface_order(a.surface)
            .0
            .cmp(&surface_order(b.surface).0)
            .then_with(|| context_order(a.context).cmp(&context_order(b.context)))
            // Tie-break on the context label so #[non_exhaustive] contexts that
            // share the sentinel rank still group deterministically.
            .then_with(|| context_label(a.context).cmp(&context_label(b.context)))
            .then_with(|| a.literal.cmp(b.literal))
            .then_with(|| a.scope.cmp(b.scope))
    });
    rows
}

/// Build the full generated body (everything from [`BODY_SENTINEL`] to EOF),
/// returning the document text and the number of rows rendered.
fn render(rows: &[Row]) -> (String, usize) {
    let total = rows.len();
    let mut out = String::new();
    out.push_str(BODY_SENTINEL);
    out.push_str("\n\n");
    out.push_str(&format!(
        "_Generated from {total} capability rows across the `lazuli_keywords` registry._\n",
    ));

    // Group by surface, then context, preserving the sorted order.
    let mut by_surface: BTreeMap<(u8, &'static str), Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by_surface
            .entry(surface_order(r.surface))
            .or_default()
            .push(r);
    }

    for ((_, surface_label), surface_rows) in &by_surface {
        out.push_str(&format!("\n## {surface_label}\n"));

        let mut by_context: BTreeMap<(u16, String), Vec<&&Row>> = BTreeMap::new();
        for r in surface_rows {
            by_context
                .entry((context_order(r.context), context_label(r.context)))
                .or_default()
                .push(r);
        }

        for ((_, ctx_label), ctx_rows) in &by_context {
            out.push_str(&format!("\n### {ctx_label}\n\n"));
            out.push_str(
                "| keyword / literal | scope | semantic token | sigil | hover | produces |\n",
            );
            out.push_str("| --- | --- | --- | --- | --- | --- |\n");
            for r in ctx_rows {
                out.push_str(&format!(
                    "| `{}` | `{}` | `{}` | {} | {} | {} |\n",
                    escape_cell(r.literal),
                    escape_cell(r.scope),
                    r.token,
                    r.sigil,
                    r.hover,
                    r.produces,
                ));
            }
        }
    }
    (out, total)
}

/// Human-facing heading for a [`Context`]. `Debug` of a fieldless enum variant
/// is its identifier — stable and unique — which is exactly what the reader
/// needs to disambiguate same-literal/different-block rows. Using `Debug`
/// keeps this projector total over `#[non_exhaustive]` Context additions with
/// no parallel name table to forget to update: a new variant is named the
/// instant it appears in the registry.
fn context_label(c: Context) -> String {
    format!("{c:?}")
}

/// Assemble the full document text: header + generated body.
fn build_document() -> (String, usize) {
    let (body, total) = render(&rows());
    let mut doc = String::new();
    doc.push_str(HEADER);
    doc.push_str("\n\n");
    doc.push_str(&body);
    // Normalize to a single trailing newline.
    let trimmed = doc.trim_end();
    (format!("{trimmed}\n"), total)
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../tools/xtask; climb two levels to the root.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // xtask
    p.pop(); // tools
    p
}

/// Entry point for the `gen-keyword-reference` subcommand.
pub fn run(check: bool) -> Result<(), String> {
    let path = workspace_root().join(DOC_REL);
    let (fresh, total) = build_document();

    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    // Compare on normalized line endings so a CRLF checkout doesn't spuriously
    // diff against the LF the generator emits.
    let same = normalize(&committed) == normalize(&fresh);

    if check {
        if same {
            println!("gen-keyword-reference --check: {DOC_REL} is fresh ({total} rows).");
            Ok(())
        } else {
            Err(format!(
                "stale {DOC_REL} — run `cargo run -p xtask -- gen-keyword-reference`."
            ))
        }
    } else if same {
        println!("gen-keyword-reference: {DOC_REL} already fresh ({total} rows), no write.");
        Ok(())
    } else {
        std::fs::write(&path, &fresh).map_err(|e| format!("writing {}: {e}", path.display()))?;
        println!("gen-keyword-reference: wrote {total} rows to {DOC_REL}.");
        Ok(())
    }
}

/// Normalize line endings for a byte-exact-modulo-EOL comparison.
fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}
