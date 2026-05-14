//! Frontend scaffolding helpers for `lazuli new --frontends <list>`.
//!
//! Implements L0 #1 §6.1 — when the user passes `--frontends web`
//! and/or `--frontends mobile` to `lazuli new`, this module materializes
//! the canonical app-shell, theme, config, and manifest entries.
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
/// - `app/shell/web/{root.tsx, layout.tsx, error_boundary.tsx}`
/// - `app/theme/{globals.css, theme_provider.tsx}`
/// - `tailwind.config.ts`, `tsconfig.json`, `vite.config.ts`, `package.json`, `.gitignore`
///
/// Appends `[frontends.web]` to `lazurite.toml` if no such block exists.
pub fn scaffold_frontend_web(project_root: &Path, _app_name: &str) -> Result<()> {
    ensure_dir(project_root)?;

    let shell_dir = project_root.join("app").join("shell").join("web");
    let theme_dir = project_root.join("app").join("theme");
    fs::create_dir_all(&shell_dir)
        .with_context(|| format!("creating {}", shell_dir.display()))?;
    fs::create_dir_all(&theme_dir)
        .with_context(|| format!("creating {}", theme_dir.display()))?;

    write_if_absent(&shell_dir.join("root.tsx"), templates::FRONTEND_WEB_ROOT_TSX)?;
    write_if_absent(&shell_dir.join("layout.tsx"), templates::FRONTEND_WEB_LAYOUT_TSX)?;
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
        &project_root.join("tailwind.config.ts"),
        templates::FRONTEND_TAILWIND_CONFIG_TS,
    )?;
    write_if_absent(
        &project_root.join("tsconfig.json"),
        templates::FRONTEND_TSCONFIG_JSON,
    )?;
    write_if_absent(
        &project_root.join("vite.config.ts"),
        templates::FRONTEND_VITE_CONFIG_TS,
    )?;
    write_if_absent(
        &project_root.join("package.json"),
        templates::FRONTEND_PACKAGE_JSON,
    )?;
    write_if_absent(
        &project_root.join(".gitignore"),
        templates::FRONTEND_GITIGNORE,
    )?;

    append_manifest_block(
        &project_root.join("lazurite.toml"),
        "[frontends.web]",
        templates::FRONTEND_MANIFEST_WEB_SNIPPET,
    )?;

    Ok(())
}

/// Scaffold the mobile (Expo) frontend skeleton. Idempotent.
///
/// Creates (when missing):
/// - `app/shell/mobile/root.tsx`
/// - `mobile/package.json` (Expo-specific deps; web's `package.json`
///   is kept separate because Expo and Vite-react ecosystems pin
///   incompatible `react`/`react-dom` versions today).
///
/// Appends `[frontends.mobile]` to `lazurite.toml` if no such block exists.
pub fn scaffold_frontend_mobile(project_root: &Path, _app_name: &str) -> Result<()> {
    ensure_dir(project_root)?;

    let shell_dir = project_root.join("app").join("shell").join("mobile");
    fs::create_dir_all(&shell_dir)
        .with_context(|| format!("creating {}", shell_dir.display()))?;

    write_if_absent(
        &shell_dir.join("root.tsx"),
        templates::FRONTEND_MOBILE_ROOT_TSX,
    )?;

    let mobile_dir = project_root.join("mobile");
    fs::create_dir_all(&mobile_dir)
        .with_context(|| format!("creating {}", mobile_dir.display()))?;
    write_if_absent(
        &mobile_dir.join("package.json"),
        templates::FRONTEND_MOBILE_PACKAGE_JSON,
    )?;

    append_manifest_block(
        &project_root.join("lazurite.toml"),
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
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, content).with_context(|| format!("writing {}", path.display()))
}

