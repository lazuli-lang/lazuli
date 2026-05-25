//! Doctor rule `feature-orphan-component`.
//!
//! Flags user-authored `.tsx` / `.ts` files in non-canonical paths under a
//! Lazurite product repo. The canonical frontend layout is defined by L0 #1
//! (`docs/proposals/lazurite-frontend-folder-canon.md` §4), the Wave H
//! corrective (`[[client_src_canonical_architecture_2026-05-17]]` §3), and
//! the singular-vs-plural decision
//! (`[[frontend_layout_default_and_slicing_2026-05-16]]`).
//!
//! Canonical locations (post-Wave-H 7+6 closed catalog):
//!
//!   Plural client topology (`app/clients/<name>/src/...`):
//!     - `app/clients/<name>/src/shell/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/routes/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/cells/<feature>/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/ui/{forms,feedback,navigation,display,overlays,layout}/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/theme/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/state/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/assets/**/*.{ts,tsx}`
//!     - `app/clients/<name>/src/{index,main,vite-env}.{ts,tsx}` (wrapper-level files)
//!     - `app/clients/<name>/{main,vite.config,tailwind.config,playwright.config,nativewind-env}.ts(x)` (client config)
//!
//!   Singular client topology (`app/web/...` — same shape, different wrapper):
//!     - `app/web/shell/**/*.{ts,tsx}`
//!     - `app/web/routes/**/*.{ts,tsx}`
//!     - `app/web/cells/<feature>/**/*.{ts,tsx}`
//!     - `app/web/ui/{forms,feedback,navigation,display,overlays,layout}/**/*.{ts,tsx}`
//!     - `app/web/theme/**/*.{ts,tsx}`
//!     - `app/web/state/**/*.{ts,tsx}`
//!     - `app/web/assets/**/*.{ts,tsx}`
//!     - `app/web/{index,main,vite-env}.{ts,tsx}`
//!
//!   Legacy DSL feature shape (still recognised — generators emit here):
//!     - `app/features/<feat>/web/{cells,views/<audience>}/*.{ts,tsx}`
//!     - `app/features/<feat>/mobile/{cells,views/<audience>}/*.{ts,tsx}`
//!     - `features/<feat>/...` (test-fixture form, identical shape)
//!
//!   Legacy frontends wrapper (pre-Wave-H pilots):
//!     - `frontends/<target>/{shell,theme,ui,hooks,lib}/**/*.{ts,tsx}`
//!     - `frontends/<target>/{main,vite.config,tailwind.config}.ts(x)`
//!
//! Anything else inside the Lazuli-owned tree, such as `src/components/`,
//! `app/components/`, or `lib/components/`, is an orphan component.
//!
//! ## Scope discipline (polyglot monorepo)
//!
//! Per `CLAUDE.md` and the polyglot-monorepo canon, a Lazurite repo's root
//! may host non-Lazuli siblings (`apps/<frontend>/`, `packages/<pkg>/`,
//! `brand/`, `scripts/`, `docs/`, etc.) that are owned by the surrounding
//! pnpm/turbo/Astro/Vite workspace and NOT by the Lazuli compiler.
//!
//! This rule therefore only descends into the Lazuli-owned top-level
//! entries — `app/`, `features/` (legacy fixture form), and `frontends/`.
//! Anything else at the root is invisible to this rule. The Lazuli-owned
//! tree's contents are still walked recursively from those entry points,
//! so orphans inside `app/shared/ui/` or `app/components/` still fire.
//!
//! One subtree under `app/` is explicitly skipped: `app/clients/external/`
//! is the polyglot escape-hatch per
//! `[[lazurite_monorepo_shape_2026-05-17]]` §2.2 (revised 2026-05-18) —
//! non-Lazuli frontends (Astro marketing sites, Rails dashboards, etc.)
//! live grouped with Lazuli-native clients but follow their own
//! conventions. Doctor does not descend into `app/clients/external/**`.

