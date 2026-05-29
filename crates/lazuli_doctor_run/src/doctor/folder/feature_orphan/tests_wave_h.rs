//! Post-Wave-H 7+6 closed-catalog coverage for `feature_orphan`.
//!
//! Covers both the plural topology (`app/clients/<name>/src/...`) and
//! the singular topology (`app/web/...`) defined by
//! `[[client_src_canonical_architecture_2026-05-17]]` §3, plus the
//! `app/clients/external/` polyglot escape-hatch from
//! `[[lazurite_monorepo_shape_2026-05-17]]` §2.2.
//!
//! Pre-Wave-H legacy coverage lives in the sibling `tests_basic.rs`.

#![cfg(test)]

use super::test_support::{TempDir, touch};
use super::*;

// ---------------------------------------------------------------
// Post-Wave-H 7+6 closed catalog — plural topology
// ---------------------------------------------------------------

/// Fixture: a full 7+6-canon plural client tree (mirroring the
/// the canonical pilot's `app/clients/the canonical pilot-app/` shape). Mirrors
/// the `vocab_client_src_001` fixture-A pattern.
#[test]
fn post_wave_h_plural_client_full_canon_is_silent() {
    let tmp = TempDir::new().unwrap();
    // 7 top-level folders.
    touch(tmp.path(), "app/clients/web-app/src/shell/App.tsx");
    touch(tmp.path(), "app/clients/web-app/src/routes/Home.tsx");
    touch(tmp.path(), "app/clients/web-app/src/theme/tokens.ts");
    touch(tmp.path(), "app/clients/web-app/src/state/toast.store.ts");
    touch(tmp.path(), "app/clients/web-app/src/assets/images.d.ts");
    touch(tmp.path(), "app/clients/web-app/src/assets/index.ts");
    // cells/<feature>/...
    touch(
        tmp.path(),
        "app/clients/web-app/src/cells/messaging/ChatExperience.tsx",
    );
    // 6 ui/ sub-buckets.
    touch(tmp.path(), "app/clients/web-app/src/ui/forms/Button.tsx");
    touch(tmp.path(), "app/clients/web-app/src/ui/feedback/Toast.tsx");
    touch(
        tmp.path(),
        "app/clients/web-app/src/ui/navigation/NavBar.tsx",
    );
    touch(tmp.path(), "app/clients/web-app/src/ui/display/Card.tsx");
    touch(tmp.path(), "app/clients/web-app/src/ui/overlays/Dialog.tsx");
    touch(tmp.path(), "app/clients/web-app/src/ui/layout/Stack.tsx");
    // wrapper-level entry files.
    touch(tmp.path(), "app/clients/web-app/src/main.tsx");
    touch(tmp.path(), "app/clients/web-app/src/vite-env.d.ts");
    // client-root config files.
    touch(tmp.path(), "app/clients/web-app/vite.config.ts");
    touch(tmp.path(), "app/clients/web-app/playwright.config.ts");
    touch(tmp.path(), "app/clients/web-app/nativewind-env.d.ts");

    let findings = check(tmp.path());

    assert!(
        findings.is_empty(),
        "Wave H 7+6 canon must not fire; got: {:?}",
        findings
    );
}

/// Deep nesting under canonical folders is allowed: `routes/onboarding/host/`,
/// `cells/messaging/threads/`, `ui/forms/inputs/`, `state/toast/queue/`.
#[test]
fn post_wave_h_deep_nesting_under_canonical_folders_is_silent() {
    let tmp = TempDir::new().unwrap();
    touch(
        tmp.path(),
        "app/clients/web-app/src/routes/onboarding/host/Address.tsx",
    );
    touch(
        tmp.path(),
        "app/clients/web-app/src/routes/settings/panels/HostAddress.tsx",
    );
    touch(
        tmp.path(),
        "app/clients/web-app/src/cells/messaging/threads/Bubble.tsx",
    );
    touch(
        tmp.path(),
        "app/clients/web-app/src/ui/forms/inputs/Slider.tsx",
    );
    touch(
        tmp.path(),
        "app/clients/web-app/src/state/toast/queue/index.ts",
    );

    let findings = check(tmp.path());

    assert!(
        findings.is_empty(),
        "deep canonical nesting must not fire; got: {:?}",
        findings
    );
}

