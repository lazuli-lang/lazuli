//! Proven-complete drift gate (Wave H1).
//!
//! This test is the whole point of the `lazuli_keywords` registry: it
//! EXTRACTS the keyword literals the hand-rolled parser actually
//! recognizes from `crates/lazuli_syntax/src/**` and asserts that **every
//! parser keyword literal appears in `lazuli_keywords::ALL`** (after
//! reduction to its head token).
//!
//! If a future change adds a keyword to the parser but forgets to add the
//! matching [`CapabilitySpec`] row to the registry, `cargo test` fails and
//! names the missing literal. That is the anti-drift guarantee H1 buys:
//! the registry can never silently fall behind the parser.
//!
//! ## How extraction works
//!
//! The parser is 248 scattered `starts_with` / `strip_prefix` / `==` /
//! match-arm call sites across ~30 files (no lexer — "each parser IS the
//! spec"). The extractor regex-scans the parser source for string-literal
//! arguments to that scanner family:
//!
//! * `.starts_with("X ...")`
//! * `.strip_prefix("X ...")`
//! * `== "X"` / `"X" ==`
//! * `"X" =>`  (first-token match arms)
//!
//! Each extracted literal is reduced to its **head token** (the first
//! whitespace-delimited word; dotted kinds like `query.list` keep their
//! dot), then checked against the registry's literal set. A literal whose
//! head is neither a registry literal nor in the explicit [`ALLOWLIST`]
//! fails the test.
//!
//! ## What the ALLOWLIST holds (documented inline below)
//!
//! The scan deliberately over-captures — it picks up string literals that
//! are NOT keywords: closed-catalog VALUES the registry models separately,
//! example/fixture identifiers in parser test-adjacent helpers, multi-word
//! phrase tails, operator/punctuation fragments, and string-content
//! matches. Every such literal is enumerated in [`ALLOWLIST`] with a
//! reason. Keeping the allowlist explicit (rather than a broad regex
//! filter) is what makes the gate trustworthy: a new keyword can't hide
//! behind a loose exclusion.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_keywords::ALL;

/// Non-keyword literals the scanner picks up. Each entry is a head token
/// the extractor produced that is intentionally NOT a registry keyword.
///
/// Grouped by reason. When you add a parser keyword, it should land in
/// `ALL`, NOT here — this list is only for genuinely-non-keyword literals.
const ALLOWLIST: &[&str] = &[
    // ── decorator/argument fragments with trailing punctuation that the
    //    head-token reducer can't cleanly map (the decorator itself IS in
    //    ALL as `@cap`/`@key`/etc.; these are call-form fragments) ──
    "@cap.Encrypted(key:@key.tenant)", // a literal example arg, not a keyword
    "sanitize_html(",                  // builtin transform call fragment
    "sum(",                            // aggregate-fn call fragment
    "covers_pii_",                     // prefix fragment (the predicate is `covers_pii`)
    "covers_pii:",                     // key-form of the `covers_pii` predicate
    "signed_token:",                   // session-token arg key
    "signed_token",                    // session-token arg (value form)
    "max_recursion:",                  // arg key (modifier `max_recursion` is in ALL)
    "max_size:",                       // arg key (statement `max_size` is in ALL)
    "read:",                           // policy-action key (`read` action is a value)
    "write:",                          // policy-action key (`write` action is a value)
    "on_unmet",                        // invariant arg fragment (no trailing space form)
    // ── type-literal prefixes matched during expression parsing (these
    //    are TYPE names, covered by the `#types` tmLanguage rule, not the
    //    keyword catalog) ──
    "Bool",
    "Enum",
    "Int",
    // ── closed-catalog VALUES not enumerated as Context::Value rows
    //    (highlighted generically by constant.language rules; not
    //    keywords) ──
    "select",    // SQL/projection value fragment
    "open",      // drawer-trigger value (`on open`)
    "cap",       // `@cap` namespace sub-token bare in a match arm
    "*",         // bare wildcard token (constant.language.wildcard.lazuli) — a value
    "4xx",       // error-status range literal — a value, not a keyword
    "5xx",       // error-status range literal — a value, not a keyword
    "utf8_safe", // sanitize-flag value fragment
    // ── RETIRED keywords: the parser still RECOGNIZES these solely to emit
    //    a retirement hard-error (E-CONTEXT-RETIRED / E-ATTACH-CTX-RETIRED /
    //    E-WORKFLOW-RETIRED). They are deliberately NOT live registry
    //    keywords — the LSP keyword_surface_parity `RETIRED_FEATURE_KEYWORDS`
    //    check would conflict if they were. Allow-listed here, NOT added to
    //    ALL. ──
    "context",    // retired feature-level `context "..."` (now `<feature>.ctx.md`)
    "attach_ctx", // retired feature-level `attach_ctx "..."` (now `<feature>.ctx.md` convention)
    "workflow",   // retired `workflow` block (now lifecycle/command effects)
    // ── reference-path roots / dotted references (NOT standalone keywords;
    //    highlighted as support.variable.context / reference paths) ──
    "row",         // `row.<field>` context-variable root
    "tools.calls", // dotted reference path `tools.calls`
    // ── example / fixture identifiers that appear in parser-internal
    //    sample strings or doc-adjacent `==` comparisons (NOT language
    //    keywords; they are user-domain names used in inline checks) ──
    "account",
    "admin_only",
    "capture_lead",
    "choose_role",
    "customer",
    "external_id",
    "host_only",
    "id",
    "is_high_value",
    "lifetime_value",
    "locale_negotiate",
    "metadata",
    "no_jump_more_than_one",
    "summarize_customer",
    "terminal_immutable",
];

