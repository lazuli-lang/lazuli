//! Canonical-shape coverage for `vocab_client_src_001`.
//!
//! Covers the clean trees (Fixture A — both singular `app/web/` and
//! plural `app/clients/<name>/src/` topologies), the `external/`
//! escape-hatches (Fixture D and `app/clients/external/`), deeper
//! canonical nesting (§8 Q1 leniency), the dist/node_modules skip
//! list, top-of-`src/` file ignorance, the placeholder-client form,
//! the singular+plural coexistence path, and the CODE-constant +
//! determinism meta tests.
//!
//! Anti-pattern coverage lives in `tests_anti_patterns.rs`.

#![cfg(test)]

use tempfile::TempDir;

use super::test_support::{mkdir_p, names, touch};
use super::*;
use std::collections::BTreeSet;

/// Fixture A — clean tree with all 6 closed top-level folders +
/// all 6 closed `ui/` children → zero diagnostics.
#[test]
fn fixture_a_clean_singular_tree_does_not_fire() {
    let temp = TempDir::new().unwrap();
    for name in TOP_LEVEL_ALLOWED {
        mkdir_p(&temp.path().join("app/web").join(name));
    }
    for name in UI_CHILDREN_ALLOWED {
        mkdir_p(&temp.path().join("app/web/ui").join(name));
    }

    let findings = check(temp.path());

    assert!(
        findings.is_empty(),
        "clean tree should not fire; got: {:?}",
        findings
    );
}

#[test]
fn fixture_a_clean_plural_tree_does_not_fire() {
    let temp = TempDir::new().unwrap();
    for name in TOP_LEVEL_ALLOWED {
        mkdir_p(&temp.path().join("app/clients/web/src").join(name));
    }
    for name in UI_CHILDREN_ALLOWED {
        mkdir_p(&temp.path().join("app/clients/web/src/ui").join(name));
    }
    // A second client to confirm multi-client walks.
    for name in TOP_LEVEL_ALLOWED {
        mkdir_p(&temp.path().join("app/clients/os/src").join(name));
    }

    let findings = check(temp.path());

    assert!(
        findings.is_empty(),
        "clean plural tree should not fire; got: {:?}",
        findings
    );
}

/// Fixture D — `external/<name>/` with anti-patterns: walker
/// never enters it, so zero diagnostics fire.
#[test]
fn fixture_d_external_is_invisible() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("external/website/src/shared"));
    mkdir_p(&temp.path().join("external/website/src/presentation"));
    mkdir_p(&temp.path().join("external/website/src/ui/actions"));
    // Astro-style top-level in an external app — definitely not Lazuli-shape.
    mkdir_p(&temp.path().join("external/website/src/pages"));
    // A separate top-level external/ — also invisible.
    mkdir_p(&temp.path().join("external/legacy-admin/components"));

    let findings = check(temp.path());

    assert!(
        findings.is_empty(),
        "external/ must be invisible; got: {:?}",
        findings
    );
}

/// Fixture D' — `app/clients/external/<name>/` (the post-2026-05-18
/// revised location per `[[lazurite_monorepo_shape_2026-05-17]]`
/// §2.2) is also invisible. Walker enters `app/clients/` but
/// explicitly skips the `external` entry.
#[test]
fn app_clients_external_is_invisible() {
    let temp = TempDir::new().unwrap();
    // Anti-pattern names INSIDE app/clients/external/website/src/ —
    // would fire if walker descended, but must not.
    mkdir_p(&temp.path().join("app/clients/external/website/src/shared"));
    mkdir_p(
        &temp
            .path()
            .join("app/clients/external/website/src/presentation"),
    );
    mkdir_p(&temp.path().join("app/clients/external/website/src/pages"));
    mkdir_p(
        &temp
            .path()
            .join("app/clients/external/website/src/components"),
    );
    // Sibling Lazuli-native client with a real divergence — must still fire.
    mkdir_p(&temp.path().join("app/clients/web-app/src/shared"));

    let findings = check(temp.path());

    assert_eq!(findings.len(), 1, "got: {:?}", findings);
    assert!(
        findings[0].path.ends_with("app/clients/web-app/src/shared"),
        "should fire only on the lazuli-native client: {:?}",
        findings[0].path
    );
}

/// `dist/` and `node_modules/` at the client top level are skipped
/// (treated as out-of-scope, not flagged).
#[test]
fn dist_and_node_modules_skipped() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web/dist"));
    mkdir_p(&temp.path().join("app/web/node_modules"));
    mkdir_p(&temp.path().join("app/web/.lazuli"));
    mkdir_p(&temp.path().join("app/web/shell"));

    let findings = check(temp.path());

    assert!(
        findings.is_empty(),
        "dist/, node_modules/, .lazuli/ should be skipped; got: {:?}",
        findings
    );
}

/// Files at the top level of `src/` are ignored (this rule
/// enforces directory vocabulary; file-level rules live elsewhere).
#[test]
fn top_level_files_are_ignored() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web"));
    touch(&temp.path().join("app/web/main.tsx"));
    touch(&temp.path().join("app/web/App.tsx"));
    touch(&temp.path().join("app/web/styles.css"));

    let findings = check(temp.path());

    assert!(
        findings.is_empty(),
        "files should be ignored: {:?}",
        findings
    );
}

/// Both singular and plural can coexist briefly during migration
/// — each is walked independently.
#[test]
fn singular_and_plural_walked_independently() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web/shared"));
    mkdir_p(&temp.path().join("app/clients/legacy/src/presentation"));

    let findings = check(temp.path());

    assert_eq!(findings.len(), 2);
    let got = names(&findings);
    let expected: BTreeSet<String> = ["presentation", "shared"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(got, expected);
}

/// Plural client without a `src/` subdir is skipped (the rule
/// only enforces inside the canonical `src/` wrapper).
#[test]
fn plural_client_without_src_is_skipped() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/clients/placeholder/.gitkeep"));

    let findings = check(temp.path());

    assert!(findings.is_empty());
}

/// Deeper nesting under canonical folders is allowed (e.g.,
/// `routes/onboarding/host/` per §8 Q1). Only top-of-`src/` and
/// top-of-`ui/` are closed-catalog levels.
#[test]
fn deeper_nesting_under_canonical_folders_is_allowed() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web/routes/onboarding/host"));
    mkdir_p(&temp.path().join("app/web/ui/forms/inputs"));
    mkdir_p(&temp.path().join("app/web/state/toast/queue"));

    let findings = check(temp.path());

    assert!(
        findings.is_empty(),
        "deep nesting should pass: {:?}",
        findings
    );
}

/// CODE constant is the brief-mandated string verbatim.
#[test]
fn code_constant_matches_decision() {
    assert_eq!(Finding::CODE, "VOCAB-CLIENT-SRC-001");
}

/// Deterministic output across runs.
#[test]
fn deterministic_output() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web/shared"));
    mkdir_p(&temp.path().join("app/web/features"));
    mkdir_p(&temp.path().join("app/web/ui/actions"));
    mkdir_p(&temp.path().join("app/web/ui/branding"));

    let first: Vec<PathBuf> = check(temp.path()).into_iter().map(|f| f.path).collect();
    let second: Vec<PathBuf> = check(temp.path()).into_iter().map(|f| f.path).collect();

    assert_eq!(first, second);
}
