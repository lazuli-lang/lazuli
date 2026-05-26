//! Shared test helpers for the `type_duplicate` rule's sibling test files.
//!
//! Centralizes the ad-hoc `TempDir` and `write` fixtures so each
//! `tests_*.rs` file stays focused on its sub-concern (basic
//! redeclaration / Wave H plural+singular / Wave S2 import-block
//! awareness) instead of repeating the scaffolding.

#![cfg(test)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Test-only scratch directory used by the rule's sibling
/// `tests_*.rs` files. Removes itself on `Drop`.
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

/// Write `contents` to `root/rel`, creating any intermediate
/// directories. Used by fixtures to stand up tiny project trees.
///
/// ## Examples
///
/// ```ignore
/// // test-only helper
/// ```
pub fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_creates_parents() {
        let dir = TempDir::new().expect("tempdir");
        write(dir.path(), "nested/x.ts", "export {};");
        assert_eq!(
            fs::read_to_string(dir.path().join("nested/x.ts")).unwrap(),
            "export {};"
        );
    }
}
