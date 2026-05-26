//! Shared test helpers for the `vocab_client_src_001` rule's sibling
//! test files.
//!
//! Centralizes the `tempfile::TempDir` setup + `mkdir_p` / `touch` /
//! `names` helpers used across both `tests_canon.rs` (canonical-shape
//! coverage) and `tests_anti_patterns.rs` (anti-pattern + edge-case
//! coverage).

#![cfg(test)]

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::Path;

use super::Finding;

/// Create `path` and every intermediate parent directory. Panics on
/// I/O error to keep test fixtures terse.
///
/// ## Examples
///
/// ```ignore
/// // test-only helper; see sibling rule tests for usage
/// ```
pub fn mkdir_p(path: &Path) {
    fs::create_dir_all(path).unwrap();
}

/// Ensure `path` exists as an empty file, creating intermediate
/// directories as needed.
///
/// ## Examples
///
/// ```ignore
/// // test-only helper; see sibling rule tests for usage
/// ```
pub fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    File::create(path).unwrap();
}

/// Project a list of findings into the set of their offending
/// directory names. Tests use this for order-independent comparisons.
///
/// ## Examples
///
/// ```ignore
/// // test-only helper; see sibling rule tests for usage
/// ```
pub fn names(findings: &[Finding]) -> BTreeSet<String> {
    findings
        .iter()
        .map(|f| {
            f.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn names_collects_basenames() {
        let dir = env::temp_dir().join("lazuli-vocab-names-test");
        let _ = fs::remove_dir_all(&dir);
        mkdir_p(&dir);
        let findings = vec![Finding {
            path: dir.join("foo"),
            message: "x".into(),
        }];
        let bag = names(&findings);
        assert!(bag.contains("foo"));
        let _ = fs::remove_dir_all(&dir);
    }
}
