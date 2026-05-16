//! Doctor rule `feature-orphan-component`.
//!
//! Flags user-authored `.tsx` / `.ts` files in non-canonical paths under a
//! Lazurite product repo. The canonical frontend layout is defined by L0 #1,
//! `docs/proposals/lazurite-frontend-folder-canon.md` §4, and the forbidden
//! React/Vite anti-pattern is listed in §5.1.
//!
//! Canonical locations:
//!   - `app/features/<feat>/web/{cells,views/<audience>}/*.{ts,tsx}`
//!   - `app/features/<feat>/mobile/{cells,views/<audience>}/*.{ts,tsx}`
//!   - `frontends/<target>/{shell,theme,ui,hooks,lib}/**/*.{ts,tsx}`
//!   - `frontends/<target>/{main,vite.config,tailwind.config}.ts(x)`
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
            "`{}` is outside the canonical Lazurite frontend layout. Move \
             to `app/features/<feat>/web/views/<audience>/` (page), \
             `app/features/<feat>/web/cells/` (slot impl), or \
             `frontends/<target>/ui/` (shared primitive).",
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
            if !should_skip_dir(&name) {
                walk(root, &path, findings);
            }
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
        assert_eq!(findings[1].path, PathBuf::from("frontends/web/junk/Bad.tsx"));
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
}