/// Files whose literals are NOT parser spec — scan-excluded. Anything
/// ending in `_tests.rs` is a unit-test module, and `error.rs` holds
/// diagnostic-message string content (not keyword recognition).
fn is_excluded(path: &Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    name.ends_with("_tests.rs") || name == "error.rs"
}

fn parser_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/lazuli_keywords
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("lazuli_syntax")
        .join("src")
        .join("parser")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") && !is_excluded(&path) {
            out.push(path);
        }
    }
}

/// Pull every double-quoted string literal that is the argument to one of
/// the recognized scanner forms. Returns the RAW literal contents.
///
/// All slicing is anchored on byte offsets returned by `str::find`, which
/// are always valid char boundaries — so this is UTF-8-safe even though
/// the parser source contains multibyte characters (em-dashes in docs).
fn extract_literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();

    // Forward-reading forms: the literal FOLLOWS the marker.
    for marker in [".starts_with(\"", ".strip_prefix(\"", "== \""] {
        let mut from = 0;
        while let Some(rel) = src[from..].find(marker) {
            let start = from + rel + marker.len(); // first byte of literal content
            if let Some(end_rel) = find_string_end(&src[start..]) {
                out.push(src[start..start + end_rel].to_string());
            }
            from = start;
        }
    }

    // Match-arm form: `"literal" =>` — the literal PRECEDES the marker.
    let arm = "\" =>";
    let mut j = 0;
    while let Some(rel) = src[j..].find(arm) {
        let close = j + rel; // byte index of the closing quote (valid boundary)
        if let Some(open) = src[..close].rfind('"') {
            // Single-line literal only (skip anything spanning a newline).
            let inner = &src[open + 1..close];
            if !inner.contains('\n') {
                out.push(inner.to_string());
            }
        }
        j = close + arm.len();
    }
    out
}

/// Given the source AFTER an opening quote, return the byte offset of the
/// closing quote (handling `\"` escapes). `None` if unterminated on line.
fn find_string_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            b'\n' => return None,
            _ => i += 1,
        }
    }
    None
}

/// Reduce a raw parser literal to its head token for membership checking.
/// Head = the first whitespace-delimited word, with trailing `.`/`:`
/// stripped (so reference-path prefixes like `filters.` map to `filters`).
fn head_token(literal: &str) -> String {
    let first = literal.split_whitespace().next().unwrap_or("");
    first.trim_end_matches(['.', ':']).to_string()
}