/// Orphan inside an otherwise canonical plural client tree still
/// fires (the rule narrows the canon, it doesn't open it).
#[test]
fn post_wave_h_orphan_inside_client_src_still_fires() {
    let tmp = TempDir::new().unwrap();
    // Canonical neighbour.
    touch(tmp.path(), "app/clients/web-app/src/ui/forms/Button.tsx");
    // Orphan: `components/` is NOT in the closed catalog at any depth.
    touch(tmp.path(), "app/clients/web-app/src/components/Sidebar.tsx");
    // Orphan: `lib/` catch-all at the client-src top level.
    touch(tmp.path(), "app/clients/web-app/src/lib/format.ts");
    // Orphan: `ui/cards/` — `cards` is not in the 6-sub-bucket catalog.
    touch(tmp.path(), "app/clients/web-app/src/ui/cards/PromoCard.tsx");

    let findings = check(tmp.path());

    assert_eq!(findings.len(), 3, "found: {:?}", findings);
    let paths: Vec<PathBuf> = findings.iter().map(|f| f.path.clone()).collect();
    assert!(paths.contains(&PathBuf::from(
        "app/clients/web-app/src/components/Sidebar.tsx"
    )));
    assert!(paths.contains(&PathBuf::from("app/clients/web-app/src/lib/format.ts")));
    assert!(paths.contains(&PathBuf::from(
        "app/clients/web-app/src/ui/cards/PromoCard.tsx"
    )));
}

/// Multiple clients under `app/clients/` are each their own
/// canonical tree — no cross-client interference.
#[test]
fn post_wave_h_multi_client_independent_walks() {
    let tmp = TempDir::new().unwrap();
    // Two clients, both canonical.
    touch(tmp.path(), "app/clients/web-app/src/shell/App.tsx");
    touch(tmp.path(), "app/clients/web-app/src/routes/Home.tsx");
    touch(tmp.path(), "app/clients/web-os/src/shell/App.tsx");
    touch(tmp.path(), "app/clients/web-os/src/theme/tokens.ts");
    // One orphan in the second client only.
    touch(tmp.path(), "app/clients/web-os/src/utils/format.ts");

    let findings = check(tmp.path());

    assert_eq!(findings.len(), 1, "found: {:?}", findings);
    assert_eq!(
        findings[0].path,
        PathBuf::from("app/clients/web-os/src/utils/format.ts")
    );
}

// ---------------------------------------------------------------
// Post-Wave-H 7+6 closed catalog — singular topology
// ---------------------------------------------------------------

/// Singular topology (`app/web/...`) must accept the same 7+6 shape
/// — only the wrapper differs.
#[test]
fn post_wave_h_singular_app_web_canon_is_silent() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "app/web/shell/App.tsx");
    touch(tmp.path(), "app/web/routes/Home.tsx");
    touch(tmp.path(), "app/web/theme/tokens.ts");
    touch(tmp.path(), "app/web/state/toast.store.ts");
    touch(tmp.path(), "app/web/assets/images.d.ts");
    touch(tmp.path(), "app/web/cells/messaging/ChatExperience.tsx");
    touch(tmp.path(), "app/web/ui/forms/Button.tsx");
    touch(tmp.path(), "app/web/ui/feedback/Toast.tsx");
    touch(tmp.path(), "app/web/ui/navigation/NavBar.tsx");
    touch(tmp.path(), "app/web/ui/display/Card.tsx");
    touch(tmp.path(), "app/web/ui/overlays/Dialog.tsx");
    touch(tmp.path(), "app/web/ui/layout/Stack.tsx");
    touch(tmp.path(), "app/web/main.tsx");
    touch(tmp.path(), "app/web/vite-env.d.ts");

    let findings = check(tmp.path());

    assert!(
        findings.is_empty(),
        "Wave H singular canon must not fire; got: {:?}",
        findings
    );
}

