//! Doctor rule `type-duplicate`.
//!
//! Flags user-authored `.ts(x)` files that declare an `interface X {}`
//! or `type X = ...` whose name `X` is already exported by a generated
//! `dist/ts-web/<feature>/<feature>.gen.ts` or
//! `dist/ts-mobile/<feature>/<feature>.gen.ts` module. Duplicating the
//! generated type is drift-prone: the user copy does not update when
//! `lazuli generate ts` regenerates.
//!
//! Detection is intentionally regex-free: generated and user-authored files
//! are scanned line-by-line for leading `interface` / `type` declarations.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Finding {
    pub user_file: PathBuf,
    pub type_name: String,
    pub generated_origin: PathBuf,
    pub message: String,
}

impl Finding {
    pub const CODE: &'static str = "type-duplicate";

    pub fn message(type_name: &str, user_file: &Path, generated_origin: &Path) -> String {
        format!(
            "`{}` redeclares type `{}` which is already exported from `{}`. \
             Import the generated type instead of duplicating; the user copy \
             will drift on next `lazuli generate ts`.",
            user_file.display(),
            type_name,
            generated_origin.display(),
        )
    }
}

/// Walk `root`, collect generated type names from `dist/ts-web/` and
/// `dist/ts-mobile/`, scan user code for redeclarations, emit Findings.
pub fn check(root: &Path) -> Vec<Finding> {
    let generated = collect_generated_types(root);
    if generated.is_empty() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for user_file in collect_user_ts_files(root) {
        let Ok(source) = fs::read_to_string(&user_file) else {
            continue;
        };

        for type_name in extract_declared_type_names(&source) {
            if type_name.len() < 2 {
                continue;
            }
            let Some(generated_origin) = generated.get(&type_name) else {
                continue;
            };

            findings.push(Finding {
                message: Finding::message(&type_name, &user_file, generated_origin),
                user_file: user_file.clone(),
                type_name,
                generated_origin: generated_origin.clone(),
            });
        }
    }

    findings.sort_by(|a, b| {
        a.user_file
            .cmp(&b.user_file)
            .then_with(|| a.type_name.cmp(&b.type_name))
    });
    findings
}

// --- private helpers ---

fn collect_generated_types(root: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    for target in ["ts-mobile", "ts-web"] {
        let base = root.join("dist").join(target);
        let mut files = Vec::new();
        collect_generated_files(&base, &mut files);
        files.sort();

        for path in files {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };

            for name in extract_declared_type_names(&source) {
                if name.len() >= 2 {
                    out.entry(name).or_insert_with(|| path.clone());
                }
            }
        }
    }
    out
}

fn collect_generated_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = sorted_entries(dir) else {
        return;
    };

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_generated_files(&path, out);
        } else if is_feature_gen_file(&path) {
            out.push(path);
        }
    }
}

fn is_feature_gen_file(path: &Path) -> bool {
    if path.file_name().and_then(|n| n.to_str()) == Some("routes.gen.ts") {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(feature_dir) = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    else {
        return false;
    };

    file_name == format!("{feature_dir}.gen.ts")
}

fn collect_user_ts_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_user_ts_files_under(&root.join("features"), true, &mut out);
    collect_user_ts_files_under(&root.join("app"), false, &mut out);
    out.sort();
    out
}

fn collect_user_ts_files_under(dir: &Path, require_platform_dir: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = sorted_entries(dir) else {
        return;
    };

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            if should_skip_dir(entry.file_name().to_str().unwrap_or("")) {
                continue;
            }
            collect_user_ts_files_under(&path, require_platform_dir, out);
        } else if is_ts_source(&path)
            && (!require_platform_dir || has_web_or_mobile_component(&path))
        {
            out.push(path);
        }
    }
}

fn sorted_entries(dir: &Path) -> std::io::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}

fn is_ts_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts") | Some("tsx")
    )
}

fn has_web_or_mobile_component(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str().to_str().unwrap_or("");
        value == "web" || value == "mobile"
    })
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "dist" | "node_modules" | "target" | ".git" | ".lazuli" | ".next" | ".expo"
    )
}

