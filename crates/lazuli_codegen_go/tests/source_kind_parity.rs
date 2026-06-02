//! FACE-PARITY harness — Source-kind contract (codegen side).
//!
//! Companion to `TestResolveSourceHandlesEveryEmittableKind` in
//! `runtime/go/lazuli/readctx_parity_test.go`. The codegen lowers binding
//! RHS expressions into a fixed set of `lazuli.From*` Source constructors
//! (`effect.go`); the runtime `resolveSource` switch (`handle.go`) must
//! have a non-stub arm for each. A `From*` the codegen emits with no
//! resolveSource arm hits `default` ("unknown source kind") → 500; a stub
//! arm (today: `sourceTarget` → 501) → 501. This is bug #10 / #18-adjacent.
//!
//! The runtime side enumerates the kinds it expects (the Go
//! `emittableSourceKinds()` map) and probes each through resolveSource.
//! THIS test pins that the codegen really only emits THOSE constructors —
//! so the Go map cannot silently lag behind a newly-emitted constructor.
//! Mechanism: scrape every `lazuli.From*(` the codegen emits and assert
//! the set equals the known set (which the Go map mirrors one-for-one).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// The Source constructors the codegen is known to emit. This MUST stay
/// one-for-one with the Go `emittableSourceKinds()` map in
/// `runtime/go/lazuli/readctx_parity_test.go`. Adding a constructor here
/// without a runtime arm makes the Go test fail; emitting a new
/// constructor in codegen without listing it here makes THIS test fail.
const KNOWN_EMITTED_CONSTRUCTORS: &[&str] = &[
    "FromConst",
    "FromCtx",
    "FromCtxOwnedVia",
    "FromFn",
    "FromInput",
    "FromInputOptional",
    "FromTarget",
];

/// Recursively collect `lazuli.From<Name>(` constructor names emitted by
/// the codegen `.rs` sources (excluding test files, whose assertions name
/// constructors that are not necessarily emitted by production paths).
fn collect_emitted_constructors(dir: &Path, acc: &mut BTreeSet<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_emitted_constructors(&path, acc);
            continue;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.ends_with(".rs") {
            continue;
        }
        // Skip test modules: `*_test*.rs`, `*_tests.rs`, and `tests.rs`.
        if name.contains("test") {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap_or_default();
        scrape_constructors(&src, acc);
    }
}

fn scrape_constructors(src: &str, acc: &mut BTreeSet<String>) {
    let marker = "lazuli.From";
    let mut from = 0;
    while let Some(i) = src[from..].find(marker) {
        let start = from + i + "lazuli.".len();
        // Read the identifier up to the `(`.
        let rest = &src[start..];
        if let Some(paren) = rest.find('(') {
            let ident = &rest[..paren];
            if ident.chars().all(|c| c.is_alphanumeric()) && ident.starts_with("From") {
                acc.insert(ident.to_owned());
            }
            from = start + paren;
        } else {
            break;
        }
    }
}

#[test]
fn codegen_emits_only_known_source_constructors() {
    let mut emitted = BTreeSet::new();
    collect_emitted_constructors(&workspace_root().join("crates/lazuli_codegen_go/src"), &mut emitted);

    let known: BTreeSet<String> = KNOWN_EMITTED_CONSTRUCTORS.iter().map(|s| s.to_string()).collect();

    // Floor sanity: the scraper must find the core constructors, else it
    // has gone stale (false negative).
    assert!(
        emitted.contains("FromConst") && emitted.contains("FromCtx"),
        "source-kind scraper found neither FromConst nor FromCtx in codegen src — \
         scraper stale (false-negative). Found: {emitted:?}"
    );

    let unknown: Vec<&String> = emitted.difference(&known).collect();
    assert!(
        unknown.is_empty(),
        "source-kind FACE-PARITY drift: the codegen emits Source constructor(s) NOT in \
         KNOWN_EMITTED_CONSTRUCTORS. Each emitted `lazuli.From*` must (a) be listed here \
         and (b) have a probed arm in the Go `emittableSourceKinds()` map + a non-stub \
         `resolveSource` arm. Add the new constructor to both faces:\n  - {}",
        unknown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  - ")
    );

    // Reverse: a known constructor never found in source is stale debt —
    // delete it so the list stays honest (mirrors the keyword-parity
    // stale-allowlist hygiene assertion).
    let stale: Vec<&String> = known.difference(&emitted).collect();
    assert!(
        stale.is_empty(),
        "KNOWN_EMITTED_CONSTRUCTORS lists constructor(s) the codegen no longer emits — \
         delete them (and prune the Go emittableSourceKinds map) so the list stays \
         honest:\n  - {}",
        stale.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  - ")
    );
}
