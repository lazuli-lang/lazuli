//! Shared test helpers for the `feature_orphan` rule's sibling test files.
//!
//! Centralizes the ad-hoc `TempDir` and `touch` fixtures so each
//! `tests_*.rs` file stays focused on its sub-concern (basic canon /
//! Wave H 7+6 catalog) instead of repeating the scaffolding.

#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Test-only scratch directory that removes itself on `Drop`. Used by
/// the rule's sibling `tests_*.rs` files to lay out fixture trees
/// without leaking artifacts.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Allocate a fresh unique temp directory under the system temp.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// // test-only helper; see the sibling rule tests for usage
    /// ```
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

/// Create an empty file at `root/rel`, ensuring intermediate parents
/// exist. Used by the rule's fixtures to stand up tiny project trees.
///
/// ## Examples
///
/// ```ignore
/// // test-only helper
/// ```
pub fn touch(root: &Path, rel: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::File::create(path).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_creates_parent_and_file() {
        let dir = TempDir::new().expect("tempdir new");
        touch(dir.path(), "nested/x.ts");
        assert!(dir.path().join("nested/x.ts").exists());
    }
}
