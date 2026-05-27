//! Shared filesystem walker for `INTERNAL-*` rules.
//!
//! Walks `<workspace_root>/crates/lazuli_*/src/` recursively, yielding
//! `(crate_name, relative_path, absolute_path, source_text)` tuples for
//! every `.rs` file. Std-only (`std::fs::read_dir`) — no `walkdir`
//! dependency, mirroring the existing
//! [`crate::correctness::schema_migration_present`] convention so
//! `lazuli_doctor` stays minimal-dep for the LSP's live-squiggle path.
//!
//! ## Skip rules
//!
//! - `target/` and `.git/` are pruned at entry.
//! - `tests/`, `examples/`, `benches/` are NOT pruned — those `.rs` files
//!   are real source the framework ships. They're tagged differently
//!   from `src/` so per-rule logic can opt to skip them (e.g.
//!   `INTERNAL-NO-EXAMPLE-001` only fires on `src/`).
//! - `mod.rs` files are walked normally; rules treat them per
//!   their own logic.
//!
//! ## Examples
//!
//! Walk the lazuli workspace, count files per crate:
//!
//! ```no_run
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let root = Path::new("/path/to/lazuli");
//! let files = walk_workspace_rust_sources(root);
//! for f in &files {
//!     println!("{:>20} {}", f.crate_name, f.relative_path.display());
//! }
//! ```
//!
//! ## See also
//!
//! - [`crate::internal_hygiene::file_size_001`] — first consumer
//!   (checks `loc_count` per file).
//! - [`crate::internal_hygiene::undoc_pub_001`] — second consumer
//!   (scans `source` for `pub` items + docstring presence).

use std::fs;
use std::path::{Path, PathBuf};

/// One `.rs` source file discovered by the walker. `source` is the
/// full UTF-8 contents (BOM stripped if present); `loc_count` is the
/// total line count (including blank lines and comments — file_size
/// rule uses this raw count, not a stripped one, to match what reads
/// in an editor).
#[derive(Debug, Clone)]
pub struct RustSourceFile {
    /// Crate the file belongs to (e.g. `"lazuli_doctor"`,
    /// `"lazuli_cli"`). Derived from the path segment after `crates/`.
    pub crate_name: String,
    /// Path relative to the workspace root, normalized with forward
    /// slashes (`crates/lazuli_doctor/src/internal_hygiene/walker.rs`).
    pub relative_path: PathBuf,
    /// Absolute path on disk. Used by rules that need to emit
    /// `DoctorDiagnostic.path`.
    pub absolute_path: PathBuf,
    /// Full file contents, UTF-8. BOM stripped.
    pub source: String,
    /// Line count of `source`. Cached at walk time so rules don't
    /// re-count.
    pub loc_count: usize,
    /// `true` when the file lives under `src/` (not `tests/`,
    /// `examples/`, `benches/`). Some rules apply only to library
    /// code (e.g. `INTERNAL-NO-EXAMPLE-001` skips test/example files).
    pub is_library_src: bool,
}

/// Walk `<workspace_root>/crates/lazuli_*/src/` (and sibling
/// `tests/`/`examples/`/`benches/` directories) recursively. Returns
/// every `.rs` file found, with metadata pre-computed.
///
/// Skips `target/`, `.git/`, `.worktrees/`, and any non-`lazuli_*`
/// directory under `crates/`. Returns an empty vec if `workspace_root`
/// does not contain a `crates/` directory (e.g. when run outside the
/// framework repo).
///
/// ## Examples
///
/// ```no_run
/// use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
/// use std::path::Path;
///
/// let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
/// // 333 files at the time of W3 landing.
/// assert!(files.len() > 100);
/// ```
pub fn walk_workspace_rust_sources(workspace_root: &Path) -> Vec<RustSourceFile> {
    let crates_root = workspace_root.join("crates");
    if !crates_root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let crate_dirs = match fs::read_dir(&crates_root) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    for entry in crate_dirs.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(crate_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !crate_name.starts_with("lazuli_") {
            continue;
        }
        for sub in ["src", "tests", "examples", "benches"] {
            let sub_path = path.join(sub);
            if sub_path.is_dir() {
                walk_dir_recursive(
                    workspace_root,
                    crate_name,
                    sub == "src",
                    &sub_path,
                    &mut out,
                );
            }
        }
    }
    out
}

