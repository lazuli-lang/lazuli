//! Doctor rule `cross-feature-direct-import`.
//!
//! Flags TypeScript `import` statements in `features/A/{web,mobile}/<...>.tsx`
//! that reference `features/B/{web,mobile}/<...>` for A != B. Cross-feature
//! visual dependencies must go via slot bindings (`@client.<slot>` declared
//! in `B.<target>.lzx`) so the audience scope and typed contract are
//! preserved.
//!
//! Allowed cross-feature paths:
//!   - `@/dist/ts-web/<feat>/<...>` — generated SDK is shared across features.
//!   - `@/app/<...>` — shared primitives (shell, theme, ui, lib).
//!
//! Forbidden:
//!   - `@/features/B/web/cells/<...>` from `features/A/`.
//!   - `@/features/B/web/views/<...>` from `features/A/`.

use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub source_file: PathBuf,
    pub target_feature: String,
    pub source_feature: String,
    pub import_specifier: String,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "cross-feature-direct-import";

    pub fn message(
        source_file: &Path,
        source_feature: &str,
        target_feature: &str,
        import_specifier: &str,
    ) -> String {
        format!(
            "`{}` (feature `{}`) imports directly from feature `{}` via \
             `{}`. Cross-feature visual dependencies must go via slot \
             bindings (`@client.<slot>` declared in \
             `{}.<target>.lzx`) or shared `app/ui/` primitives.",
            source_file.display(),
            source_feature,
            target_feature,
            import_specifier,
            target_feature,
        )
    }
}

/// Walk `root/features/`, scan each `.tsx`/`.ts` for import statements
/// that reference a sibling feature's frontend tree. Emit Finding per
/// violation.
pub fn check(root: &Path) -> Vec<Finding> {
    let features_root = root.join("features");
    let mut findings = Vec::new();

    for feature_dir in read_sorted_dirs(&features_root) {
        let Some(source_feature) = file_name_str(&feature_dir).map(str::to_owned) else {
            continue;
        };

        for target in ["web", "mobile"] {
            let frontend_root = feature_dir.join(target);
            walk_frontend_files(&frontend_root, &mut |source_file| {
                scan_file(root, source_file, &source_feature, &mut findings);
            });
        }
    }

    findings.sort_by(|a, b| {
        (&a.source_file, &a.target_feature, &a.import_specifier).cmp(&(
            &b.source_file,
            &b.target_feature,
            &b.import_specifier,
        ))
    });
    findings
}

// --- helpers ---

/// Extract the import specifier from a line like `import X from "Y";`
/// or `import { X } from "Y";` or `import type { X } from "Y";`.
/// Returns the string between the matched quotes, or None.
fn extract_import_specifier(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("import") {
        return None;
    }
    let from_idx = trimmed.find(" from ")?;
    let after_from = &trimmed[from_idx + " from ".len()..];
    let after_from = after_from.trim_start();
    let quote = after_from.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &after_from[1..];
    let end = rest.find(quote)?;
    Some(&rest[..end])
}

/// Parse a `@/features/<feat>/<target>/<rest>` specifier into (feat,
/// target). Returns None if the specifier doesn't match the shape.
/// `target` is "web" or "mobile"; anything else returns None.
fn parse_feature_import(spec: &str) -> Option<(&str, &str)> {
    let rest = spec.strip_prefix("@/features/")?;
    parse_frontend_feature_path(rest)
}

fn scan_file(root: &Path, source_file: &Path, source_feature: &str, findings: &mut Vec<Finding>) {
    let Ok(contents) = fs::read_to_string(source_file) else {
        return;
    };

    for line in contents.lines() {
        let Some(import_specifier) = extract_import_specifier(line) else {
            continue;
        };
        let Some((target_feature, _target)) =
            parse_import_target(root, source_file, import_specifier)
        else {
            continue;
        };
        if target_feature == source_feature {
            continue;
        }

        findings.push(Finding {
            source_file: source_file.to_path_buf(),
            target_feature: target_feature.clone(),
            source_feature: source_feature.to_owned(),
            import_specifier: import_specifier.to_owned(),
            message: Finding::message(
                source_file,
                source_feature,
                &target_feature,
                import_specifier,
            ),
        });
    }
}