use std::fs;
use std::path::{Path, PathBuf};

/// One `feature-orphan-component` finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "feature-orphan-component";

    pub fn message(path: &Path) -> String {
        format!(
            "`{}` is outside the canonical Lazurite frontend layout. \
             Route into the 7+6 closed catalog per \
             `[[client_src_canonical_architecture_2026-05-17]]` §3: \
             `app/clients/<name>/src/routes/**` (page-level .tsx), \
             `app/clients/<name>/src/cells/<feature>/**` (handcrafted slot \
             widget), `app/clients/<name>/src/ui/{{forms,feedback,navigation,\
             display,overlays,layout}}/**` (shared primitive), or \
             `app/clients/<name>/src/{{shell,theme,state,assets}}/**`. \
             Singular topology uses `app/web/` with the same shape.",
            path.display()
        )
    }
}

/// Walk the Lazuli-owned subtrees of `root`, classify each `.tsx`/`.ts` file
/// as canonical or orphan, and return findings for the orphans.
///
/// Only the Lazuli-owned top-level entries are walked (`app/`, `features/`,
/// `frontends/`). Polyglot-sibling roots (`apps/<frontend>/`,
/// `packages/<pkg>/`, `brand/`, `scripts/`, etc.) are invisible to this
/// rule — they are owned by the surrounding workspace, not by the Lazuli
/// compiler. See module docs for the scope-discipline rationale.
///
/// Inside Lazuli-owned subtrees, skips directories: `node_modules`, `dist`,
/// `.lazuli`, `.git`, `target`, `.next`, `.expo`, `.cache`, `coverage`.
///
/// Also skips test/story files via extension: `*.test.tsx`, `*.spec.tsx`,
/// `*.stories.tsx`.
pub fn check(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for entry in LAZULI_OWNED_ROOTS {
        let subtree = root.join(entry);
        if subtree.is_dir() {
            walk(root, &subtree, &mut findings);
        }
    }
    findings
}

/// Top-level entries under the project root that this rule descends into.
///
/// Anything else at the root is a polyglot sibling and out of scope per
/// `docs/scope-discipline.md` + the polyglot-monorepo canon. Adding a new
/// entry here is a language-canon decision, not a per-project config.
const LAZULI_OWNED_ROOTS: &[&str] = &["app", "features", "frontends"];

// --- internal helpers ---

fn walk(root: &Path, dir: &Path, findings: &mut Vec<Finding>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if should_skip_dir(&name) {
                continue;
            }
            if is_external_clients_subtree(root, &path) {
                continue;
            }
            walk(root, &path, findings);
            continue;
        }

        if !file_type.is_file() || !is_ts_or_tsx_file(&path) || is_test_or_story_file(&path) {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(path.as_path());
        if !is_canonical_path(rel) {
            findings.push(Finding {
                path: rel.to_path_buf(),
                message: Finding::message(rel),
            });
        }
    }
}

