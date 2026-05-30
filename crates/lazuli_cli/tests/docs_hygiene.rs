//! Docs anti-stale gate (tier 2 of the docs-freshness mechanism).
//!
//! Every maintained `docs/*.md` is *continuously verified* against the repo:
//! every `path/file.ext[:line]` citation must resolve to a real file, and every
//! `[text](relative.md)` link must point at an existing doc. A doc that cites a
//! moved/renamed source file, or links to a deleted doc, fails the build — so
//! prose docs can no longer silently rot the way the 43 dead citations this
//! gate first caught did.
//!
//! Exemptions (deliberate, narrow):
//! - `runtime/…` — the Go/TS runtime is not built yet (`docs/architecture.md`);
//!   citations are forward-looking.
//! - `docs/proposals/…` files — archived design snapshots (the live archive
//!   moved to lazuli-ops); they are frozen, not maintained, so they are not
//!   scanned and citations *to* them are exempt.
//! - tokens containing `...` — illustrative elided paths, not real citations.
//!
//! Companion gates: generated catalogs (`catalog_reference_fresh`) and keyword
//! reference (`keyword_reference_fresh`) cover the docs that *can* be generated.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is .../crates/lazuli_cli; climb two levels.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // lazuli_cli
    p.pop(); // crates
    p
}

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_md(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

const CITE_PREFIXES: &[&str] = &[
    "crates/",
    "tools/",
    "docs/",
    "examples/",
    "lazurite/",
    "editors/",
    "runtime/",
];

/// If `tok` looks like a repo file citation, return the file path (sans
/// `:line` suffix); else `None`.
fn citation_path(tok: &str) -> Option<&str> {
    if !CITE_PREFIXES.iter().any(|p| tok.starts_with(p)) || tok.contains("...") {
        return None;
    }
    // Strip any `:` suffix — a `file.rs:215` line ref OR a `file.rs:Symbol`
    // item ref both cite the file. Repo paths are drive-letter-free, so the
    // first `:` always begins the suffix.
    let path = tok.split(':').next().unwrap_or(tok);
    // Require a non-empty alphanumeric extension on the last segment, so a
    // placeholder like `docs/grammar.` (a split `docs/grammar.<surface>.md`)
    // is not mistaken for a real file citation.
    let last = path.rsplit('/').next().unwrap_or("");
    match last.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()) => {
            Some(path)
        }
        _ => None,
    }
}

/// Split prose into path-shaped tokens (chars valid inside a repo path).
fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| {
        !(c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | ':' | '-' | '_'))
    })
    .filter(|t| !t.is_empty())
}

/// Extract `](target)` link targets.
fn md_link_targets(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(p) = rest.find("](") {
        let after = &rest[p + 2..];
        if let Some(q) = after.find(')') {
            out.push(&after[..q]);
            rest = &after[q + 1..];
        } else {
            break;
        }
    }
    out
}

#[test]
fn docs_citations_and_links_resolve() {
    let root = workspace_root();
    let mut docs = Vec::new();
    collect_md(&root.join("docs"), &mut docs);
    assert!(!docs.is_empty(), "no docs/*.md found under {}", root.display());

    let mut violations: Vec<String> = Vec::new();

    for doc in &docs {
        let rel = doc.strip_prefix(&root).unwrap_or(doc).to_string_lossy().replace('\\', "/");
        // archived proposals are frozen snapshots, not maintained — skip.
        if rel.contains("/proposals/") {
            continue;
        }
        let text = std::fs::read_to_string(doc).unwrap();

        for tok in tokens(&text) {
            if let Some(path) = citation_path(tok) {
                if path.starts_with("runtime/") || path.starts_with("docs/proposals/") {
                    continue; // forward-looking runtime / archived proposals
                }
                if !root.join(path).exists() {
                    violations.push(format!("{rel}: dead citation `{path}`"));
                }
            }
        }

        for target in md_link_targets(&text) {
            let clean = target.split('#').next().unwrap_or(target);
            if !clean.ends_with(".md") || clean.starts_with("http") {
                continue;
            }
            if clean.starts_with("docs/proposals/") || clean.contains("proposals/") {
                continue;
            }
            let resolved = doc.parent().unwrap().join(clean);
            if !resolved.exists() {
                violations.push(format!("{rel}: dead link `{target}`"));
            }
        }
    }

    if !violations.is_empty() {
        violations.sort();
        panic!(
            "docs drift — {} dead citation(s)/link(s). A cited file moved/renamed, \
             or a linked doc was deleted. Fix the reference (or move the source \
             back):\n  {}",
            violations.len(),
            violations.join("\n  "),
        );
    }
}
