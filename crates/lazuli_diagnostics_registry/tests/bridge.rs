//! The convergence bridge asserts (Wave C1).
//!
//! These tests are the whole point of `lazuli_diagnostics_registry`: they
//! link the two single-source manifests — the capability table
//! ([`lazuli_keywords::ALL`] + [`lazuli_keywords::GLOBAL_DIAGNOSTICS`]) and the
//! doctor rule `CODE` consts — and prove they cohere.
//!
//! ## Live-code enumeration (test-time grep)
//!
//! `lazuli_doctor` has no central runtime registry of its codes; each rule
//! co-locates a `pub const CODE: &'static str = "…"` (a few rules use a
//! differently-named `*CODE*` const). The H1 proven-complete test established
//! the precedent of scanning sibling-crate *source* at test time to enumerate
//! a hand-distributed set; we do the same here, scanning
//! `crates/lazuli_doctor/src/**` for the `const …CODE…: &'static str = "…"`
//! pattern and collecting the literal value.
//!
//! Test modules (`#[cfg(test)]`) inside rule files also declare string
//! literals, but only *real* rules declare a `CODE` const, and those consts
//! sit at module scope (above the file's `#[cfg(test)] mod tests`), so the
//! line-pattern scan picks up exactly the live codes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use lazuli_diagnostics_registry::{claimed_codes, claimed_facets};
use lazuli_doctor::{DoctorSeverity, RuleCategory};

/// Root of the `lazuli_doctor` rule source we scan for `CODE` consts.
fn doctor_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <root>/crates/lazuli_diagnostics_registry
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("lazuli_doctor")
        .join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Extract every diagnostic `CODE` const value declared in the doctor source.
///
/// Matches lines of the form
/// `… const <NAME-containing-CODE>: &'static str = "<value>";` and returns the
/// `<value>` set. The `<NAME-containing-CODE>` guard is what restricts the scan
/// to diagnostic-code consts (vs. arbitrary `&'static str` consts like scope
/// leaves or message templates).
fn extract_codes(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim_start();
        // Must be a const declaration whose name contains `CODE`.
        if !t.starts_with("const ") && !t.starts_with("pub const ") {
            continue;
        }
        let Some(colon) = t.find(':') else { continue };
        let name = &t[..colon];
        if !name.contains("CODE") {
            continue;
        }
        let ty_and_val = &t[colon..];
        if !ty_and_val.contains("&'static str") && !ty_and_val.contains("& 'static str") {
            continue;
        }
        // Pull the string literal after `=`.
        let Some(eq) = ty_and_val.find('=') else {
            continue;
        };
        let rhs = &ty_and_val[eq + 1..];
        let Some(open) = rhs.find('"') else { continue };
        let after = &rhs[open + 1..];
        let Some(close) = after.find('"') else {
            continue;
        };
        out.push(after[..close].to_string());
    }
    out
}

/// Scan the doctor source and return the deduped set of live `CODE` const
/// values. Asserts the scan found a healthy population so a broken extractor
/// can't make the gate vacuous.
fn live_codes() -> BTreeSet<String> {
    let root = doctor_src_root();
    assert!(
        root.is_dir(),
        "lazuli_doctor source root not found at {} — the bridge scan cannot run",
        root.display()
    );

    let mut files = Vec::new();
    rust_files(&root, &mut files);
    assert!(
        files.len() > 50,
        "expected to scan 100+ doctor rule files, found only {} — the scan path is wrong",
        files.len()
    );

    let mut codes = BTreeSet::new();
    for file in &files {
        let src = fs::read_to_string(file).unwrap_or_default();
        for c in extract_codes(&src) {
            codes.insert(c);
        }
    }

    assert!(
        codes.len() > 120,
        "extractor found only {} codes — expected ~174 (a broken scan would make these gates vacuous)",
        codes.len()
    );
    codes
}

