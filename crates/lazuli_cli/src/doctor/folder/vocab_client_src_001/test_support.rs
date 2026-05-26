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

pub fn mkdir_p(path: &Path) {
    fs::create_dir_all(path).unwrap();
}

pub fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    File::create(path).unwrap();
}

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
