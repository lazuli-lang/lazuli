//! Anti-pattern coverage for `vocab_client_src_001`.
//!
//! Covers the closed-set anti-pattern catalog at the top of `src/`
//! (Fixture B), the closed-set anti-pattern catalog at the top of
//! `ui/` (Fixture C), the Atomic-Design `ui/{atoms,molecules,organisms}`
//! routing hint, the `ui/{cards,service}` routing destinations, and
//! the open-set top-level catch-all bucket detection.
//!
//! Canonical-shape coverage lives in `tests_canon.rs`.

#![cfg(test)]

use std::collections::BTreeSet;

use tempfile::TempDir;

use super::test_support::{mkdir_p, names};
use super::*;

/// Fixture B — top-level anti-patterns: `shared`, `presentation`,
/// `application`, `features` → exactly 4 diagnostics.
#[test]
fn fixture_b_top_level_anti_patterns_fire() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web/shared"));
    mkdir_p(&temp.path().join("app/web/presentation"));
    mkdir_p(&temp.path().join("app/web/application"));
    mkdir_p(&temp.path().join("app/web/features"));
    // A canonical folder mixed in to confirm we don't fire on it.
    mkdir_p(&temp.path().join("app/web/shell"));

    let findings = check(temp.path());

    assert_eq!(
        findings.len(),
        4,
        "expected 4 findings; got: {:?}",
        findings
    );
    let got = names(&findings);
    let expected: BTreeSet<String> = ["application", "features", "presentation", "shared"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(got, expected);

    // Every message has the rule's hint-routing destination text.
    for finding in &findings {
        assert!(
            finding
                .message
                .contains("client_src_canonical_architecture"),
            "message missing anchor: {}",
            finding.message
        );
    }
}

/// Fixture C — `ui/{actions, branding}` → exactly 2 diagnostics
/// (and zero top-level diagnostics).
#[test]
fn fixture_c_ui_anti_patterns_fire() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web/ui/actions"));
    mkdir_p(&temp.path().join("app/web/ui/branding"));
    // Canonical ui/ children mixed in to confirm we don't fire.
    mkdir_p(&temp.path().join("app/web/ui/forms"));
    mkdir_p(&temp.path().join("app/web/ui/feedback"));

    let findings = check(temp.path());

    assert_eq!(
        findings.len(),
        2,
        "expected 2 findings; got: {:?}",
        findings
    );
    let got = names(&findings);
    let expected: BTreeSet<String> = ["actions", "branding"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(got, expected);

    // Hints route into the closed catalog.
    let actions = findings
        .iter()
        .find(|f| f.path.ends_with("actions"))
        .unwrap();
    assert!(actions.message.contains("ui/forms"));
    let branding = findings
        .iter()
        .find(|f| f.path.ends_with("branding"))
        .unwrap();
    assert!(branding.message.contains("ui/display"));
}

/// Atomic-design anti-patterns at the `ui/` level → 3 diagnostics
/// with the Atomic-Design hint.
#[test]
fn ui_atomic_design_fires_with_hint() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web/ui/atoms"));
    mkdir_p(&temp.path().join("app/web/ui/molecules"));
    mkdir_p(&temp.path().join("app/web/ui/organisms"));

    let findings = check(temp.path());

    assert_eq!(findings.len(), 3);
    for finding in &findings {
        assert!(
            finding.message.contains("Atomic Design rejected"),
            "missing atomic-design hint: {}",
            finding.message
        );
    }
}

/// `cards` and `service` route into `ui/display` and
/// `app/features/<f>/cells/` respectively.
#[test]
fn ui_cards_and_service_route_to_destinations() {
    let temp = TempDir::new().unwrap();
    mkdir_p(&temp.path().join("app/web/ui/cards"));
    mkdir_p(&temp.path().join("app/web/ui/service"));

    let findings = check(temp.path());

    assert_eq!(findings.len(), 2);
    let cards = findings.iter().find(|f| f.path.ends_with("cards")).unwrap();
    assert!(cards.message.contains("ui/display"));
    let service = findings
        .iter()
        .find(|f| f.path.ends_with("service"))
        .unwrap();
    assert!(service.message.contains("app/features/<f>/cells/"));
}

/// Open-set catch-all top-level dirs (`lib`, `utils`, etc.) all fire
/// with the catch-all hint.
#[test]
fn top_level_catch_all_buckets_fire() {
    let temp = TempDir::new().unwrap();
    for bucket in &[
        "lib",
        "utils",
        "helpers",
        "services",
        "components",
        "types",
        "api",
        "common",
        "misc",
    ] {
        mkdir_p(&temp.path().join("app/web").join(bucket));
    }

    let findings = check(temp.path());

    assert_eq!(findings.len(), 9, "got: {:?}", findings);
    for finding in &findings {
        assert!(
            finding.message.contains("catch-all"),
            "missing catch-all hint: {}",
            finding.message
        );
    }
}