/// Returns true if the relative-to-root path matches one of the canonical
/// locations.
fn is_canonical_path(rel: &Path) -> bool {
    let parts = path_parts(rel);

    match parts.as_slice() {
        // --- Wave H 7+6 canon, plural topology ---
        // app/clients/<name>/src/{shell,routes,theme,state,assets}/<...>
        ["app", "clients", _, "src", folder, rest @ ..]
            if is_client_src_top_level(folder) && !rest.is_empty() =>
        {
            true
        }
        // app/clients/<name>/src/cells/<feature>/<...>
        // (per Wave H amendment: subdirs MUST mirror app/features/<feature>/;
        // the mirror enforcement is sibling rule's job — here we just accept
        // any non-empty cells/<feature>/file shape).
        ["app", "clients", _, "src", "cells", _feature, rest @ ..] if !rest.is_empty() => true,
        // app/clients/<name>/src/ui/<sub-bucket>/<...>
        ["app", "clients", _, "src", "ui", sub, rest @ ..]
            if is_ui_sub_bucket(sub) && !rest.is_empty() =>
        {
            true
        }
        // app/clients/<name>/src/ui/index.{ts,tsx} — barrel file
        // re-exporting the 6 sub-bucket primitives. Common pilot
        // convention; not a sub-bucket so it has its own arm.
        ["app", "clients", _, "src", "ui", file] if is_barrel_file(file) => true,
        // app/clients/<name>/src/<wrapper-file>.{ts,tsx}
        ["app", "clients", _, "src", file] if is_client_src_wrapper_file(file) => true,
        // app/clients/<name>/<client-config>.{ts,tsx} — tooling configs that
        // live at the wrapper level (vite.config.ts, playwright.config.ts,
        // tailwind.config.ts, nativewind-env.d.ts, etc.).
        ["app", "clients", _, file] if is_client_wrapper_config_file(file) => true,

        // --- Wave H 7+6 canon, singular topology ---
        // app/web/{shell,routes,theme,state,assets}/<...>
        ["app", "web", folder, rest @ ..]
            if is_client_src_top_level(folder) && !rest.is_empty() =>
        {
            true
        }
        // app/web/cells/<feature>/<...>
        ["app", "web", "cells", _feature, rest @ ..] if !rest.is_empty() => true,
        // app/web/ui/<sub-bucket>/<...>
        ["app", "web", "ui", sub, rest @ ..] if is_ui_sub_bucket(sub) && !rest.is_empty() => true,
        // app/web/ui/index.{ts,tsx} — barrel file
        ["app", "web", "ui", file] if is_barrel_file(file) => true,
        // app/web/<wrapper-file>.{ts,tsx} (top-level shell entry)
        ["app", "web", file] if is_client_src_wrapper_file(file) => true,

        // --- Legacy DSL feature shape ---
        // app/features/<feat>/web/cells/<file>.tsx
        // app/features/<feat>/mobile/cells/<file>.tsx
        ["app", "features", _, platform, "cells", rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }

        // app/features/<feat>/web/views/<audience>/<file>.tsx
        // app/features/<feat>/mobile/views/<audience>/<file>.tsx
        ["app", "features", _, platform, "views", _, rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }

        // Backwards-compatible legacy fixture shape.
        ["features", _, platform, "cells", rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }
        ["features", _, platform, "views", _, rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }

        // --- Legacy frontends wrapper (pre-Wave-H pilots) ---
        // frontends/web/shell/<...>.tsx
        // frontends/mobile/theme/<...>.tsx
        ["frontends", platform, subdir, rest @ ..]
            if is_platform(platform)
                && matches!(*subdir, "shell" | "theme" | "ui" | "hooks" | "lib")
                && !rest.is_empty() =>
        {
            true
        }

        // frontends/web/main.tsx, vite.config.ts, tailwind.config.ts
        ["frontends", platform, file]
            if is_platform(platform)
                && matches!(
                    *file,
                    "main.tsx" | "main.ts" | "vite.config.ts" | "tailwind.config.ts"
                ) =>
        {
            true
        }

        _ => false,
    }
}

/// Top-level folders of the post-Wave-H 7-folder closed catalog,
/// excluding `cells/` (which has its own match arm because it requires a
/// `<feature>` sub-bucket) and `ui/` (which has a closed sub-catalog).
///
/// Per `[[client_src_canonical_architecture_2026-05-17]]` §3.1 the 7 are
/// `{shell, routes, ui, theme, state, assets, cells}`. This helper covers
/// the 4 that accept any rest path (`shell`, `routes`, `theme`, `state`)
/// plus `assets` (static files).
fn is_client_src_top_level(name: &str) -> bool {
    matches!(name, "shell" | "routes" | "theme" | "state" | "assets")
}