fn parse_import_target(
    root: &Path,
    source_file: &Path,
    import_specifier: &str,
) -> Option<(String, String)> {
    if let Some(parsed) = parse_feature_import(import_specifier) {
        return Some((parsed.0.to_owned(), parsed.1.to_owned()));
    }

    if !import_specifier.starts_with('.') {
        return None;
    }

    if let Some((feature, target)) = parse_relative_feature_import(import_specifier) {
        return Some((feature.to_owned(), target.to_owned()));
    }

    let normalized = normalize_path(source_file.parent()?.join(import_specifier));
    if let Some(parsed) = parse_normalized_feature_path(root, &normalized) {
        return Some(parsed);
    }

    None
}

fn parse_normalized_feature_path(root: &Path, path: &Path) -> Option<(String, String)> {
    let features_root = normalize_path(root.join("features"));
    let rel = path.strip_prefix(features_root).ok()?;
    let mut parts = rel.components();
    let feat = component_str(parts.next()?)?;
    let target = component_str(parts.next()?)?;
    if target != "web" && target != "mobile" {
        return None;
    }
    Some((feat.to_owned(), target.to_owned()))
}

fn parse_relative_feature_import(spec: &str) -> Option<(&str, &str)> {
    let mut rest = spec;
    loop {
        if let Some(next) = rest.strip_prefix("../") {
            rest = next;
        } else if let Some(next) = rest.strip_prefix("./") {
            rest = next;
        } else {
            break;
        }
    }
    parse_frontend_feature_path(rest)
}

fn parse_frontend_feature_path(path: &str) -> Option<(&str, &str)> {
    let mut parts = path.splitn(3, '/');
    let feat = parts.next()?;
    let target = parts.next()?;
    if feat.is_empty() || (target != "web" && target != "mobile") {
        return None;
    }
    Some((feat, target))
}

fn walk_frontend_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
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
            if file_name_str(&path).is_some_and(should_skip_dir) {
                continue;
            }
            walk_frontend_files(&path, visit);
        } else if file_type.is_file() && is_ts_or_tsx(&path) {
            visit(&path);
        }
    }
}

fn read_sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            file_type.is_dir().then(|| entry.path())
        })
        .collect();
    dirs.sort_by_key(|path| file_name_str(path).map(str::to_owned));
    dirs
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

fn component_str(component: Component<'_>) -> Option<&str> {
    match component {
        Component::Normal(value) => value.to_str(),
        _ => None,
    }
}

fn is_ts_or_tsx(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("ts" | "tsx")
    )
}

fn file_name_str(path: &Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | "dist" | "target" | ".git" | ".lazuli" | ".next" | ".expo"
    )
}

#[cfg(test)]
mod tests {
    use self::tempfile::TempDir;
    use super::*;
    use std::fs;

    mod tempfile {
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicUsize, Ordering};

        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            pub fn new() -> std::io::Result<Self> {
                let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "lazuli-cross-feature-import-test-{}-{id}",
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

    fn write_file(root: &Path, rel: &str, contents: &str) -> PathBuf {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn same_feature_internal_import_does_not_fire() {
        let temp = TempDir::new().unwrap();
        write_file(
            temp.path(),
            "features/slug/web/views/admin/list.tsx",
            "import Badge from \"@/features/slug/web/cells/type_badge\";\n",
        );

        assert!(check(temp.path()).is_empty());
    }

    #[test]
    fn cross_feature_cell_import_fires() {
        let temp = TempDir::new().unwrap();
        let source = write_file(
            temp.path(),
            "features/slug/web/views/admin/list.tsx",
            "import Avatar from \"@/features/account/web/cells/avatar\";\n",
        );

        let findings = check(temp.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].source_file, source);
        assert_eq!(findings[0].source_feature, "slug");
        assert_eq!(findings[0].target_feature, "account");
        assert_eq!(
            findings[0].import_specifier,
            "@/features/account/web/cells/avatar"
        );
        assert!(findings[0].message.contains("slot bindings"));
        assert_eq!(Finding::CODE, "cross-feature-direct-import");
    }

    #[test]
    fn cross_feature_view_import_fires() {
        let temp = TempDir::new().unwrap();
        write_file(
            temp.path(),
            "features/slug/web/views/admin/list.tsx",
            "import Login from \"@/features/account/web/views/admin/login\";\n",
        );

        let findings = check(temp.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].target_feature, "account");
    }

