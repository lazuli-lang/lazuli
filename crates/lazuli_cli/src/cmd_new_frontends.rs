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
/// Creates (when missing):
/// - `frontends/web/{index.html,main.tsx}`
/// - `frontends/web/shell/{root.tsx,layout.tsx,error_boundary.tsx}`
/// - `frontends/web/theme/{globals.css,theme_provider.tsx}`
/// - `frontends/web/{tailwind.config.ts,tsconfig.json,vite.config.ts,package.json}`
/// - root `.gitignore`
///
/// Appends `[frontends.web]` to `Lazurite.toml` if no such block exists.
pub fn scaffold_frontend_web(project_root: &Path, _app_name: &str) -> Result<()> {
    ensure_dir(project_root)?;

    let web_dir = project_root.join("frontends").join("web");
    let shell_dir = web_dir.join("shell");
    let theme_dir = web_dir.join("theme");
    fs::create_dir_all(&shell_dir).with_context(|| format!("creating {}", shell_dir.display()))?;
    fs::create_dir_all(&theme_dir).with_context(|| format!("creating {}", theme_dir.display()))?;

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
    write_if_absent(
        &theme_dir.join("globals.css"),
        templates::FRONTEND_THEME_GLOBALS_CSS,
    )?;
    write_if_absent(
        &theme_dir.join("theme_provider.tsx"),
        templates::FRONTEND_THEME_PROVIDER_TSX,
    )?;

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

/// Scaffold the mobile (Expo Router) frontend skeleton. Idempotent.
///
/// Creates (when missing) under `frontends/mobile/`:
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

    let mobile_dir = project_root.join("frontends").join("mobile");
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

        // frontends/web/
        assert!(
            root.join("frontends/web/index.html").exists(),
            "index.html missing"
        );
        assert!(
            root.join("frontends/web/main.tsx").exists(),
            "main.tsx missing"
        );
        assert!(
            root.join("frontends/web/shell/root.tsx").exists(),
            "root.tsx missing"
        );
        assert!(
            root.join("frontends/web/shell/layout.tsx").exists(),
            "layout.tsx missing"
        );
        assert!(
            root.join("frontends/web/shell/error_boundary.tsx").exists(),
            "error_boundary.tsx missing"
        );

        // frontends/web/theme/
        assert!(
            root.join("frontends/web/theme/globals.css").exists(),
            "globals.css missing"
        );
        assert!(
            root.join("frontends/web/theme/theme_provider.tsx").exists(),
            "theme_provider.tsx missing"
        );

        assert!(
            root.join("frontends/web/tailwind.config.ts").exists(),
            "tailwind.config.ts missing"
        );
        assert!(
            root.join("frontends/web/tsconfig.json").exists(),
            "tsconfig.json missing"
        );
        assert!(
            root.join("frontends/web/vite.config.ts").exists(),
            "vite.config.ts missing"
        );
    }

    #[test]
    fn scaffold_web_writes_package_and_gitignore() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        let pkg = fs::read_to_string(root.join("frontends/web/package.json")).unwrap();
        assert!(pkg.contains("\"@tanstack/react-query\""));
        assert!(pkg.contains("\"@lazuli/runtime\""));
        assert!(pkg.contains("\"react-hook-form\""));
        assert!(pkg.contains("\"tailwindcss\""));

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
        assert!(manifest.contains("source = \"frontends/web\""));
        assert!(manifest.contains("out = \"dist/ts-web\""));
        assert!(manifest.contains("[lazuli]")); // pre-existing block preserved
    }

    #[test]
    fn scaffold_web_does_not_double_append_manifest() {
        let project = tempdir();
        let root = project.path();

        // manifest already has the block; helper must NOT append again
        let initial = "[frontends.web]\ntarget = \"tanstack-vite\"\nsource = \"frontends/web\"\nout = \"dist/ts-web\"\naudiences = [\"admin\"]\n";
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
            root.join("frontends/mobile/app/_layout.tsx").exists(),
            "mobile app/_layout.tsx missing"
        );
        assert!(
            root.join("frontends/mobile/app/index.tsx").exists(),
            "mobile app/index.tsx missing"
        );
        // LazuliClient construction lives under shell/.
        assert!(
            root.join("frontends/mobile/shell/client.ts").exists(),
            "mobile shell/client.ts missing"
        );
        // Expo project plumbing.
        assert!(root.join("frontends/mobile/app.json").exists(), "app.json missing");
        assert!(
            root.join("frontends/mobile/babel.config.js").exists(),
            "babel.config.js missing"
        );
        assert!(
            root.join("frontends/mobile/metro.config.js").exists(),
            "metro.config.js missing"
        );
        assert!(
            root.join("frontends/mobile/tsconfig.json").exists(),
            "tsconfig.json missing"
        );
        assert!(
            root.join("frontends/mobile/package.json").exists(),
            "mobile package.json missing"
        );

        // _layout.tsx is the user-owned re-export of the regen body.
        let layout = fs::read_to_string(root.join("frontends/mobile/app/_layout.tsx")).unwrap();
        assert!(layout.contains("dist/ts-mobile/runtime/layout"));

        let manifest = fs::read_to_string(root.join("Lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.mobile]"));
        assert!(manifest.contains("target = \"expo\""));
        assert!(manifest.contains("source = \"frontends/mobile\""));
        assert!(manifest.contains("out = \"dist/ts-mobile\""));

        let pkg = fs::read_to_string(root.join("frontends/mobile/package.json")).unwrap();
        assert!(pkg.contains("\"expo\""));
        assert!(pkg.contains("\"react-native\""));
        assert!(pkg.contains("\"expo-router\""));
        assert!(pkg.contains("\"@react-native-async-storage/async-storage\""));
        assert!(pkg.contains("\"@lazuli/runtime\""));

        // .gitignore covers Expo-specific paths.
        let gitignore = fs::read_to_string(root.join(".gitignore")).unwrap();
        assert!(gitignore.contains(".expo/"));
    }

    #[test]
    fn scaffold_web_is_idempotent_on_files_and_manifest() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        // User edits one of the scaffolded files; the second pass must not clobber it.
        let edited_root_tsx = "// user-customized content\n";
        fs::write(root.join("frontends/web/shell/root.tsx"), edited_root_tsx).unwrap();

        scaffold_frontend_web(root, "demo").unwrap();

        // file untouched
        let after = fs::read_to_string(root.join("frontends/web/shell/root.tsx")).unwrap();
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
        fs::write(root.join("frontends/mobile/app/_layout.tsx"), edited).unwrap();

        scaffold_frontend_mobile(root, "demo").unwrap();

        let after = fs::read_to_string(root.join("frontends/mobile/app/_layout.tsx")).unwrap();
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

        assert!(root.join("frontends/web/shell/root.tsx").exists());
        assert!(root.join("frontends/mobile/app/_layout.tsx").exists());

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
        assert!(!templates::FRONTEND_THEME_GLOBALS_CSS.is_empty());
        assert!(!templates::FRONTEND_THEME_PROVIDER_TSX.is_empty());
        assert!(!templates::FRONTEND_TAILWIND_CONFIG_TS.is_empty());
        assert!(!templates::FRONTEND_TSCONFIG_JSON.is_empty());
        assert!(!templates::FRONTEND_VITE_CONFIG_TS.is_empty());
        assert!(!templates::FRONTEND_PACKAGE_JSON.is_empty());
        assert!(!templates::FRONTEND_GITIGNORE.is_empty());

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
}
