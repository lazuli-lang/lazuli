//! Frontend scaffolding helpers for `lazuli new --frontends <list>`.
//!
//! Implements L0 #1 §6.1 — when the user passes `--frontends web`
//! and/or `--frontends mobile` to `lazuli new`, this module materializes
//! the canonical frontend-adapter shell, theme, config, and manifest entries.
//!
//! ## Boundary
//!
//! These helpers know NOTHING about the backend scaffold (already wired
//! by `main::scaffold_bare` / `main::scaffold_from_template`). They
//! operate on an already-created `project_root` and only add/append.
//!
//! ## Idempotency
//!
//! Both `scaffold_frontend_web` and `scaffold_frontend_mobile` are
//! **idempotent**: a second invocation on the same `project_root` is a
//! no-op — already-existing files are left untouched and the manifest
//! is only appended once. This lets the orchestrator call them
//! defensively (e.g. when re-running `lazuli new --frontends web,mobile`
//! after a partial failure) without clobbering user edits.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::templates;

/// Scaffold the web frontend skeleton. Idempotent.
///
/// Creates (when missing) at `app/web/` — the canonical default
/// client location per `docs/project-structure.md`. Multi-client
/// projects migrate to `app/clients/<name>/` explicitly (no
/// automated scaffold flow for that yet; the migration is mechanical).
///
/// Emits the **canonical 6+6 closed catalog** per
/// `docs/decisions/client_src_canonical_architecture_2026-05-17.md` §3:
///
/// - `app/web/{index.html, main.tsx}` (entrypoints)
/// - `app/web/shell/{root.tsx, layout.tsx, error_boundary.tsx}` (1/7)
/// - `app/web/routes/index.tsx` (2/7, placeholder)
/// - `app/web/ui/{forms, feedback, navigation, display, overlays, layout}/.gitkeep` (3/7, 7+6)
/// - `app/web/ui/forms/{Button.tsx, Input.tsx}` (Wave K — W2 Shadcn seeds)
/// - `app/web/ui/feedback/Toast.tsx` (Wave K — W2 Shadcn seed; Sonner wrapper)
/// - `app/web/ui/display/Card.tsx` (Wave K — W2 Shadcn seed)
/// - `app/web/ui/overlays/Dialog.tsx` (Wave K — W2 Shadcn seed; Radix Dialog)
/// - `app/web/ui/layout/Stack.tsx` (Wave K — W2 Shadcn seed; pure Tailwind)
/// - `app/web/theme/{globals.css, theme_provider.tsx, cn.ts}` (4/7)
/// - `app/web/state/app_store.ts` (5/7, Zustand placeholder)
/// - `app/web/assets/.gitkeep` (6/7)
/// - `app/web/cells/.gitkeep` (7/7, per §3.1 amendment 2026-05-17;
///   subdirs land per-feature as the canonical Lazurite pilot adopts)
/// - `app/web/{tailwind.config.ts, tsconfig.json, vite.config.ts, package.json}` (configs)
/// - root `.gitignore`
///
/// The output is guaranteed to satisfy `VOCAB-CLIENT-SRC-001` — a
/// fresh scaffold produces ZERO doctor diagnostics. The
/// `scaffold_web_satisfies_vocab_client_src_001` test enforces this
/// invariant.
///
/// Appends `[frontends.web]` to `Lazurite.toml` if no such block exists.
pub fn scaffold_frontend_web(project_root: &Path, _app_name: &str) -> Result<()> {
    ensure_dir(project_root)?;

    let web_dir = project_root.join("app").join("web");
    let shell_dir = web_dir.join("shell");
    let routes_dir = web_dir.join("routes");
    let ui_dir = web_dir.join("ui");
    let theme_dir = web_dir.join("theme");
    let state_dir = web_dir.join("state");
    let assets_dir = web_dir.join("assets");
    let cells_dir = web_dir.join("cells");

    // Top-level 7/7 closed catalog (per
    // `[[client_src_canonical_architecture_2026-05-17]]` §3.1, amended
    // 2026-05-17 to include `cells/` as the 7th folder — resolves
    // tsc module-resolution for handcrafted feature cells).
    fs::create_dir_all(&shell_dir).with_context(|| format!("creating {}", shell_dir.display()))?;
    fs::create_dir_all(&routes_dir)
        .with_context(|| format!("creating {}", routes_dir.display()))?;
    fs::create_dir_all(&ui_dir).with_context(|| format!("creating {}", ui_dir.display()))?;
    fs::create_dir_all(&theme_dir).with_context(|| format!("creating {}", theme_dir.display()))?;
    fs::create_dir_all(&state_dir).with_context(|| format!("creating {}", state_dir.display()))?;
    fs::create_dir_all(&assets_dir)
        .with_context(|| format!("creating {}", assets_dir.display()))?;
    fs::create_dir_all(&cells_dir).with_context(|| format!("creating {}", cells_dir.display()))?;

    // ui/ 6/6 closed sub-catalog (per §3.2). Each sub-dir gets a
    // `.gitkeep` so the canonical shape is present even before any
    // primitive lands. Wave K (2026-05-17) seeds one Shadcn-compose
    // primitive per kind on top of the .gitkeep (the gitkeep stays so
    // empty kinds — i.e. `navigation/` — keep their canonical shape).
    for ui_sub in WEB_UI_SUBDIRS {
        let path = ui_dir.join(ui_sub);
        fs::create_dir_all(&path).with_context(|| format!("creating {}", path.display()))?;
        write_if_absent(&path.join(".gitkeep"), "")?;
    }

    // Wave K — W2 Shadcn-seed primitives (one essential per kind, v0
    // closed-set = 6 total). User owns each file post-scaffold; Lazuli
    // never overwrites. Anchor: docs/proposals/
    // lazurite-frontend-stack-web-grading-2026-05-17.md §W2.
    write_if_absent(
        &ui_dir.join("forms").join("Button.tsx"),
        templates::FRONTEND_WEB_UI_BUTTON_TSX,
    )?;
    write_if_absent(
        &ui_dir.join("forms").join("Input.tsx"),
        templates::FRONTEND_WEB_UI_INPUT_TSX,
    )?;
    write_if_absent(
        &ui_dir.join("feedback").join("Toast.tsx"),
        templates::FRONTEND_WEB_UI_TOAST_TSX,
    )?;
    write_if_absent(
        &ui_dir.join("display").join("Card.tsx"),
        templates::FRONTEND_WEB_UI_CARD_TSX,
    )?;
    write_if_absent(
        &ui_dir.join("overlays").join("Dialog.tsx"),
        templates::FRONTEND_WEB_UI_DIALOG_TSX,
    )?;
    write_if_absent(
        &ui_dir.join("layout").join("Stack.tsx"),
        templates::FRONTEND_WEB_UI_STACK_TSX,
    )?;

    write_if_absent(
        &web_dir.join("index.html"),
        templates::FRONTEND_WEB_INDEX_HTML,
    )?;
    write_if_absent(&web_dir.join("main.tsx"), templates::FRONTEND_WEB_MAIN_TSX)?;
    write_if_absent(
        &shell_dir.join("root.tsx"),
        templates::FRONTEND_WEB_ROOT_TSX,
    )?;
    write_if_absent(
        &shell_dir.join("layout.tsx"),
        templates::FRONTEND_WEB_LAYOUT_TSX,
    )?;
    write_if_absent(
        &shell_dir.join("error_boundary.tsx"),
        templates::FRONTEND_WEB_ERROR_BOUNDARY_TSX,
    )?;

    // routes/ placeholder — minimal route component until v0.2 codegen
    // emits a `routes.gen.ts` table from `.lzx` view declarations.
    write_if_absent(
        &routes_dir.join("index.tsx"),
        templates::FRONTEND_WEB_ROUTES_INDEX_TSX,
    )?;

    write_if_absent(
        &theme_dir.join("globals.css"),
        templates::FRONTEND_THEME_GLOBALS_CSS,
    )?;
    write_if_absent(
        &theme_dir.join("theme_provider.tsx"),
        templates::FRONTEND_THEME_PROVIDER_TSX,
    )?;
    // Wave K — `cn()` helper used by every Shadcn-seed primitive.
    write_if_absent(
        &theme_dir.join("cn.ts"),
        templates::FRONTEND_WEB_THEME_CN_TS,
    )?;

    // state/ — Zustand placeholder per W1 pick.
    write_if_absent(
        &state_dir.join("app_store.ts"),
        templates::FRONTEND_WEB_STATE_APP_STORE_TS,
    )?;

    // assets/ — empty for v0; brand artifacts land here per pilot.
    write_if_absent(&assets_dir.join(".gitkeep"), "")?;

    // cells/ — empty for v0; subdirs land per-feature
    // (`cells/<feature>/<name>.tsx`) as the pilot authors handcrafted
    // slot widgets. Per §3.3 amendment 2026-05-17 — cells live in the
    // client because they're audience-projections that need
    // node_modules walk-up reach for tsc.
    write_if_absent(&cells_dir.join(".gitkeep"), "")?;

    write_if_absent(
        &web_dir.join("tailwind.config.ts"),
        templates::FRONTEND_TAILWIND_CONFIG_TS,
    )?;
    write_if_absent(
        &web_dir.join("tsconfig.json"),
        templates::FRONTEND_TSCONFIG_JSON,
    )?;
    write_if_absent(
        &web_dir.join("vite.config.ts"),
        templates::FRONTEND_VITE_CONFIG_TS,
    )?;
    write_if_absent(
        &web_dir.join("package.json"),
        templates::FRONTEND_PACKAGE_JSON,
    )?;
    write_if_absent(
        &project_root.join(".gitignore"),
        templates::FRONTEND_GITIGNORE,
    )?;

    append_manifest_block(
        &project_root.join("Lazurite.toml"),
        "[frontends.web]",
        templates::FRONTEND_MANIFEST_WEB_SNIPPET,
    )?;

    Ok(())
}

