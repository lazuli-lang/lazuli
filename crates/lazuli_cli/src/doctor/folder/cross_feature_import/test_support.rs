//! Shared test helpers for the `cross_feature_import` rule.
//!
//! Centralizes the `TempDir` + `write_file` fixtures so the sibling
//! test files stay focused on their sub-concerns.

#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

/// Test-only scratch directory used by the rule's sibling test files.
/// Removes itself on `Drop` so the temp dir does not leak between
/// test runs.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Allocate a fresh unique temp directory.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // test-only helper; see the sibling rule tests for usage
    /// ```
    pub fn new() -> std::io::Result<Self> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "lazuli-cross-feature-import-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// Borrow the allocated directory.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // test-only helper; see the sibling rule tests for usage
    /// ```
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Write `contents` to `root/rel`, creating any intermediate directories.
/// Returns the absolute path to the new file.
pub fn write_file(root: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempdir_creates_and_removes_itself() {
        let path;
        {
            let dir = TempDir::new().expect("tempdir new");
            assert!(dir.path().exists());
            path = dir.path().to_owned();
        }
        assert!(!path.exists(), "tempdir should clean up on Drop");
    }

    #[test]
    fn write_file_creates_parent_dir() {
        let dir = TempDir::new().expect("tempdir new");
        let path = write_file(dir.path(), "nested/x.txt", "hello");
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello");
    }
}
