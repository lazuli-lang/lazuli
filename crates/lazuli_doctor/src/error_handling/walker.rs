//! Walker for user-app Go handler sources.
//!
//! `HANDLER-*` rules need to scan `.go` files under
//! `features/<feature>/{handlers,domain,jobs,integrations}/`. This
//! walker yields one [`GoHandlerSourceFile`] per discovered `.go` file
//! with the full UTF-8 source and feature attribution.
//!
//! Std-only (`fs::read_dir`), same posture as
//! [`crate::internal_hygiene::walker`] — `lazuli_doctor` is imported by
//! the LSP and must stay minimal-dep.
//!
//! ## Scope
//!
//! Walks `<workspace_root>/features/*/{handlers,domain,jobs,integrations}/`
//! recursively. Skips `vendor/`, `dist/`, hidden dirs, and any non-`.go`
//! file. `_test.go` files are flagged (`is_test`) so per-rule logic can
//! opt to skip them — `HANDLER-NO-PANIC-001` allows panics inside tests
//! (`go test` semantics).
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::error_handling::walker::walk_workspace_go_handlers;
//! use std::path::Path;
//!
//! let root = Path::new("c:/Users/lucas/hostpoint");
//! let files = walk_workspace_go_handlers(root);
//! for f in &files {
//!     println!("{} ({}): {} LOC", f.relative_path.display(), f.feature_name, f.loc_count);
//! }
//! ```

use std::fs;
use std::path::{Path, PathBuf};

/// One `.go` source file under a feature's handler tree.
#[derive(Debug, Clone)]
pub struct GoHandlerSourceFile {
    /// Feature this file belongs to (e.g. `"properties"`, `"auth"`) —
    /// derived from the path segment immediately after `features/`.
    pub feature_name: String,
    /// Which sub-tree under the feature (`"handlers"`, `"domain"`,
    /// `"jobs"`, `"integrations"`) — useful when rules need to apply
    /// only to certain handler kinds (`HANDLER-RETRIABLE-001` only
    /// fires under `jobs/` and `integrations/`).
    pub bucket: String,
    /// Path relative to the workspace root.
    pub relative_path: PathBuf,
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Full file contents, UTF-8. BOM stripped.
    pub source: String,
    /// Line count of `source`.
    pub loc_count: usize,
    /// `true` for `*_test.go` files. Rules that allow panics-in-tests
    /// branch on this.
    pub is_test: bool,
}

/// Walk `<workspace_root>/features/*/{handlers,domain,jobs,integrations}/`
/// recursively. Returns every `.go` file with metadata.
///
/// Returns an empty vec if `workspace_root` does not contain a
/// `features/` directory.
///
/// ## Examples
///
/// ```no_run
/// use lazuli_doctor::error_handling::walker::walk_workspace_go_handlers;
/// use std::path::Path;
///
/// let files = walk_workspace_go_handlers(Path::new("c:/Users/lucas/hostpoint"));
/// assert!(files.iter().all(|f| f.relative_path.extension().unwrap() == "go"));
/// ```
pub fn walk_workspace_go_handlers(workspace_root: &Path) -> Vec<GoHandlerSourceFile> {
    let features_root = workspace_root.join("features");
    if !features_root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let feature_dirs = match fs::read_dir(&features_root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    for entry in feature_dirs.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(feature_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        for bucket in ["handlers", "domain", "jobs", "integrations"] {
            let bucket_path = path.join(bucket);
            if bucket_path.is_dir() {
                walk_dir_recursive(workspace_root, feature_name, bucket, &bucket_path, &mut out);
            }
        }
    }
    out
}

fn walk_dir_recursive(
    workspace_root: &Path,
    feature_name: &str,
    bucket: &str,
    dir: &Path,
    out: &mut Vec<GoHandlerSourceFile>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.is_dir() {
            if matches!(name, "vendor" | "dist" | "node_modules" | ".git") {
                continue;
            }
            walk_dir_recursive(workspace_root, feature_name, bucket, &path, out);
        } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("go") {
            let source = match fs::read_to_string(&path) {
                Ok(s) => strip_bom(s),
                Err(_) => continue,
            };
            let loc_count = source.lines().count();
            let relative_path = path
                .strip_prefix(workspace_root)
                .unwrap_or(&path)
                .to_path_buf();
            let is_test = name.ends_with("_test.go");
            out.push(GoHandlerSourceFile {
                feature_name: feature_name.to_owned(),
                bucket: bucket.to_owned(),
                relative_path,
                absolute_path: path,
                source,
                loc_count,
                is_test,
            });
        }
    }
}

fn strip_bom(s: String) -> String {
    if let Some(stripped) = s.strip_prefix('\u{feff}') {
        stripped.to_owned()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn make_mock_app(root: &Path) {
        for (feature, bucket, file, body) in [
            ("auth", "handlers", "login.go", "package handlers\n\nfunc Login() {}\n"),
            ("auth", "handlers", "login_test.go", "package handlers\n\nfunc TestLogin(t *T) {}\n"),
            ("billing", "domain", "invoice.go", "package domain\n\nfunc Invoice() {}\n"),
            ("billing", "jobs", "reconcile.go", "package jobs\n\nfunc Reconcile() {}\n"),
        ] {
            let dir = root.join(format!("features/{feature}/{bucket}"));
            fs::create_dir_all(&dir).unwrap();
            let mut f = fs::File::create(dir.join(file)).unwrap();
            f.write_all(body.as_bytes()).unwrap();
        }
    }

    #[test]
    fn walker_empty_when_no_features_dir() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(walk_workspace_go_handlers(tmp.path()).is_empty());
    }

    #[test]
    fn walker_yields_one_file_per_handler() {
        let tmp = tempfile::tempdir().unwrap();
        make_mock_app(tmp.path());
        let files = walk_workspace_go_handlers(tmp.path());
        assert_eq!(files.len(), 4);

        let login = files.iter().find(|f| f.relative_path.ends_with("login.go")).unwrap();
        assert_eq!(login.feature_name, "auth");
        assert_eq!(login.bucket, "handlers");
        assert!(!login.is_test);

        let test = files
            .iter()
            .find(|f| f.relative_path.ends_with("login_test.go"))
            .unwrap();
        assert!(test.is_test);

        let invoice = files
            .iter()
            .find(|f| f.relative_path.ends_with("invoice.go"))
            .unwrap();
        assert_eq!(invoice.bucket, "domain");
    }

    #[test]
    fn walker_skips_vendor_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let vendor = tmp.path().join("features/auth/handlers/vendor");
        fs::create_dir_all(&vendor).unwrap();
        fs::File::create(vendor.join("buried.go")).unwrap();
        let mut handlers = tmp.path().join("features/auth/handlers");
        handlers.push("real.go");
        fs::File::create(&handlers).unwrap();
        let files = walk_workspace_go_handlers(tmp.path());
        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.ends_with("real.go"));
    }
}
