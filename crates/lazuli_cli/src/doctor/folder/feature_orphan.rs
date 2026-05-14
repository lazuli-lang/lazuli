//! Doctor rule `feature-orphan-component`.
//!
//! Flags user-authored `.tsx` / `.ts` files in non-canonical paths under a
//! Lazurite product repo. The canonical frontend layout is defined by L0 #1,
//! `docs/proposals/lazurite-frontend-folder-canon.md` §4, and the forbidden
//! React/Vite anti-pattern is listed in §5.1.
//!
//! Canonical locations:
//!   - `features/<feat>/web/{cells,views/<audience>}/*.{ts,tsx}`
//!   - `features/<feat>/mobile/{cells,views/<audience>}/*.{ts,tsx}`
//!   - `app/{shell/{web,mobile},theme,ui,lib}/**/*.{ts,tsx}`
//!
//! Anything else, such as `src/components/`, `app/components/`, or
//! `lib/components/`, is an orphan component.

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
             to `features/<feat>/web/views/<audience>/` (page), \
             `features/<feat>/web/cells/` (slot impl), or `app/ui/` \
             (shared primitive).",
            path.display()
        )
    }
}

/// Walk `root` recursively, classify each `.tsx`/`.ts` file as canonical
/// or orphan, and return findings for the orphans.
///
/// Skips directories: `node_modules`, `dist`, `.lazuli`, `.git`, `target`,
/// `.next`, `.expo`, `.cache`, `coverage`.
///
/// Also skips test/story files via extension: `*.test.tsx`, `*.spec.tsx`,
/// `*.stories.tsx`.
pub fn check(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    walk(root, root, &mut findings);
    findings
}

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
        // features/<feat>/web/cells/<file>.tsx
        // features/<feat>/mobile/cells/<file>.tsx
        ["features", _, platform, "cells", rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }

        // features/<feat>/web/views/<audience>/<file>.tsx
        // features/<feat>/mobile/views/<audience>/<file>.tsx
        ["features", _, platform, "views", _, rest @ ..]
            if is_platform(platform) && !rest.is_empty() =>
        {
            true
        }

        // app/shell/web/<...>.tsx
        // app/shell/mobile/<...>.tsx
        ["app", "shell", platform, rest @ ..] if is_platform(platform) && !rest.is_empty() => true,

        // app/theme/<...>.tsx
        // app/ui/<...>.tsx
        // app/lib/<...>.tsx
        ["app", subdir, rest @ ..]
            if matches!(*subdir, "theme" | "ui" | "lib") && !rest.is_empty() =>
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
        touch(tmp.path(), "features/slug/web/views/admin/list.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn canonical_feature_cell_does_not_fire() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "features/slug/web/cells/type_badge.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn canonical_app_ui_does_not_fire() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "app/ui/button.tsx");

        let findings = check(tmp.path());

        assert!(findings.is_empty());
    }

    #[test]
    fn orphan_src_components_fires() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "src/components/SlugTable.tsx");

        let findings = check(tmp.path());

        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].path,
            PathBuf::from("src/components/SlugTable.tsx")
        );
        assert_eq!(findings[0].message, Finding::message(&findings[0].path));
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
    fn test_files_are_skipped() {
        let tmp = tempfile::TempDir::new().unwrap();
        touch(tmp.path(), "features/slug/web/views/admin/list.test.tsx");

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
        touch(tmp.path(), "src/components/Zed.tsx");
        touch(tmp.path(), "src/components/Alpha.tsx");
        touch(tmp.path(), "app/components/Foo.tsx");

        let first = check(tmp.path());
        let second = check(tmp.path());

        assert_eq!(first, second);
        assert_eq!(
            first.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
            vec![
                PathBuf::from("app/components/Foo.tsx"),
                PathBuf::from("src/components/Alpha.tsx"),
                PathBuf::from("src/components/Zed.tsx"),
            ]
        );
    }
}