/// Closed catalog for `ui/` sub-buckets per
/// `[[client_src_canonical_architecture_2026-05-17]]` §3.2.
fn is_ui_sub_bucket(name: &str) -> bool {
    matches!(
        name,
        "forms" | "feedback" | "navigation" | "display" | "overlays" | "layout"
    )
}

/// Barrel-file names allowed at a folder's top level (a single
/// re-export hub for the folder's primitives). Common pilot convention
/// for `ui/index.ts` etc.; not part of the closed catalog but harmless.
fn is_barrel_file(file: &str) -> bool {
    matches!(file, "index.ts" | "index.tsx")
}

/// File names that may live directly under `app/clients/<name>/src/` or
/// `app/web/` (the client wrapper root). These are the bootstrap entry
/// points + Vite/TS housekeeping files that scaffolds emit at the
/// wrapper level.
fn is_client_src_wrapper_file(file: &str) -> bool {
    matches!(
        file,
        "main.tsx" | "main.ts" | "index.ts" | "index.tsx" | "App.tsx" | "App.ts" | "vite-env.d.ts"
    )
}

/// File names that may live directly under `app/clients/<name>/` (one
/// level above `src/` — the client root). These are tooling configs
/// that have to sit next to `package.json` so Vite / Playwright /
/// Tailwind pick them up. Only `.ts(x)` entries are listed — sibling
/// `.cjs` / `.mjs` / `.js` configs are out of scope for this rule (it
/// only scans `.ts(x)` files; see `is_ts_or_tsx_file`).
fn is_client_wrapper_config_file(file: &str) -> bool {
    matches!(
        file,
        "vite.config.ts"
            | "vitest.config.ts"
            | "playwright.config.ts"
            | "tailwind.config.ts"
            | "postcss.config.ts"
            | "nativewind-env.d.ts"
            | "env.d.ts"
    )
}

fn is_ts_or_tsx_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx")
    )
}

fn is_test_or_story_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.ends_with(".test.tsx")
        || name.ends_with(".test.ts")
        || name.ends_with(".spec.tsx")
        || name.ends_with(".spec.ts")
        || name.ends_with(".stories.tsx")
        || name.ends_with(".stories.ts")
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules"
            | "dist"
            | ".lazuli"
            | ".git"
            | "target"
            | ".next"
            | ".expo"
            | ".cache"
            | "coverage"
    )
}

fn path_parts(path: &Path) -> Vec<&str> {
    path.components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect()
}

fn is_platform(part: &str) -> bool {
    matches!(part, "web" | "mobile")
}