/// Append `snippet` to `manifest_path` unless `header` already appears
/// in the existing manifest (idempotency: re-running won't double-append).
/// If the manifest does not exist, it is created from scratch with just
/// the snippet (orchestrator usually writes `lazurite.toml` itself; this
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

    fs::write(&manifest_buf, out)
        .with_context(|| format!("writing {}", manifest_buf.display()))
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

        // app/shell/web/
        assert!(root.join("app/shell/web/root.tsx").exists(), "root.tsx missing");
        assert!(
            root.join("app/shell/web/layout.tsx").exists(),
            "layout.tsx missing"
        );
        assert!(
            root.join("app/shell/web/error_boundary.tsx").exists(),
            "error_boundary.tsx missing"
        );

        // app/theme/
        assert!(root.join("app/theme/globals.css").exists(), "globals.css missing");
        assert!(
            root.join("app/theme/theme_provider.tsx").exists(),
            "theme_provider.tsx missing"
        );

        // root-level config
        assert!(
            root.join("tailwind.config.ts").exists(),
            "tailwind.config.ts missing"
        );
        assert!(root.join("tsconfig.json").exists(), "tsconfig.json missing");
        assert!(root.join("vite.config.ts").exists(), "vite.config.ts missing");
    }

    #[test]
    fn scaffold_web_writes_package_and_gitignore() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        let pkg = fs::read_to_string(root.join("package.json")).unwrap();
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
            root.join("lazurite.toml"),
            "[lazuli]\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        scaffold_frontend_web(root, "demo").unwrap();

        let manifest = fs::read_to_string(root.join("lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.web]"));
        assert!(manifest.contains("target = \"vite-react\""));
        assert!(manifest.contains("out = \"dist/ts-web\""));
        assert!(manifest.contains("[lazuli]")); // pre-existing block preserved
    }

    #[test]
    fn scaffold_web_does_not_double_append_manifest() {
        let project = tempdir();
        let root = project.path();

        // manifest already has the block; helper must NOT append again
        let initial = "[frontends.web]\ntarget = \"vite-react\"\nout = \"dist/ts-web\"\naudiences = [\"admin\"]\n";
        fs::write(root.join("lazurite.toml"), initial).unwrap();

        scaffold_frontend_web(root, "demo").unwrap();

        let manifest = fs::read_to_string(root.join("lazurite.toml")).unwrap();
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

        assert!(
            root.join("app/shell/mobile/root.tsx").exists(),
            "mobile root.tsx missing"
        );
        assert!(
            root.join("mobile/package.json").exists(),
            "mobile package.json missing"
        );

        let manifest = fs::read_to_string(root.join("lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.mobile]"));
        assert!(manifest.contains("target = \"expo\""));
        assert!(manifest.contains("out = \"dist/ts-mobile\""));

        let pkg = fs::read_to_string(root.join("mobile/package.json")).unwrap();
        assert!(pkg.contains("\"expo\""));
        assert!(pkg.contains("\"react-native\""));
    }

    #[test]
    fn scaffold_web_is_idempotent_on_files_and_manifest() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();

        // User edits one of the scaffolded files; the second pass must not clobber it.
        let edited_root_tsx = "// user-customized content\n";
        fs::write(root.join("app/shell/web/root.tsx"), edited_root_tsx).unwrap();

        scaffold_frontend_web(root, "demo").unwrap();

        // file untouched
        let after = fs::read_to_string(root.join("app/shell/web/root.tsx")).unwrap();
        assert_eq!(after, edited_root_tsx, "second pass must NOT overwrite user edit");

        // manifest still has exactly one block
        let manifest = fs::read_to_string(root.join("lazurite.toml")).unwrap();
        assert_eq!(manifest.matches("[frontends.web]").count(), 1);
    }

    #[test]
    fn scaffold_mobile_is_idempotent() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_mobile(root, "demo").unwrap();
        let edited = "// user-customized mobile root\n";
        fs::write(root.join("app/shell/mobile/root.tsx"), edited).unwrap();

        scaffold_frontend_mobile(root, "demo").unwrap();

        let after = fs::read_to_string(root.join("app/shell/mobile/root.tsx")).unwrap();
        assert_eq!(after, edited);

        let manifest = fs::read_to_string(root.join("lazurite.toml")).unwrap();
        assert_eq!(manifest.matches("[frontends.mobile]").count(), 1);
    }

    #[test]
    fn web_and_mobile_compose_in_same_project() {
        let project = tempdir();
        let root = project.path();

        scaffold_frontend_web(root, "demo").unwrap();
        scaffold_frontend_mobile(root, "demo").unwrap();

        assert!(root.join("app/shell/web/root.tsx").exists());
        assert!(root.join("app/shell/mobile/root.tsx").exists());

        let manifest = fs::read_to_string(root.join("lazurite.toml")).unwrap();
        assert!(manifest.contains("[frontends.web]"));
        assert!(manifest.contains("[frontends.mobile]"));
    }

    /// Smoke test that the FRONTEND_* template consts aren't empty —
    /// catches a future accidental truncation of `templates.rs`.
    #[test]
    fn frontend_template_consts_are_nonempty() {
        // web
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
        assert!(!templates::FRONTEND_MOBILE_ROOT_TSX.is_empty());
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
            templates::FRONTEND_MOBILE_ROOT_TSX,
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