/// **Resolvable** — every claimed facet points at a live rule, with a
/// coherent category mirror and a well-formed base-severity mirror.
#[test]
fn every_claimed_facet_resolves() {
    let live = live_codes();
    let mut problems: Vec<String> = Vec::new();

    for facet in claimed_facets() {
        // (a) the code is a live lazuli_doctor CODE const.
        if !live.contains(facet.code) {
            problems.push(format!(
                "`{}` is claimed by a capability/GLOBAL but is NOT a live lazuli_doctor CODE const \
                 (typo, or a code that was removed/renamed in the rule file)",
                facet.code
            ));
            continue;
        }

        // (b) the mirrored category matches the canonical derivation.
        let canonical = RuleCategory::from_code_prefix(facet.code).as_str();
        if facet.category != canonical {
            problems.push(format!(
                "`{}` mirrors category `{}` but RuleCategory::from_code_prefix says `{}`",
                facet.code, facet.category, canonical
            ));
        }

        // (c) the mirrored base severity is a well-formed DoctorSeverity.
        if DoctorSeverity::parse(facet.base_severity).is_none() {
            problems.push(format!(
                "`{}` has base_severity `{}` which is not a valid DoctorSeverity \
                 (expected error|warning|info|hint)",
                facet.code, facet.base_severity
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "DIAGNOSTIC-REGISTRY RESOLVABLE failures ({}):\n  - {}",
        problems.len(),
        problems.join("\n  - ")
    );
}

/// **Complete** — every live `CODE` const is claimed exactly once: no ORPHAN
/// (a diagnostic with no capability/GLOBAL home) and no DUPLICATE (a code
/// claimed by two capabilities, or by both a capability and GLOBAL).
///
/// This is the assertion that makes the per-capability convergence
/// enforceable. A future diagnostic added to `lazuli_doctor` without a
/// matching `produces[]` row (or a documented `GLOBAL_DIAGNOSTICS` entry) fails
/// this test.
#[test]
fn every_live_code_is_claimed_exactly_once() {
    let live = live_codes();

    // Count how many times each code is claimed (across all produces[] + GLOBAL).
    let mut claim_count: BTreeMap<&str, usize> = BTreeMap::new();
    for code in claimed_codes() {
        *claim_count.entry(code).or_insert(0) += 1;
    }

    // DUPLICATE: any code claimed more than once.
    let duplicates: Vec<String> = claim_count
        .iter()
        .filter(|&(_, &n)| n > 1)
        .map(|(code, n)| format!("`{code}` claimed {n}× (must be exactly once)"))
        .collect();

    // ORPHAN: a live code that nothing claims.
    let claimed: BTreeSet<&str> = claim_count.keys().copied().collect();
    let orphans: Vec<String> = live
        .iter()
        .filter(|c| !claimed.contains(c.as_str()))
        .map(|c| {
            format!(
                "`{c}` is a live diagnostic with NO capability home — add it to the producing \
                 capability's `produces` in lazuli_keywords::ALL, or to GLOBAL_DIAGNOSTICS if it \
                 is genuinely cross-cutting (category `{}`)",
                RuleCategory::from_code_prefix(c).as_str()
            )
        })
        .collect();

    // STALE: a claimed code that is not live (also caught by `resolvable`, but
    // reported here so completeness failures are self-contained).
    let stale: Vec<String> = claimed
        .iter()
        .filter(|c| !live.contains(**c))
        .map(|c| format!("`{c}` is claimed but is not a live CODE const (stale claim)"))
        .collect();

    let mut problems = Vec::new();
    problems.extend(orphans);
    problems.extend(duplicates);
    problems.extend(stale);

    assert!(
        problems.is_empty(),
        "DIAGNOSTIC-REGISTRY COMPLETE failures ({}) — {} live codes, {} distinct claims:\n  - {}",
        problems.len(),
        live.len(),
        claimed.len(),
        problems.join("\n  - ")
    );
}

/// Guard: the GLOBAL bucket is a *documented home*, not a dumping ground —
/// it must stay small relative to the capability-attributed set. If this trips,
/// a code was parked in GLOBAL that should have a real capability home.
#[test]
fn global_bucket_stays_minority() {
    let global = lazuli_keywords::GLOBAL_DIAGNOSTICS.len();
    let attributed: usize = lazuli_keywords::ALL.iter().map(|c| c.produces.len()).sum();
    assert!(
        global < attributed,
        "GLOBAL_DIAGNOSTICS ({global}) should be a minority vs capability-attributed codes \
         ({attributed}) — a code may have been parked in GLOBAL that has a real capability home"
    );
}