    #[test]
    fn import_from_dist_does_not_fire() {
        let temp = TempDir::new().unwrap();
        write_file(
            temp.path(),
            "features/slug/web/views/admin/list.tsx",
            "import { Account } from \"@/dist/ts-web/account/account.gen\";\n",
        );

        assert!(check(temp.path()).is_empty());
    }

    #[test]
    fn import_from_app_ui_does_not_fire() {
        let temp = TempDir::new().unwrap();
        write_file(
            temp.path(),
            "features/slug/web/views/admin/list.tsx",
            "import { Button } from \"@/app/ui/button\";\n",
        );

        assert!(check(temp.path()).is_empty());
    }

    #[test]
    fn relative_cross_feature_fires() {
        let temp = TempDir::new().unwrap();
        write_file(
            temp.path(),
            "features/slug/web/views/admin/list.tsx",
            "import Avatar from \"../../account/web/cells/avatar\";\n",
        );

        let findings = check(temp.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].source_feature, "slug");
        assert_eq!(findings[0].target_feature, "account");
        assert_eq!(
            findings[0].import_specifier,
            "../../account/web/cells/avatar"
        );
    }

    #[test]
    fn import_type_form_works() {
        let temp = TempDir::new().unwrap();
        write_file(
            temp.path(),
            "features/slug/web/views/admin/list.tsx",
            "import type { X } from '@/features/account/web/cells/x';\n",
        );

        let findings = check(temp.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].target_feature, "account");
    }

    #[test]
    fn commented_import_does_not_fire() {
        let temp = TempDir::new().unwrap();
        write_file(
            temp.path(),
            "features/slug/web/views/admin/list.tsx",
            "// import { X } from \"@/features/account/web/cells/x\";\n",
        );

        assert!(check(temp.path()).is_empty());
    }

    #[test]
    fn deterministic_order() {
        let temp = TempDir::new().unwrap();
        write_file(
            temp.path(),
            "features/slug/web/views/admin/z.tsx",
            "import Z from \"@/features/account/web/cells/z\";\n",
        );
        write_file(
            temp.path(),
            "features/slug/web/views/admin/a.tsx",
            "import A from \"@/features/billing/web/cells/a\";\n",
        );

        let first = check(temp.path());
        let second = check(temp.path());
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
        assert!(first[0].source_file.ends_with("a.tsx"));
        assert!(first[1].source_file.ends_with("z.tsx"));
    }

    #[test]
    fn extract_import_specifier_handles_named_import() {
        assert_eq!(
            extract_import_specifier("import { X } from \"@/features/a/web/cells/x\";"),
            Some("@/features/a/web/cells/x")
        );
    }

    #[test]
    fn extract_import_specifier_handles_default_import_with_single_quotes() {
        assert_eq!(
            extract_import_specifier("import X from '@/features/a/web/cells/x';"),
            Some("@/features/a/web/cells/x")
        );
    }

    #[test]
    fn extract_import_specifier_ignores_commented_import() {
        assert_eq!(
            extract_import_specifier("// import { X } from \"@/features/a/web/cells/x\";"),
            None
        );
    }

    #[test]
    fn extract_import_specifier_handles_type_import() {
        assert_eq!(
            extract_import_specifier("import type { X } from \"@/features/a/web/cells/x\";"),
            Some("@/features/a/web/cells/x")
        );
    }

    #[test]
    fn parse_feature_import_handles_web_target() {
        assert_eq!(
            parse_feature_import("@/features/account/web/cells/avatar"),
            Some(("account", "web"))
        );
    }

    #[test]
    fn parse_feature_import_handles_mobile_target() {
        assert_eq!(
            parse_feature_import("@/features/account/mobile/views/admin/login"),
            Some(("account", "mobile"))
        );
    }

    #[test]
    fn parse_feature_import_ignores_non_frontend_target() {
        assert_eq!(
            parse_feature_import("@/features/account/handlers/verify_password"),
            None
        );
    }
}