/// `ui/` 6-kind closed sub-catalog per
/// `[[client_src_canonical_architecture_2026-05-17]]` §3.2. Order is
/// the source-of-truth ordering; `scaffold_frontend_web` materializes
/// each as an empty sub-dir with a `.gitkeep` so the canonical shape
/// is present even before any primitive lands.
const WEB_UI_SUBDIRS: &[&str] = &[
    "forms",
    "feedback",
    "navigation",
    "display",
    "overlays",
    "layout",
];

/// Scaffold the mobile (Expo Router) frontend skeleton. Idempotent.
///
/// Mobile is always a `clients/` slice (not the default `app/web/`)
/// because it targets a different runtime — per the singular-vs-plural
/// rule in `docs/project-structure.md`, different runtimes always get
/// separate clients. So this writer emits at `app/clients/mobile/`.
///
/// Creates (when missing) under `app/clients/mobile/`:
/// - `app/_layout.tsx` — one-line re-export of the regen body that
///   `lazuli generate ts` writes to `dist/ts-mobile/runtime/layout`
///   (per `docs/proposals/mobile-target.md` §5.4).
/// - `app/index.tsx` — placeholder home; user customizes once a
///   `surface ... mobile` view scaffolds a real route file.
/// - `shell/client.ts` — `LazuliClient` construction (baseUrl from
///   `process.env.EXPO_PUBLIC_API_URL`).
/// - `app.json` — Expo manifest (name, slug, scheme, expo-router plugin).
/// - `babel.config.js`, `metro.config.js`, `tsconfig.json` — Expo
///   project plumbing.
/// - `package.json` — Expo SDK, expo-router, react-native,
///   AsyncStorage, `@lazuli/runtime` workspace dep.
/// - `.gitignore` (project root) gains Expo-specific ignores.
///
/// Appends `[frontends.mobile]` to `Lazurite.toml` if no such block
/// exists.
pub fn scaffold_frontend_mobile(project_root: &Path, _app_name: &str) -> Result<()> {
    ensure_dir(project_root)?;

    let mobile_dir = project_root.join("app").join("clients").join("mobile");
    let app_dir = mobile_dir.join("app");
    let shell_dir = mobile_dir.join("shell");
    fs::create_dir_all(&app_dir).with_context(|| format!("creating {}", app_dir.display()))?;
    fs::create_dir_all(&shell_dir).with_context(|| format!("creating {}", shell_dir.display()))?;

    write_if_absent(
        &app_dir.join("_layout.tsx"),
        templates::FRONTEND_MOBILE_APP_LAYOUT_TSX,
    )?;
    write_if_absent(
        &app_dir.join("index.tsx"),
        templates::FRONTEND_MOBILE_APP_INDEX_TSX,
    )?;
    write_if_absent(
        &shell_dir.join("client.ts"),
        templates::FRONTEND_MOBILE_SHELL_CLIENT_TS,
    )?;

    write_if_absent(
        &mobile_dir.join("app.json"),
        templates::FRONTEND_MOBILE_APP_JSON,
    )?;
    write_if_absent(
        &mobile_dir.join("babel.config.js"),
        templates::FRONTEND_MOBILE_BABEL_CONFIG,
    )?;
    write_if_absent(
        &mobile_dir.join("metro.config.js"),
        templates::FRONTEND_MOBILE_METRO_CONFIG,
    )?;
    write_if_absent(
        &mobile_dir.join("tsconfig.json"),
        templates::FRONTEND_MOBILE_TSCONFIG,
    )?;
    write_if_absent(
        &mobile_dir.join("package.json"),
        templates::FRONTEND_MOBILE_PACKAGE_JSON,
    )?;

    // Project-level .gitignore: prefer the shared web template when
    // present (it already covers node_modules/, dist/, .vite/, etc.);
    // append Expo-specific ignores idempotently.
    let gitignore_path = project_root.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, templates::FRONTEND_GITIGNORE)
            .with_context(|| format!("writing {}", gitignore_path.display()))?;
    }
    append_manifest_block(
        &gitignore_path,
        "# Expo",
        templates::FRONTEND_MOBILE_GITIGNORE,
    )?;

    append_manifest_block(
        &project_root.join("Lazurite.toml"),
        "[frontends.mobile]",
        templates::FRONTEND_MANIFEST_MOBILE_SNIPPET,
    )?;

    Ok(())
}

