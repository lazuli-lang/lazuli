//! Doctor rule `VOCAB-CLIENT-SRC-001`.
//!
//! Closed-catalog enforcement for the Lazurite client `src/` layout per
//! `docs/decisions/client_src_canonical_architecture_2026-05-17.md` §3.
//!
//! The rule walks two possible client-tree shapes (per
//! `[[lazurite_monorepo_shape_2026-05-17]]` and
//! `[[frontend_layout_default_and_slicing_2026-05-16]]`):
//!
//! - **Singular** topology: `app/web/` — one web target, the 6-folder
//!   closed catalog applies at this level.
//! - **Plural** topology: `app/clients/<name>/src/` — every direct
//!   child of `app/clients/` is a client root; the 6-folder closed
//!   catalog applies under each `<name>/src/`.
//!
//! Two closed catalogs are enforced:
//!
//! 1. Top-level of the client tree (the wrapper itself): must be one of
//!    `{shell, routes, ui, theme, state, assets, cells}` (per the
//!    §3.1 amendment 2026-05-17 — was 6 folders; `cells/` added as the
//!    7th to resolve tsc module-resolution for handcrafted feature
//!    cells).
//! 2. Children of the `ui/` subfolder (when present): must be one of
//!    `{forms, feedback, navigation, display, overlays, layout}`.
//! 3. Children of the `cells/` subfolder (when present): each subdir
//!    MUST mirror an existing `app/features/<feature>/` directory.
//!    Orphan cells (no matching feature) fire a diagnostic with the
//!    cell sub-dir's path. NOTE: orphan-cell enforcement is a future
//!    enhancement; v0 only enforces the top-level + `ui/` closed
//!    catalogs.
//!
//! Any other directory at those two levels fires a finding. Files
//! (rather than directories) at those levels are NOT flagged by this
//! rule — the file layout discipline lives in sibling rules like
//! `feature-orphan-component`.
//!
//! ## Skip discipline
//!
//! Non-Lazuli sub-apps under `app/clients/external/<name>/` (per
//! `[[lazurite_monorepo_shape_2026-05-17]]` §2.2, revised 2026-05-18)
//! are invisible: when listing `app/clients/<name>/`, the walker
//! explicitly skips the `external` entry. The pre-revision top-level
//! `external/` location is also invisible — the walker never enters
//! it because it only descends into `app/web/` and
//! `app/clients/<name>/src/`.
//!
//! Inside walked trees, the rule does not descend further than the
//! checked levels — it inspects the immediate children of the client
//! root and the immediate children of `ui/` only. Build outputs like
//! `dist/` and `node_modules/` therefore can't appear in either
//! closed-catalog level since they would themselves be the
//! anti-pattern. To keep error noise low, however, the well-known
//! tooling dirs `dist`, `node_modules`, and `.lazuli` are explicitly
//! skipped at the top level (treated as out-of-scope rather than
//! diagnosed — the user typically can't avoid them and they aren't
//! source code).

use std::fs;
use std::path::{Path, PathBuf};

/// One `VOCAB-CLIENT-SRC-001` finding — a single anti-pattern
/// directory at one of the two enforced levels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Path of the offending directory (relative to the project root or
    /// absolute, as walked). Callers normalize via `doctor_rule_path`.
    pub path: PathBuf,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-CLIENT-SRC-001";
}

/// Top-level closed catalog for the client tree (singular `app/web/`
/// or each plural `app/clients/<name>/src/`).
const TOP_LEVEL_ALLOWED: &[&str] =
    &["shell", "routes", "ui", "theme", "state", "assets", "cells"];

/// Closed sub-catalog for `ui/` children inside the client tree.
const UI_CHILDREN_ALLOWED: &[&str] =
    &["forms", "feedback", "navigation", "display", "overlays", "layout"];

/// Well-known tooling directories that are skipped at the top-level
/// without firing a diagnostic. They are not source-controlled vocab
/// choices the user makes, so flagging them is noise.
const TOP_LEVEL_SKIP: &[&str] = &["dist", "node_modules", ".lazuli", ".git", ".cache"];

/// Walk the project's client trees and return one finding per
/// closed-catalog violation. See module docs for scope.
pub fn check(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Singular: app/web/
    let app_web = root.join("app").join("web");
    if app_web.is_dir() {
        check_client_tree(&app_web, &mut findings);
    }

    // Plural: app/clients/<name>/src/ — visit every direct child of
    // app/clients/ that has a src/ subdir. Children without a src/
    // (e.g., placeholder dirs) are out of scope; the
    // monorepo-exception convention puts non-Lazuli apps in
    // external/<name>/ instead.
    let app_clients = root.join("app").join("clients");
    if let Ok(entries) = fs::read_dir(&app_clients) {
        let mut client_dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        client_dirs.sort();

        for client_dir in client_dirs {
            // Skip the polyglot escape-hatch subtree per
            // `[[lazurite_monorepo_shape_2026-05-17]]` §2.2 (revised
            // 2026-05-18) — `app/clients/external/<name>/` houses
            // non-Lazuli frontends and is exempt from canon walks.
            if client_dir.file_name().and_then(|n| n.to_str()) == Some("external") {
                continue;
            }
            let client_src = client_dir.join("src");
            if client_src.is_dir() {
                check_client_tree(&client_src, &mut findings);
            }
        }
    }

    findings
}