/// Orphan inside `app/web/` still fires (singular orphan path).
#[test]
fn post_wave_h_singular_app_web_orphan_fires() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "app/web/shell/App.tsx"); // canonical neighbour
    touch(tmp.path(), "app/web/components/Sidebar.tsx"); // orphan
    touch(tmp.path(), "app/web/ui/atoms/Button.tsx"); // orphan: atoms anti-pattern

    let findings = check(tmp.path());

    assert_eq!(findings.len(), 2, "found: {:?}", findings);
    let paths: Vec<PathBuf> = findings.iter().map(|f| f.path.clone()).collect();
    assert!(paths.contains(&PathBuf::from("app/web/components/Sidebar.tsx")));
    assert!(paths.contains(&PathBuf::from("app/web/ui/atoms/Button.tsx")));
}

/// `ui/index.ts` barrels (common pilot convention re-exporting the
/// 6 sub-bucket primitives) are accepted at the `ui/` level.
#[test]
fn post_wave_h_ui_index_barrel_is_silent() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "app/clients/web-app/src/ui/index.ts");
    touch(tmp.path(), "app/clients/web-app/src/ui/forms/Button.tsx");
    // Same for singular.
    touch(tmp.path(), "app/web/ui/index.ts");

    let findings = check(tmp.path());

    assert!(
        findings.is_empty(),
        "ui/index.ts barrels must not fire; got: {:?}",
        findings
    );
}

/// `app/clients/external/<name>/` is the polyglot escape-hatch per
/// `[[lazurite_monorepo_shape_2026-05-17]]` §2.2 (revised
/// 2026-05-18) — non-Lazuli frontends grouped under
/// `app/clients/` but excluded from Lazuli-canon walks. Doctor
/// must not descend into it.
#[test]
fn external_clients_subtree_is_invisible() {
    let tmp = TempDir::new().unwrap();
    // Astro-shape sub-app under app/clients/external/website/.
    touch(
        tmp.path(),
        "app/clients/external/website/src/content/copy.ts",
    );
    touch(
        tmp.path(),
        "app/clients/external/website/src/pages/index.astro",
    );
    touch(
        tmp.path(),
        "app/clients/external/website/src/components/Hero.tsx",
    );
    // Sibling Lazuli-native client stays in scope.
    touch(tmp.path(), "app/clients/web-app/src/shell/App.tsx");
    // Orphan in the Lazuli-native client still fires.
    touch(tmp.path(), "app/clients/web-app/src/components/Sidebar.tsx");

    let findings = check(tmp.path());

    assert_eq!(findings.len(), 1, "found: {:?}", findings);
    assert_eq!(
        findings[0].path,
        PathBuf::from("app/clients/web-app/src/components/Sidebar.tsx")
    );
}

/// Message updated to reference the post-Wave-H canon paths, not
/// the pre-canon `frontends/<target>/ui/`.
#[test]
fn message_references_post_wave_h_canon_paths() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "app/components/Foo.tsx");

    let findings = check(tmp.path());

    assert_eq!(findings.len(), 1);
    let msg = &findings[0].message;
    assert!(
        msg.contains("app/clients/<name>/src/routes/"),
        "message should reference Wave H routes/ canon: {msg}"
    );
    assert!(
        msg.contains("app/clients/<name>/src/cells/<feature>/"),
        "message should reference Wave H cells/<feature>/ canon: {msg}"
    );
    assert!(
        msg.contains("client_src_canonical_architecture_2026-05-17"),
        "message should cite the Wave H decision anchor: {msg}"
    );
}