/// True if `path` is the `app/clients/external/` subtree under `root` —
/// the polyglot escape-hatch per `[[lazurite_monorepo_shape_2026-05-17]]`
/// §2.2 (revised 2026-05-18). Non-Lazuli frontends (Astro marketing,
/// Rails dashboards, etc.) live here and are excluded from canon walks.
fn is_external_clients_subtree(root: &Path, path: &Path) -> bool {
    path == root.join("app").join("clients").join("external")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};

    mod tempfile {
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::time::{SystemTime, UNIX_EPOCH};

        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            pub fn new() -> std::io::Result<Self> {
                let mut path = std::env::temp_dir();
                let nanos = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                path.push(format!(
                    "lazuli-feature-orphan-{}-{nanos}",
                    std::process::id()
                ));
                fs::create_dir_all(&path)?;
                Ok(Self { path })
            }

            pub fn path(&self) -> &Path {
                &self.path
            }
        }

        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    fn touch(root: &Path, rel: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(path).unwrap();
    }

    #[test]
    fn canonical_feature_view_does_not_fire() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "app/features/slug/web/views/admin/list.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn canonical_feature_cell_does_not_fire() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "app/features/slug/web/cells/type_badge.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn canonical_app_ui_does_not_fire() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "frontends/web/ui/button.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn orphan_top_level_src_is_out_of_scope() {
        // `src/` at the project root is a polyglot-sibling concern (Astro,
        // Vite, etc.) — not Lazuli-owned. Doctor must not flag files there;
        // an orphan inside `app/components/` is the correct in-scope signal.
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "src/components/SlugTable.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn orphan_app_components_fires() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "app/components/Foo.tsx");

        let findings = check(tmp.path());

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].path, PathBuf::from("app/components/Foo.tsx"));
    }

    #[test]
    fn polyglot_apps_sibling_is_out_of_scope() {
        // Polyglot monorepo: `apps/website/` is an Astro/Vite sibling owned
        // by the pnpm workspace, not Lazuli. Doctor must not descend.
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "apps/website/src/content/copy.ts");
        touch(tmp.path(), "apps/example-app/src/main.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn polyglot_packages_sibling_is_out_of_scope() {
        // `packages/<pkg>/` are pnpm-workspace siblings (design tokens,
        // shared utilities, etc.) — not Lazuli-owned.
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "packages/design-tokens/src/index.ts");
        touch(tmp.path(), "packages/design-tokens/scripts/build-css.ts");
        touch(tmp.path(), "brand/assets/index.ts");
        touch(tmp.path(), "scripts/seed.ts");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn orphans_inside_lazuli_owned_tree_still_fire() {
        // Polyglot siblings stay invisible; orphans inside the Lazuli-owned
        // subtree (`app/...`, `frontends/...`) still fire.
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "apps/website/src/orphan.ts");
        touch(tmp.path(), "packages/util/src/orphan.ts");
        touch(tmp.path(), "app/shared/ui/Bad.tsx");
        touch(tmp.path(), "frontends/web/junk/Bad.tsx");
        touch(tmp.path(), "app/features/slug/web/cells/ok.tsx");
        touch(tmp.path(), "frontends/web/ui/button.tsx");

        let findings = check(tmp.path());

        assert_eq!(findings.len(), 2, "found: {:?}", findings);
        assert_eq!(findings[0].path, PathBuf::from("app/shared/ui/Bad.tsx"));
        assert_eq!(
            findings[1].path,
            PathBuf::from("frontends/web/junk/Bad.tsx")
        );
    }

    #[test]
    fn test_files_are_skipped() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(
            tmp.path(),
            "app/features/slug/web/views/admin/list.test.tsx",
        );

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn node_modules_skipped() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "node_modules/pkg/components/Button.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn deterministic_order() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "frontends/web/junk/Zed.tsx");
        touch(tmp.path(), "frontends/web/junk/Alpha.tsx");
        touch(tmp.path(), "app/components/Foo.tsx");

        let first = check(tmp.path());
        let second = check(tmp.path());

        assert_eq!(first, second);
        assert_eq!(
            first.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            vec![
                PathBuf::from("app/components/Foo.tsx"),
                PathBuf::from("frontends/web/junk/Alpha.tsx"),
                PathBuf::from("frontends/web/junk/Zed.tsx"),
            ]
        );
    }

    // ---------------------------------------------------------------
    // Post-Wave-H 7+6 closed catalog — plural topology
    // ---------------------------------------------------------------

    /// Fixture: a full 7+6-canon plural client tree (mirroring the
    /// hostpoint pilot's `app/clients/hostpoint-app/` shape). Mirrors
    /// the `vocab_client_src_001` fixture-A pattern.
    #[test]
    fn post_wave_h_plural_client_full_canon_is_silent() {
        let tmp = tempfile::TempDir::new().unwrap();
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
        let tmp = tempfile::TempDir::new().unwrap();
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
        let tmp = tempfile::TempDir::new().unwrap();
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
        let tmp = tempfile::TempDir::new().unwrap();
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
        let tmp = tempfile::TempDir::new().unwrap();
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
        let tmp = tempfile::TempDir::new().unwrap();
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
        let tmp = tempfile::TempDir::new().unwrap();
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
        let tmp = tempfile::TempDir::new().unwrap();
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
        let tmp = tempfile::TempDir::new().unwrap();
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
}
