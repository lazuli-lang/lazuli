//! FACE-PARITY harness — ctx-path contract (codegen side).
//!
//! Companion to `runtime/go/lazuli/readctx_parity_test.go`. Together they
//! gate the codegen↔runtime ctx-path face that bug #17 broke: the codegen
//! lowers `ctx.<head>.<rest>` to `lazuli.FromCtx("<head>.<rest>")`
//! (`emitter/command/effects_format.rs`, the `"ctx" =>` arm) and the
//! runtime `readCtx` must handle every such path or it 500s with "unknown
//! ctx path". `ctx.actor.id` shipped a 500 because no test asserted this.
//!
//! ## Single source of truth
//!
//! `runtime/go/lazuli/ctx_path_catalog.json` is the ONE list of canonical
//! ctx paths. The Go test asserts `catalog == readCtx-handled` (both
//! directions), so the catalog can never drift from the runtime. THIS test
//! asserts the other face: every ctx path the FRAMEWORK ITSELF lowers
//! (the literal ctx paths baked into the analyzer/codegen synth — me-mode
//! keys, scope bindings, owner-scope filters, lifecycle `ctx.now`, …) is
//! in that catalog. So: framework-emittable ⊆ catalog == readCtx-handled.
//!
//! ## Honesty about enumerability (read this before trusting the gate)
//!
//! The codegen's `"ctx" =>` arm emits `FromCtx("<tail>")` for WHATEVER an
//! author writes after `ctx.` — that author-controlled tail is NOT a
//! finite set, so this test CANNOT enumerate every byte a user could type
//! (a brand-new author-written `ctx.foo.bar` would lower to an unhandled
//! `FromCtx("foo.bar")` and slip past this test — caught only at runtime,
//! or by the future `CODEGEN-UNRESOLVED-BINDING-SOURCE` analyzer pass that
//! the audit ranks #2). What IS finite and enumerable is the set of ctx
//! paths the FRAMEWORK'S OWN synth emits — exactly the surface that grew
//! `actor.id`/`actor.user_id`/`actor.org_id` and bit us. This test pins
//! that surface to the catalog by scraping the literal ctx paths out of
//! the synth source (so a new framework-emitted ctx path forces a catalog
//! entry, which forces a `readCtx` arm via the Go test). See the FINAL
//! follow-up list for the author-tail gap.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/lazuli_codegen_go
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read(rel: &str) -> String {
    let p = workspace_root().join(rel);
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("cannot read {}: {e}", p.display()))
}

/// Parse the `paths` array out of the SoT catalog JSON. Dependency-free
/// (the crate has no serde): the file is hand-checked-in and simple, so we
/// slice the `"paths": [ ... ]` array and pull every quoted literal. The
/// `_comment` field is outside the array and never read.
fn catalog_paths() -> BTreeSet<String> {
    let raw = read("runtime/go/lazuli/ctx_path_catalog.json");
    let start = raw
        .find("\"paths\"")
        .expect("catalog JSON missing a \"paths\" key");
    let arr_start = raw[start..]
        .find('[')
        .map(|i| start + i)
        .expect("catalog \"paths\" has no opening [");
    let arr_end = raw[arr_start..]
        .find(']')
        .map(|i| arr_start + i)
        .expect("catalog \"paths\" has no closing ]");
    let body = &raw[arr_start + 1..arr_end];
    let mut out = BTreeSet::new();
    for piece in body.split(',') {
        let piece = piece.trim();
        if let Some(stripped) = piece.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            out.insert(stripped.to_owned());
        }
    }
    assert!(
        !out.is_empty(),
        "parsed zero paths from ctx_path_catalog.json — the parser or the file is broken"
    );
    out
}

/// Every framework synth source file that bakes in literal ctx paths the
/// codegen will lower through the `"ctx" =>` FromCtx arm. This is the
/// finite, framework-controlled half of the emittable set.
const SYNTH_CTX_PATH_SOURCES: &[&str] = &[
    "crates/lazuli_codegen_go/src/emitter/command/scope.rs",
    "crates/lazuli_codegen_go/src/emitter/query/filters.rs",
    "crates/lazuli_codegen_go/src/emitter/query/list.rs",
    "crates/lazuli_analyzer/src/conventions/me_mode.rs",
    "crates/lazuli_analyzer/src/lifecycle/mod.rs",
];

/// Pull ctx paths out of `lazuli.FromCtx("<path>")` and
/// `lazuli.FromCtxOwnedVia(..., "<ctx_path>")` string literals emitted by
/// the synth. For the OwnedVia form the ctx path is the LAST string
/// argument. We only collect concrete (no `{` interpolation placeholder)
/// literals — a `FromCtx("{tail}")` template is the generic author-tail
/// passthrough, not a framework-baked path, and is covered by the honesty
/// note above, not enumerable here.
/// Normalize a scraped literal. The synth embeds Go source with ESCAPED
/// quotes (`\"user.id\"`), so naive splitting on `"` leaves a trailing
/// backslash from the closing `\"`. Strip it. Also reject anything still
/// holding an interpolation placeholder.
fn norm_literal(lit: &str) -> Option<String> {
    let lit = lit.strip_suffix('\\').unwrap_or(lit);
    if lit.is_empty() || lit.contains('{') || lit.contains('\\') {
        return None;
    }
    Some(lit.to_owned())
}