fn walk_dir_recursive(
    workspace_root: &Path,
    crate_name: &str,
    is_library_src: bool,
    dir: &Path,
    out: &mut Vec<RustSourceFile>,
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
            // Skip artefact / metadata directories.
            if matches!(name, "target" | ".git" | ".worktrees" | "node_modules") {
                continue;
            }
            walk_dir_recursive(workspace_root, crate_name, is_library_src, &path, out);
        } else if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let source = match fs::read_to_string(&path) {
                Ok(s) => strip_bom(s),
                Err(_) => continue,
            };
            let loc_count = source.lines().count();
            let relative_path = path
                .strip_prefix(workspace_root)
                .unwrap_or(&path)
                .to_path_buf();
            out.push(RustSourceFile {
                crate_name: crate_name.to_owned(),
                relative_path,
                absolute_path: path,
                source,
                loc_count,
                is_library_src,
            });
        }
    }
}

/// Strip a UTF-8 BOM if present. Files generated by some Windows tools
/// carry one and it confuses downstream `pub` regex matching.
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

    /// Make a minimal mock workspace with one fake `lazuli_test` crate
    /// containing two `.rs` files: one in `src/`, one in `tests/`.
    fn make_mock_workspace(root: &Path) {
        let crate_src = root.join("crates/lazuli_test/src");
        let crate_tests = root.join("crates/lazuli_test/tests");
        fs::create_dir_all(&crate_src).unwrap();
        fs::create_dir_all(&crate_tests).unwrap();
        let mut f = fs::File::create(crate_src.join("lib.rs")).unwrap();
        let _ = writeln!(f, "//! A");
        let _ = writeln!(f, "pub fn x() {{}}");
        let mut t = fs::File::create(crate_tests.join("integration.rs")).unwrap();
        let _ = writeln!(t, "#[test]");
        let _ = writeln!(t, "fn it() {{}}");
    }

    #[test]
    fn walker_returns_empty_when_crates_dir_missing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(walk_workspace_rust_sources(tmp.path()).is_empty());
    }

    #[test]
    fn walker_finds_lib_and_test_rs_files() {
        let tmp = tempfile::tempdir().unwrap();
        make_mock_workspace(tmp.path());
        let files = walk_workspace_rust_sources(tmp.path());
        assert_eq!(files.len(), 2);

        let lib = files.iter().find(|f| f.is_library_src).unwrap();
        assert!(lib.relative_path.to_string_lossy().contains("lib.rs"));
        assert_eq!(lib.crate_name, "lazuli_test");
        assert!(lib.source.contains("pub fn x"));
        assert!(lib.loc_count >= 2);

        let test = files.iter().find(|f| !f.is_library_src).unwrap();
        assert!(test.relative_path.to_string_lossy().contains("integration.rs"));
    }

    #[test]
    fn walker_skips_non_lazuli_prefixed_crates() {
        let tmp = tempfile::tempdir().unwrap();
        let other = tmp.path().join("crates/some_other_crate/src");
        fs::create_dir_all(&other).unwrap();
        fs::File::create(other.join("lib.rs")).unwrap();
        assert!(walk_workspace_rust_sources(tmp.path()).is_empty());
    }

    #[test]
    fn walker_skips_target_and_git_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("crates/lazuli_test/src/target");
        fs::create_dir_all(&target).unwrap();
        fs::File::create(target.join("buried.rs")).unwrap();
        let src = tmp.path().join("crates/lazuli_test/src");
        fs::create_dir_all(&src).unwrap();
        fs::File::create(src.join("lib.rs")).unwrap();
        let files = walk_workspace_rust_sources(tmp.path());
        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.to_string_lossy().ends_with("lib.rs"));
    }
}