#[test]
fn every_parser_keyword_literal_is_in_the_registry() {
    let registry: BTreeSet<&str> = ALL.iter().map(|c| c.literal).collect();
    let allow: BTreeSet<&str> = ALLOWLIST.iter().copied().collect();

    let root = parser_src_root();
    assert!(
        root.is_dir(),
        "parser source root not found at {} — the proven-complete scan cannot run",
        root.display()
    );

    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(
        files.len() > 20,
        "expected to scan ~30 parser files, found only {} — the scan path is wrong",
        files.len()
    );

    let mut all_literals: BTreeSet<String> = BTreeSet::new();
    for file in &files {
        let src = fs::read_to_string(file).unwrap_or_default();
        for lit in extract_literals(&src) {
            all_literals.insert(lit);
        }
    }

    // Sanity: the scan must actually find a healthy population of literals,
    // otherwise a broken extractor would make the test vacuously pass.
    assert!(
        all_literals.len() > 150,
        "extractor found only {} literals — expected 200+ (broken scan would make this gate vacuous)",
        all_literals.len()
    );

    let mut missing: Vec<String> = Vec::new();
    for lit in &all_literals {
        let head = head_token(lit);
        if head.is_empty() {
            continue;
        }
        // A literal is covered if its head token, OR the full literal
        // (for dotted kinds / decorators), is a registry literal — or if
        // the raw literal / head is explicitly allow-listed.
        let covered = registry.contains(head.as_str())
            || registry.contains(lit.as_str())
            || registry.contains(head_token(lit.as_str()).as_str())
            || allow.contains(lit.as_str())
            || allow.contains(head.as_str());
        if !covered {
            missing.push(format!("`{lit}` (head `{head}`)"));
        }
    }

    assert!(
        missing.is_empty(),
        "PARSER KEYWORD DRIFT — these keyword literals are recognized by the parser \
         (crates/lazuli_syntax/src/parser/**) but are MISSING from lazuli_keywords::ALL. \
         Add a CapabilitySpec row for each (or, if it is genuinely a non-keyword literal — a \
         closed-catalog value, fixture identifier, or operator fragment — add it to the \
         ALLOWLIST in this test with a reason).\n  - {}",
        missing.join("\n  - ")
    );
}

/// Guard: the registry must not regress to empty or near-empty, and every
/// row must carry a non-empty literal + an (empty, in H1) `produces`.
#[test]
fn registry_is_populated_and_h1_shaped() {
    assert!(
        ALL.len() > 200,
        "registry has only {} rows — expected 200+ after the H1 port",
        ALL.len()
    );
    let mut seen: HashSet<(&str, lazuli_keywords::Context)> = HashSet::new();
    for spec in ALL {
        assert!(!spec.literal.is_empty(), "empty literal in registry");
        assert!(!spec.scope.is_empty(), "empty scope for `{}`", spec.literal);
        // Wave C1 backfilled the `produces` facet: each `DiagnosticFacet`
        // must carry a non-empty code/base_severity/category triple. The
        // code↔capability↔category coherence + completeness asserts live in
        // the `lazuli_diagnostics_registry` bridge crate (над `lazuli_doctor`);
        // here we only guard the in-leaf shape.
        for facet in spec.produces {
            assert!(
                !facet.code.is_empty(),
                "`{}` has a produces facet with an empty code",
                spec.literal
            );
            assert!(
                !facet.base_severity.is_empty(),
                "`{}` produces `{}` with an empty base_severity",
                spec.literal,
                facet.code
            );
            assert!(
                !facet.category.is_empty(),
                "`{}` produces `{}` with an empty category",
                spec.literal,
                facet.code
            );
        }
        // Context-as-data: each (literal, context) pair must be unique. A
        // duplicate is a copy-paste bug — two rows would generate two
        // identical tmLanguage alternations / LSP completion items.
        assert!(
            seen.insert((spec.literal, spec.context)),
            "duplicate (literal, context) row: ({:?}, {:?})",
            spec.literal,
            spec.context
        );
    }
}