fn scrape_fromctx_literals(src: &str, acc: &mut BTreeSet<String>) {
    for marker in ["lazuli.FromCtx(\"", "FromCtx(\""] {
        let mut from = 0;
        while let Some(i) = src[from..].find(marker) {
            let lit_start = from + i + marker.len();
            if let Some(j) = src[lit_start..].find('"') {
                let lit = &src[lit_start..lit_start + j];
                if let Some(n) = norm_literal(lit) {
                    acc.insert(n);
                }
                from = lit_start + j + 1;
            } else {
                break;
            }
        }
    }
    // FromCtxOwnedVia("<related>", "<owner>", "<ctx_path>") — ctx path is
    // the third literal. Scrape the third quoted arg of each call.
    let marker = "FromCtxOwnedVia(";
    let mut from = 0;
    while let Some(i) = src[from..].find(marker) {
        let call_start = from + i + marker.len();
        // Bound to the end of the call (next `)`), then take the 3rd literal.
        let close = src[call_start..]
            .find(')')
            .map(|c| call_start + c)
            .unwrap_or(src.len());
        let call = &src[call_start..close];
        let lits: Vec<&str> = call
            .match_indices('"')
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>()
            .chunks(2)
            .filter_map(|w| {
                if w.len() == 2 {
                    Some(&call[w[0] + 1..w[1]])
                } else {
                    None
                }
            })
            .collect();
        if let Some(ctx_path) = lits.get(2) {
            if let Some(n) = norm_literal(ctx_path) {
                acc.insert(n);
            }
        }
        from = close + 1;
    }
}

/// Pull ctx paths out of IR-level `ir::Path::from_segments(["ctx", "a",
/// "b"])` constructions (me-mode keys, lifecycle `ctx.now`). These lower
/// through the same `"ctx" =>` arm at codegen time, so their `a.b` tail is
/// equally emittable. We match the segment-list shape with a leading
/// `"ctx"` and join the remaining literals with `.`.
fn scrape_ctx_segment_paths(src: &str, acc: &mut BTreeSet<String>) {
    let marker = "from_segments([";
    let mut from = 0;
    while let Some(i) = src[from..].find(marker) {
        let list_start = from + i + marker.len();
        let close = src[list_start..]
            .find(']')
            .map(|c| list_start + c)
            .unwrap_or(src.len());
        let list = &src[list_start..close];
        // Collect the quoted segment literals in order.
        let segs: Vec<&str> = list
            .match_indices('"')
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>()
            .chunks(2)
            .filter_map(|w| {
                if w.len() == 2 {
                    Some(&list[w[0] + 1..w[1]])
                } else {
                    None
                }
            })
            .collect();
        if segs.first() == Some(&"ctx") && segs.len() > 1 {
            acc.insert(segs[1..].join("."));
        }
        from = close + 1;
    }
}

/// The framework-emittable ctx-path set, scraped from the synth sources.
fn framework_emittable_ctx_paths() -> BTreeSet<String> {
    let mut acc = BTreeSet::new();
    for rel in SYNTH_CTX_PATH_SOURCES {
        let src = read(rel);
        scrape_fromctx_literals(&src, &mut acc);
        scrape_ctx_segment_paths(&src, &mut acc);
    }
    acc
}

#[test]
fn framework_emittable_ctx_paths_are_in_the_catalog() {
    let catalog = catalog_paths();
    let emittable = framework_emittable_ctx_paths();

    // Sanity: the scraper must actually find the known framework paths, or
    // a refactor silently emptied it (false negative). Pin the floor.
    assert!(
        emittable.contains("actor.user_id") && emittable.contains("actor.org_id"),
        "ctx-path scraper found neither actor.user_id nor actor.org_id in the synth \
         sources — the scraper has gone stale (false-negative). Found: {emittable:?}"
    );

    let missing: Vec<&String> = emittable.difference(&catalog).collect();
    assert!(
        missing.is_empty(),
        "ctx-path FACE-PARITY violation (bug #17 class): the framework synth lowers \
         these ctx path(s) to lazuli.FromCtx(...) but they are ABSENT from the SoT \
         catalog (runtime/go/lazuli/ctx_path_catalog.json). Every catalog path is \
         proven handled by readCtx (readctx_parity_test.go); a path the framework \
         emits but is NOT in the catalog has no proven runtime arm and risks a 500. \
         Add each to the catalog AND a `case` to readCtx:\n  - {}",
        missing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}

#[test]
fn ctx_lowering_arm_still_emits_fromctx() {
    // Pins the contract shape this whole harness rests on: the codegen's
    // ctx head still lowers to `lazuli.FromCtx("<tail>")`. If this arm is
    // refactored (e.g. renamed to a typed accessor), the catalog/readCtx
    // contract changes shape and this harness must be revisited — fail
    // loudly rather than silently guard the wrong thing.
    let effects = read("crates/lazuli_codegen_go/src/emitter/command/effects_format.rs");
    assert!(
        effects.contains("\"ctx\" => format!(\"lazuli.FromCtx(\\\"{tail}\\\")\")"),
        "the ctx-lowering arm in effects_format.rs no longer matches \
         `\"ctx\" => format!(\"lazuli.FromCtx(\\\"{{tail}}\\\")\")`. The ctx-path \
         face-parity harness assumes ctx.<path> lowers to FromCtx(\"<tail>\"); if the \
         lowering changed, update readctx_parity_test.go + ctx_path_catalog.json + \
         this harness to match the new shape."
    );
}
