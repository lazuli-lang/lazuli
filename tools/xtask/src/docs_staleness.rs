//! Docs anti-stale mechanism (tier 3) — a *self-maintaining* freshness check.
//!
//! Instead of a hand-maintained `last_reviewed:` date (which itself goes stale
//! the moment someone forgets to bump it), this derives freshness from git: for
//! each `docs/*.md`, it compares the doc's last-commit time against the
//! last-commit time of every source file the doc CITES. If a cited file changed
//! *after* the doc was last touched, the doc is flagged "potentially stale —
//! the code it describes moved on."
//!
//! Run periodically (a nightly/weekly CI job, not every build):
//! `cargo run -p xtask -- docs-staleness [--check]`. With `--check` it exits
//! non-zero when any doc is stale; otherwise it is a report. It is NOT a
//! per-build gate, because git-mtime depends on full history (shallow clones
//! lack it). The hard per-build gates are `docs_hygiene` (citations/links
//! resolve) and `catalog_reference_fresh` (generated docs are current).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // xtask
    p.pop(); // tools
    p
}

fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_md(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("md") {
            out.push(p);
        }
    }
}

/// Last-commit unix time of `rel_path` (relative to root), via git. `None` if
/// git has no record (untracked / shallow history).
fn git_mtime(root: &Path, rel_path: &str, cache: &mut HashMap<String, Option<u64>>) -> Option<u64> {
    if let Some(v) = cache.get(rel_path) {
        return *v;
    }
    let out = Command::new("git")
        .current_dir(root)
        .args(["log", "-1", "--format=%ct", "--", rel_path])
        .output()
        .ok();
    let v = out.and_then(|o| {
        String::from_utf8_lossy(&o.stdout)
            .trim()
            .parse::<u64>()
            .ok()
    });
    cache.insert(rel_path.to_string(), v);
    v
}

/// Source-file citations in a doc (only files that exist and are real source —
/// not the doc itself, not runtime/ or proposals/).
fn cited_source_files(text: &str, root: &Path) -> Vec<String> {
    const PREFIXES: &[&str] = &["crates/", "tools/", "examples/", "lazurite/", "editors/"];
    let mut out = Vec::new();
    for tok in text.split(|c: char| {
        !(c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | ':' | '-' | '_'))
    }) {
        if tok.is_empty() || tok.contains("...") || !PREFIXES.iter().any(|p| tok.starts_with(p)) {
            continue;
        }
        let path = tok.split(':').next().unwrap_or(tok);
        let last = path.rsplit('/').next().unwrap_or("");
        let has_ext = last
            .rsplit_once('.')
            .is_some_and(|(_, e)| !e.is_empty() && e.chars().all(|c| c.is_ascii_alphanumeric()));
        if has_ext && root.join(path).exists() && !out.contains(&path.to_string()) {
            out.push(path.to_string());
        }
    }
    out
}

/// Entry point for the `docs-staleness` subcommand.
pub fn run(check: bool) -> Result<(), String> {
    let root = workspace_root();
    let mut docs = Vec::new();
    collect_md(&root.join("docs"), &mut docs);
    docs.sort();

    let mut cache: HashMap<String, Option<u64>> = HashMap::new();
    let mut stale: Vec<String> = Vec::new();
    let mut scanned = 0u32;

    for doc in &docs {
        let rel = doc
            .strip_prefix(&root)
            .unwrap_or(doc)
            .to_string_lossy()
            .replace('\\', "/");
        if rel.contains("/proposals/") {
            continue;
        }
        let Some(doc_mtime) = git_mtime(&root, &rel, &mut cache) else {
            continue;
        };
        scanned += 1;
        let text = std::fs::read_to_string(doc).unwrap_or_default();
        let mut newer: Vec<String> = Vec::new();
        for cited in cited_source_files(&text, &root) {
            if let Some(code_mtime) = git_mtime(&root, &cited, &mut cache) {
                // 1-day grace so a same-day edit pair doesn't false-positive.
                if code_mtime > doc_mtime + 86_400 {
                    newer.push(cited);
                }
            }
        }
        if !newer.is_empty() {
            stale.push(format!("{rel} — cited code changed since the doc: {}", newer.join(", ")));
        }
    }

    if stale.is_empty() {
        println!("docs-staleness: {scanned} docs scanned, all fresh vs cited code.");
        return Ok(());
    }
    println!("docs-staleness: {} of {scanned} doc(s) potentially stale:", stale.len());
    for s in &stale {
        println!("  - {s}");
    }
    if check {
        Err(format!("{} stale doc(s) — review and update.", stale.len()))
    } else {
        Ok(())
    }
}