/// Extract exported type/interface names from a TS source string.
/// Handles `export interface X`, `export type X =`, `interface X`,
/// `type X =`. Stops at the identifier; does not parse the body.
fn extract_declared_type_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let after_export = trimmed.strip_prefix("export ").unwrap_or(trimmed);
        for prefix in &["interface ", "type "] {
            if let Some(rest) = after_export.strip_prefix(prefix) {
                if let Some(name) = first_identifier(rest) {
                    if !name.is_empty() {
                        out.push(name.to_owned());
                    }
                }
            }
        }
    }
    out
}

/// Walk leading characters of `s` while they match `[A-Za-z_][A-Za-z0-9_]*`.
fn first_identifier(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0];
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    let mut end = 1;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_alphanumeric() || b == b'_' {
            end += 1;
        } else {
            break;
        }
    }
    Some(&s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::time::{SystemTime, UNIX_EPOCH};

    mod tempfile {
        use super::*;

        pub struct TempDir {
            path: PathBuf,
        }

        impl TempDir {
            pub fn new() -> io::Result<Self> {
                let mut path = std::env::temp_dir();
                let unique = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();
                path.push(format!(
                    "lazuli-type-duplicate-{}-{unique}",
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

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn redeclared_interface_fires() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "features/foo/web/views/admin/list.tsx",
            "interface Slug { name: string }\n",
        );

        let findings = check(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "Slug");
        assert!(findings[0].message.contains("redeclares type `Slug`"));
    }

    #[test]
    fn redeclared_type_alias_fires() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export type Slug = { id: string }\n",
        );
        write(
            dir.path(),
            "app/lib/types.ts",
            "export type Slug = { name: string }\n",
        );

        let findings = check(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].generated_origin,
            dir.path().join("dist/ts-web/slug/slug.gen.ts")
        );
    }

    #[test]
    fn unique_user_type_does_not_fire() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "app/lib/types.ts",
            "type MyLocal = { id: string }\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    #[test]
    fn import_of_generated_does_not_fire() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "features/foo/web/views/admin/list.tsx",
            "import type { Slug } from \"@/dist/ts-web/slug/slug.gen\";\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    #[test]
    fn dist_is_not_scanned_as_user_source() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    #[test]
    fn nested_features_scanned() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-mobile/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "features/foo/web/cells/bar.tsx",
            "export interface Slug { name: string }\n",
        );

        let findings = check(dir.path());
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].user_file,
            dir.path().join("features/foo/web/cells/bar.tsx")
        );
    }

    #[test]
    fn node_modules_not_scanned() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\n",
        );
        write(
            dir.path(),
            "node_modules/x/y.d.ts",
            "export interface Slug { name: string }\n",
        );

        assert!(check(dir.path()).is_empty());
    }

    #[test]
    fn deterministic_order() {
        let dir = tempfile::TempDir::new().unwrap();
        write(
            dir.path(),
            "dist/ts-web/slug/slug.gen.ts",
            "export interface Slug { id: string }\nexport type Widget = { id: string }\n",
        );
        write(
            dir.path(),
            "app/lib/z.ts",
            "type Widget = {}\ninterface Slug {}\n",
        );
        write(
            dir.path(),
            "features/foo/web/cells/a.tsx",
            "interface Slug {}\n",
        );

        let first = check(dir.path())
            .into_iter()
            .map(|f| (f.user_file, f.type_name))
            .collect::<Vec<_>>();
        let second = check(dir.path())
            .into_iter()
            .map(|f| (f.user_file, f.type_name))
            .collect::<Vec<_>>();
        assert_eq!(first, second);
    }

    #[test]
    fn extract_export_interface() {
        assert_eq!(
            extract_declared_type_names("export interface Slug { id: string }"),
            vec!["Slug"]
        );
    }

    #[test]
    fn extract_type_alias() {
        assert_eq!(
            extract_declared_type_names("type Slug = { id: string }"),
            vec!["Slug"]
        );
    }

    #[test]
    fn extract_ignores_mixed_line_not_matching() {
        assert!(extract_declared_type_names("const x: Slug = value").is_empty());
    }
}