// ----- internals -----

fn ensure_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("creating {}", path.display()))
}

/// Write `content` to `path` only if the path does not already exist.
/// This is the idempotency guard: a second scaffold pass never overwrites
/// user edits.
fn write_if_absent(path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

/// Append `snippet` to `manifest_path` unless `header` already appears
/// in the existing manifest (idempotency: re-running won't double-append).
/// If the manifest does not exist, it is created from scratch with just
/// the snippet (orchestrator usually writes `Lazurite.toml` itself; this
/// branch exists for tests / `--in-place` usage).
fn append_manifest_block(manifest_path: &Path, header: &str, snippet: &str) -> Result<()> {
    let manifest_buf: PathBuf = manifest_path.to_path_buf();
    let existing = if manifest_buf.exists() {
        fs::read_to_string(&manifest_buf)
            .with_context(|| format!("reading {}", manifest_buf.display()))?
    } else {
        String::new()
    };

    if existing.contains(header) {
        return Ok(());
    }

    let mut out = existing;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(snippet);

    fs::write(&manifest_buf, out).with_context(|| format!("writing {}", manifest_buf.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal inline tempdir helper (mirrors the pattern used by
    /// `cmd_generate_feature::tests`). Avoids adding a `tempfile`
    /// dev-dep for one module.
    mod tempfile {
        use std::fs;
        use std::io;
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        static COUNTER: AtomicU64 = AtomicU64::new(0);

        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            pub fn new() -> io::Result<Self> {
                let suffix = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                let bump = COUNTER.fetch_add(1, Ordering::SeqCst);
                let path = std::env::temp_dir().join(format!(
                    "lazuli-new-frontends-test-{}-{suffix}-{bump}",
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

    fn tempdir() -> tempfile::TempDir {
        tempfile::TempDir::new().unwrap()
    }

    #[test]
    fn scaffold_web_creates_all_expected_files() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        // app/web/
        assert!(
            root.join("app/web/index.html").exists(),
            "index.html missing"
        );
        assert!(
            root.join("app/web/main.tsx").exists(),
            "main.tsx missing"
        );
        assert!(
            root.join("app/web/shell/root.tsx").exists(),
            "root.tsx missing"
        );
        assert!(
            root.join("app/web/shell/layout.tsx").exists(),
            "layout.tsx missing"
        );
        assert!(
            root.join("app/web/shell/error_boundary.tsx").exists(),
            "error_boundary.tsx missing"
        );

        // app/web/routes/
        assert!(
            root.join("app/web/routes/index.tsx").exists(),
            "routes/index.tsx missing"
        );

        // app/web/theme/
        assert!(
            root.join("app/web/theme/globals.css").exists(),
            "globals.css missing"
        );
        assert!(
            root.join("app/web/theme/theme_provider.tsx").exists(),
            "theme_provider.tsx missing"
        );
        assert!(
            root.join("app/web/theme/cn.ts").exists(),
            "theme/cn.ts missing (Wave K — cn() helper)"
        );

        // app/web/state/
        assert!(
            root.join("app/web/state/app_store.ts").exists(),
            "state/app_store.ts missing"
        );

        // app/web/assets/
        assert!(
            root.join("app/web/assets/.gitkeep").exists(),
            "assets/.gitkeep missing"
        );

        // app/web/ui/{6 closed kinds}
        for ui_sub in WEB_UI_SUBDIRS {
            let gitkeep = root.join("app/web/ui").join(ui_sub).join(".gitkeep");
            assert!(
                gitkeep.exists(),
                "ui/{}/.gitkeep missing (canonical 6-kind closed sub-catalog)",
                ui_sub
            );
        }

        // Wave K — W2 Shadcn-seed primitives (one essential per kind,
        // v0 closed set = 6). Anchor: docs/proposals/
        // lazurite-frontend-stack-web-grading-2026-05-17.md §W2.
        for (rel, label) in [
            ("app/web/ui/forms/Button.tsx", "Button"),
            ("app/web/ui/forms/Input.tsx", "Input"),
            ("app/web/ui/feedback/Toast.tsx", "Toast"),
            ("app/web/ui/display/Card.tsx", "Card"),
            ("app/web/ui/overlays/Dialog.tsx", "Dialog"),
            ("app/web/ui/layout/Stack.tsx", "Stack"),
        ] {
            assert!(
                root.join(rel).exists(),
                "Wave K primitive {} missing at {}",
                label,
                rel,
            );
        }

        assert!(
            root.join("app/web/tailwind.config.ts").exists(),
            "tailwind.config.ts missing"
        );
        assert!(
            root.join("app/web/tsconfig.json").exists(),
            "tsconfig.json missing"
        );
        assert!(
            root.join("app/web/vite.config.ts").exists(),
            "vite.config.ts missing"
        );
    }

    #[test]
    fn scaffold_web_writes_package_and_gitignore() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        let pkg = fs::read_to_string(root.join("app/web/package.json")).unwrap();
        // Baseline deps preserved.
        assert!(pkg.contains("\"@tanstack/react-query\""));
        assert!(pkg.contains("\"@lazuli/runtime\""));
        assert!(pkg.contains("\"react-hook-form\""));
        assert!(pkg.contains("\"tailwindcss\""));

        // Wave G — W1-W7 Tier-2 picks present.
        assert!(pkg.contains("\"zustand\""), "W1 Zustand missing");
        assert!(
            pkg.contains("\"@radix-ui/react-slot\""),
            "W2 Radix Slot missing"
        );
        assert!(
            pkg.contains("\"@radix-ui/react-dialog\""),
            "Wave K — @radix-ui/react-dialog missing (Dialog primitive dep)"
        );
        assert!(
            pkg.contains("\"class-variance-authority\""),
            "W2 cva missing"
        );
        assert!(pkg.contains("\"clsx\""), "W2 clsx missing");
        assert!(
            pkg.contains("\"tailwind-merge\""),
            "W2 tailwind-merge missing"
        );
        assert!(pkg.contains("\"lucide-react\""), "W3 Lucide missing");
        assert!(pkg.contains("\"date-fns\""), "W4 date-fns missing");
        assert!(pkg.contains("\"sonner\""), "W5 Sonner missing");
        assert!(pkg.contains("\"vitest\""), "W6 Vitest missing");
        assert!(
            pkg.contains("\"@playwright/test\""),
            "W6 Playwright missing"
        );
        assert!(
            pkg.contains("\"@testing-library/react\""),
            "W6 RTL missing"
        );
        assert!(
            pkg.contains("\"@biomejs/biome\""),
            "W7 Biome missing"
        );

        // W7 — `shadcn-ui` is a SCAFFOLD SEED (copy-paste recipes), not a dep.
        assert!(
            !pkg.contains("\"shadcn-ui\""),
            "shadcn-ui must NOT be a dep (W2 seed-only)"
        );
        // W7 — ESLint/Prettier replaced by Biome.
        assert!(
            !pkg.contains("\"eslint\""),
            "eslint should not be present (W7 picks Biome)"
        );
        assert!(
            !pkg.contains("\"prettier\""),
            "prettier should not be present (W7 picks Biome)"
        );

        // Scripts surfaces.
        assert!(pkg.contains("\"test:unit\""));
        assert!(pkg.contains("\"test:e2e\""));
        assert!(pkg.contains("\"lint\""));
        assert!(pkg.contains("\"format\""));

        let gi = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gi.contains("node_modules/"));
        assert!(gi.contains("dist/"));
    }

    #[test]
    fn scaffold_web_appends_manifest_block_when_missing() {
        let project = tempdir();
        let root = project.path();

        // pre-existing manifest with no frontends block
        fs::write(
            root.join("Lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        scaffold_frontend_web(root, "demo").unwrap();

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.web]"));
        assert!(manifest.contains("target = \"tanstack-vite\""));
        assert!(manifest.contains("source = \"app/web\""));
        assert!(manifest.contains("out = \"dist/ts-web\""));
        assert!(manifest.contains("[lazuli]")); // pre-existing block preserved
    }

    #[test]
    fn scaffold_web_does_not_double_append_manifest() {
        let project = tempdir();
        let root = project.path();

        // manifest already has the block; helper must NOT append again
        let initial = "[frontends.web]\ntarget = \"tanstack-vite\"\nsource = \"app/web\"\nout = \"dist/ts-web\"\naudiences = [\"admin\"]\n";
        fs::write(root.join("Lazurite.toml"), initial).unwrap();

        scaffold_frontend_web(root, "demo").unwrap();

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        let count = manifest.matches("[frontends.web]").count();
        assert_eq!(
            count, 1,
            "[frontends.web] should appear exactly once, got {count}\nmanifest:\n{manifest}"
        );
    }

    #[test]
    fn scaffold_mobile_creates_expected_files_and_appends_manifest() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_mobile(root, "demo").unwrap();

        // Expo Router app/ tree.
        assert!(
            root.join("app/clients/mobile/app/_layout.tsx").exists(),
            "mobile app/_layout.tsx missing"
        );
        assert!(
            root.join("app/clients/mobile/app/index.tsx").exists(),
            "mobile app/index.tsx missing"
        );
        // LazuliClient construction lives under shell/.
        assert!(
            root.join("app/clients/mobile/shell/client.ts").exists(),
            "mobile shell/client.ts missing"
        );
        // Expo project plumbing.
        assert!(root.join("app/clients/mobile/app.json").exists(), "app.json missing");
        assert!(
            root.join("app/clients/mobile/babel.config.js").exists(),
            "babel.config.js missing"
        );
        assert!(
            root.join("app/clients/mobile/metro.config.js").exists(),
            "metro.config.js missing"
        );
        assert!(
            root.join("app/clients/mobile/tsconfig.json").exists(),
            "tsconfig.json missing"
        );
        assert!(
            root.join("app/clients/mobile/package.json").exists(),
            "mobile package.json missing"
        );

        // _layout.tsx is the user-owned re-export of the regen body.
        let layout = fs::read_to_string(root.join("app/clients/mobile/app/_layout.tsx")).unwrap();
        assert!(layout.contains("dist/ts-mobile/runtime/layout"));

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.mobile]"));
        assert!(manifest.contains("target = \"expo\""));
        assert!(manifest.contains("source = \"app/clients/mobile\""));
        assert!(manifest.contains("out = \"dist/ts-mobile\""));

        let pkg = fs::read_to_string(root.join("app/clients/mobile/package.json")).unwrap();
        assert!(pkg.contains("\"expo\""));
        assert!(pkg.contains("\"react-native\""));
        assert!(pkg.contains("\"expo-router\""));
        assert!(pkg.contains("\"@react-native-async-storage/async-storage\""));
        assert!(pkg.contains("\"@lazuli/runtime\""));

        // Wave G — M2-M6 mobile Tier-2 picks present.
        assert!(
            pkg.contains("\"expo-secure-store\""),
            "M3-secrets missing"
        );
        assert!(
            pkg.contains("\"react-native-reanimated\""),
            "M4 Reanimated missing"
        );
        assert!(
            pkg.contains("\"expo-notifications\""),
            "M5 expo-notifications missing"
        );
        assert!(
            pkg.contains("\"lucide-react-native\""),
            "M2 lucide-react-native missing"
        );
        assert!(pkg.contains("\"zustand\""), "M6 Zustand (inherits W1) missing");

        // .gitignore covers Expo-specific paths.
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains(".expo/"));

        // babel.config.js must list `react-native-reanimated/plugin` LAST
        // (M4 pairing rule: Reanimated requires its plugin to be last).
        let babel =
            fs::read_to_string(root.join("app/clients/mobile/babel.config.js")).unwrap();
        assert!(
            babel.contains("react-native-reanimated/plugin"),
            "babel.config.js must include reanimated plugin"
        );

        // app.json plugin entries — expo-notifications must be present.
        let app_json = fs::read_to_string(root.join("app/clients/mobile/app.json")).unwrap();
        assert!(
            app_json.contains("expo-notifications"),
            "app.json must list expo-notifications plugin (M5 pairing rule)"
        );
    }

    #[test]
    fn scaffold_web_is_idempotent_on_files_and_manifest() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        // User edits one of the scaffolded files; the second pass must not clobber it.
        let edited_root_tsx = "// user-customized content\n";
        fs::write(root.join("app/web/shell/root.tsx"), edited_root_tsx).unwrap();

        scaffold_frontend_web(root, "demo").unwrap();

        // file untouched
        let after = fs::read_to_string(root.join("app/web/shell/root.tsx")).unwrap();
        assert_eq!(
            after, edited_root_tsx,
            "second pass must NOT overwrite user edit"
        );

        // manifest still has exactly one block
        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert_eq!(manifest.matches("[frontends.web]").count(), 1);
    }

    #[test]
    fn scaffold_mobile_is_idempotent() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_mobile(root, "demo").unwrap();
        let edited = "// user-customized mobile layout\n";
        fs::write(root.join("app/clients/mobile/app/_layout.tsx"), edited).unwrap();

        scaffold_frontend_mobile(root, "demo").unwrap();

        let after = fs::read_to_string(root.join("app/clients/mobile/app/_layout.tsx")).unwrap();
        assert_eq!(after, edited);

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert_eq!(manifest.matches("[frontends.mobile]").count(), 1);

        // .gitignore Expo block is only appended once.
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert_eq!(gitignore.matches("# Expo").count(), 1);
    }

    #[test]
    fn web_and_mobile_compose_in_same_project() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();
        scaffold_frontend_mobile(root, "demo").unwrap();

        assert!(root.join("app/web/shell/root.tsx").exists());
        assert!(root.join("app/clients/mobile/app/_layout.tsx").exists());

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.web]"));
        assert!(manifest.contains("[frontends.mobile]"));
    }

    /// Smoke test that the FRONTEND_* template consts aren't empty —
    /// catches a future accidental truncation of `templates.rs`.
    #[test]
    fn frontend_template_consts_are_nonempty() {
        // web
        assert!(!templates::FRONTEND_WEB_INDEX_HTML.is_empty());
        assert!(!templates::FRONTEND_WEB_MAIN_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_ROOT_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_LAYOUT_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_ERROR_BOUNDARY_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_ROUTES_INDEX_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_STATE_APP_STORE_TS.is_empty());
        assert!(!templates::FRONTEND_THEME_GLOBALS_CSS.is_empty());
        assert!(!templates::FRONTEND_THEME_PROVIDER_TSX.is_empty());
        assert!(!templates::FRONTEND_TAILWIND_CONFIG_TS.is_empty());
        assert!(!templates::FRONTEND_TSCONFIG_JSON.is_empty());
        assert!(!templates::FRONTEND_VITE_CONFIG_TS.is_empty());
        assert!(!templates::FRONTEND_PACKAGE_JSON.is_empty());
        assert!(!templates::FRONTEND_GITIGNORE.is_empty());
        // Wave K — Shadcn-seed primitives + cn() helper.
        assert!(!templates::FRONTEND_WEB_THEME_CN_TS.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_BUTTON_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_INPUT_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_TOAST_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_CARD_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_DIALOG_TSX.is_empty());
        assert!(!templates::FRONTEND_WEB_UI_STACK_TSX.is_empty());

        // mobile
        assert!(!templates::FRONTEND_MOBILE_APP_LAYOUT_TSX.is_empty());
        assert!(!templates::FRONTEND_MOBILE_APP_INDEX_TSX.is_empty());
        assert!(!templates::FRONTEND_MOBILE_APP_JSON.is_empty());
        assert!(!templates::FRONTEND_MOBILE_BABEL_CONFIG.is_empty());
        assert!(!templates::FRONTEND_MOBILE_METRO_CONFIG.is_empty());
        assert!(!templates::FRONTEND_MOBILE_TSCONFIG.is_empty());
        assert!(!templates::FRONTEND_MOBILE_SHELL_CLIENT_TS.is_empty());
        assert!(!templates::FRONTEND_MOBILE_GITIGNORE.is_empty());
        assert!(!templates::FRONTEND_MOBILE_PACKAGE_JSON.is_empty());

        // manifest snippets
        assert!(templates::FRONTEND_MANIFEST_WEB_SNIPPET.contains("[frontends.web]"));
        assert!(templates::FRONTEND_MANIFEST_MOBILE_SNIPPET.contains("[frontends.mobile]"));
    }

    /// Make sure no LF-newline got mangled into CRLF (templates must be
    /// cross-platform; Lazuli runs on Windows). r#"..."# preserves
    /// whatever is in source; this guards against future edits that
    /// might paste CRLF from a Windows clipboard.
    #[test]
    fn templates_use_lf_newlines_only() {
        let all = [
            templates::FRONTEND_WEB_INDEX_HTML,
            templates::FRONTEND_WEB_MAIN_TSX,
            templates::FRONTEND_WEB_ROOT_TSX,
            templates::FRONTEND_WEB_LAYOUT_TSX,
            templates::FRONTEND_WEB_ERROR_BOUNDARY_TSX,
            templates::FRONTEND_WEB_ROUTES_INDEX_TSX,
            templates::FRONTEND_WEB_STATE_APP_STORE_TS,
            templates::FRONTEND_WEB_THEME_CN_TS,
            templates::FRONTEND_WEB_UI_BUTTON_TSX,
            templates::FRONTEND_WEB_UI_INPUT_TSX,
            templates::FRONTEND_WEB_UI_TOAST_TSX,
            templates::FRONTEND_WEB_UI_CARD_TSX,
            templates::FRONTEND_WEB_UI_DIALOG_TSX,
            templates::FRONTEND_WEB_UI_STACK_TSX,
            templates::FRONTEND_THEME_GLOBALS_CSS,
            templates::FRONTEND_THEME_PROVIDER_TSX,
            templates::FRONTEND_TAILWIND_CONFIG_TS,
            templates::FRONTEND_TSCONFIG_JSON,
            templates::FRONTEND_VITE_CONFIG_TS,
            templates::FRONTEND_PACKAGE_JSON,
            templates::FRONTEND_GITIGNORE,
            templates::FRONTEND_MOBILE_APP_LAYOUT_TSX,
            templates::FRONTEND_MOBILE_APP_INDEX_TSX,
            templates::FRONTEND_MOBILE_APP_JSON,
            templates::FRONTEND_MOBILE_BABEL_CONFIG,
            templates::FRONTEND_MOBILE_METRO_CONFIG,
            templates::FRONTEND_MOBILE_TSCONFIG,
            templates::FRONTEND_MOBILE_SHELL_CLIENT_TS,
            templates::FRONTEND_MOBILE_GITIGNORE,
            templates::FRONTEND_MOBILE_PACKAGE_JSON,
            templates::FRONTEND_MANIFEST_WEB_SNIPPET,
            templates::FRONTEND_MANIFEST_MOBILE_SNIPPET,
        ];
        for (i, s) in all.iter().enumerate() {
            assert!(
                !s.contains('\r'),
                "template index {i} contains CR — templates must be LF-only for cross-platform output"
            );
        }
    }

    /// Wave K invariant: each Shadcn-seed primitive carries the
    /// scaffold-seed banner ("User owns this file") and references
    /// the canonical `@web/theme/cn` helper. Catches accidental
    /// drift between the templates and the W2 scaffold-seed pick.
    #[test]
    fn scaffold_web_seeds_carry_banner_and_cn_import() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        // `cn.ts` ships and is the standard tailwind-merge + clsx recipe.
        let cn_ts = fs::read_to_string(root.join("app/web/theme/cn.ts")).unwrap();
        assert!(cn_ts.contains("tailwind-merge"));
        assert!(cn_ts.contains("clsx"));
        assert!(cn_ts.contains("export function cn"));
        assert!(
            cn_ts.contains("User owns this file"),
            "cn.ts missing scaffold-seed banner"
        );

        // Every primitive carries the banner; each one that references
        // `cn()` imports it from `@web/theme/cn`.
        let primitives = [
            ("app/web/ui/forms/Button.tsx", true),
            ("app/web/ui/forms/Input.tsx", true),
            ("app/web/ui/feedback/Toast.tsx", false), // Sonner wrapper has no cn() use.
            ("app/web/ui/display/Card.tsx", true),
            ("app/web/ui/overlays/Dialog.tsx", true),
            ("app/web/ui/layout/Stack.tsx", true),
        ];
        for (rel, uses_cn) in primitives {
            let body = fs::read_to_string(root.join(rel)).unwrap();
            assert!(
                body.contains("User owns this file"),
                "{} missing scaffold-seed banner",
                rel,
            );
            if uses_cn {
                assert!(
                    body.contains("@web/theme/cn"),
                    "{} must import cn from @web/theme/cn",
                    rel,
                );
            }
        }

        // Button uses CVA + Radix Slot for asChild (the W2-specified shape).
        let button = fs::read_to_string(root.join("app/web/ui/forms/Button.tsx")).unwrap();
        assert!(button.contains("class-variance-authority"));
        assert!(button.contains("@radix-ui/react-slot"));
        assert!(button.contains("asChild"));

        // Dialog uses @radix-ui/react-dialog (the W2-specified shape).
        let dialog = fs::read_to_string(root.join("app/web/ui/overlays/Dialog.tsx")).unwrap();
        assert!(dialog.contains("@radix-ui/react-dialog"));

        // Stack is pure Tailwind — no Radix import.
        let stack = fs::read_to_string(root.join("app/web/ui/layout/Stack.tsx")).unwrap();
        assert!(!stack.contains("@radix-ui"));
        assert!(stack.contains("class-variance-authority"));

        // Toast re-exports Sonner — no Radix import.
        let toast = fs::read_to_string(root.join("app/web/ui/feedback/Toast.tsx")).unwrap();
        assert!(toast.contains("sonner"));
    }

    /// Wave G invariant: a fresh `scaffold_frontend_web` output must
    /// satisfy `VOCAB-CLIENT-SRC-001` — zero doctor diagnostics. The
    /// scaffold emits the canonical 6+6 closed catalog per
    /// `[[client_src_canonical_architecture_2026-05-17]]` §3, so the
    /// doctor walker (which checks `app/web/` singular topology) must
    /// see ONLY the six allowed top-level folders and ONLY the six
    /// allowed `ui/` children.
    #[test]
    fn scaffold_web_satisfies_vocab_client_src_001() {
        use crate::doctor::folder::vocab_client_src_001;

        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        let findings = vocab_client_src_001::check(root);
        assert!(
            findings.is_empty(),
            "fresh scaffold must produce zero VOCAB-CLIENT-SRC-001 \
             diagnostics; got {} finding(s): {:?}",
            findings.len(),
            findings
        );

        // Belt-and-braces: confirm each of the six allowed top-level
        // folders exists (this is what makes the doctor walker happy).
        for top in &["shell", "routes", "ui", "theme", "state", "assets"] {
            assert!(
                root.join("app/web").join(top).is_dir(),
                "canonical top-level folder `{}` missing",
                top
            );
        }
        // And each of the six allowed `ui/` children.
        for ui_sub in WEB_UI_SUBDIRS {
            assert!(
                root.join("app/web/ui").join(ui_sub).is_dir(),
                "canonical ui/{} missing",
                ui_sub
            );
        }
    }
}