fn check_client_tree(client_root: &Path, findings: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(client_root) else {
        return;
    };

    let mut children: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();

    for child in children {
        let name = match child.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };

        if TOP_LEVEL_SKIP.contains(&name.as_str()) {
            continue;
        }

        if !TOP_LEVEL_ALLOWED.contains(&name.as_str()) {
            findings.push(Finding {
                message: top_level_message(&name, &child),
                path: child.clone(),
            });
            continue;
        }

        if name == "ui" {
            check_ui_children(&child, findings);
        }
    }
}

fn check_ui_children(ui_root: &Path, findings: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(ui_root) else {
        return;
    };

    let mut children: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();

    for child in children {
        let name = match child.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None => continue,
        };

        if !UI_CHILDREN_ALLOWED.contains(&name.as_str()) {
            findings.push(Finding {
                message: ui_child_message(&name, &child),
                path: child.clone(),
            });
        }
    }
}

fn top_level_message(name: &str, path: &Path) -> String {
    let hint = match name {
        "shared" => Some(
            "anti-pattern catch-all; route into one of \
             `{shell, routes, ui, theme, state, assets}` or \
             `app/features/<f>/cells/`",
        ),
        "presentation" => Some(
            "DDD/Clean-Architecture layer leaked; reject per \
             `[[client_src_canonical_architecture_2026-05-17]]` §2 — \
             route children into `ui/`, `shell/`, or `theme/`",
        ),
        "application" => Some(
            "DDD/Clean-Architecture layer leaked; reject per \
             `[[client_src_canonical_architecture_2026-05-17]]` §2 — \
             cross-feature state moves to `state/`",
        ),
        "features" => Some(
            "a feature has one home — `app/features/<f>/`; cells go to \
             `app/features/<f>/cells/`",
        ),
        "lib" | "utils" | "helpers" | "services" | "components" | "types" | "api" | "common"
        | "misc" => Some(
            "catch-all anti-pattern; each file has a real home in \
             `{shell, routes, ui, theme, state, assets}` or \
             `app/features/<f>/cells/`",
        ),
        _ => None,
    };

    let allowed = TOP_LEVEL_ALLOWED.join(", ");
    match hint {
        Some(hint) => format!(
            "`{}` is outside the client `src/` closed catalog `{{{}}}` — {}. \
             See `[[client_src_canonical_architecture_2026-05-17]]` §3.",
            path.display(),
            allowed,
            hint
        ),
        None => format!(
            "`{}` is outside the client `src/` closed catalog `{{{}}}`. \
             See `[[client_src_canonical_architecture_2026-05-17]]` §3 \
             for the destination of `{}/`.",
            path.display(),
            allowed,
            name
        ),
    }
}

fn ui_child_message(name: &str, path: &Path) -> String {
    let hint = match name {
        "actions" => Some(
            "consolidate into `ui/forms/` (buttons are form actions) \
             or `ui/navigation/` (links)",
        ),
        "branding" => Some(
            "consolidate into `ui/display/` (logos as display) or \
             `theme/` (brand tokens)",
        ),
        "cards" => Some("consolidate into `ui/display/` — Card is a display primitive"),
        "service" => Some(
            "feature-specific widget; move to `app/features/<f>/cells/`",
        ),
        "atoms" | "molecules" | "organisms" => Some(
            "Atomic Design rejected per \
             `[[client_src_canonical_architecture_2026-05-17]]` §3.2 — \
             closed-catalog by function (forms/feedback/navigation/display/overlays/layout), \
             not by size",
        ),
        _ => None,
    };

    let allowed = UI_CHILDREN_ALLOWED.join(", ");
    match hint {
        Some(hint) => format!(
            "`{}` is outside the `ui/` closed catalog `{{{}}}` — {}. \
             See `[[client_src_canonical_architecture_2026-05-17]]` §3.2.",
            path.display(),
            allowed,
            hint
        ),
        None => format!(
            "`{}` is outside the `ui/` closed catalog `{{{}}}`. \
             See `[[client_src_canonical_architecture_2026-05-17]]` §3.2 \
             for the destination of `ui/{}/`.",
            path.display(),
            allowed,
            name
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs::{self, File};
    use tempfile::TempDir;

    fn mkdir_p(path: &Path) {
        fs::create_dir_all(path).unwrap();
    }

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(path).unwrap();
    }

    fn names(findings: &[Finding]) -> BTreeSet<String> {
        findings
            .iter()
            .map(|f| {
                f.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect()
    }

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

        assert_eq!(findings.len(), 4, "expected 4 findings; got: {:?}", findings);
        let got = names(&findings);
        let expected: BTreeSet<String> = ["application", "features", "presentation", "shared"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(got, expected);

        // Every message has the rule's hint-routing destination text.
        for finding in &findings {
            assert!(
                finding.message.contains("client_src_canonical_architecture"),
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

        assert_eq!(findings.len(), 2, "expected 2 findings; got: {:?}", findings);
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
        mkdir_p(&temp.path().join("app/clients/external/website/src/presentation"));
        mkdir_p(&temp.path().join("app/clients/external/website/src/pages"));
        mkdir_p(&temp.path().join("app/clients/external/website/src/components"));
        // Sibling Lazuli-native client with a real divergence — must still fire.
        mkdir_p(&temp.path().join("app/clients/web-app/src/shared"));

        let findings = check(temp.path());

        assert_eq!(findings.len(), 1, "got: {:?}", findings);
        assert!(
            findings[0]
                .path
                .ends_with("app/clients/web-app/src/shared"),
            "should fire only on the lazuli-native client: {:?}",
            findings[0].path
        );
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

        assert!(findings.is_empty(), "files should be ignored: {:?}", findings);
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

        assert!(findings.is_empty(), "deep nesting should pass: {:?}", findings);
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
}
