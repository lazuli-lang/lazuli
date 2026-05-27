//! `INTERNAL-FILE-SIZE-001` — flag `.rs` files exceeding the size
//! thresholds.
//!
//! Mirrors clippy's `too-many-lines` lint but file-level, not
//! function-level. The Rails analog is rubocop's `Metrics/ClassLength`.
//! At W3 landing the framework had 5 files over 10k LOC; this rule's
//! job is to make further drift visible at PR time.
//!
//! ## Thresholds (default)
//!
//! - `> 2000 LOC` → `Warning`
//! - `> 5000 LOC` → `Error`
//!
//! Under `[doctor.internal_hygiene].preset = "tdd-iron-hand"` both
//! tiers escalate to `Error`. Configurable per-rule via
//! `severity_override` with mandatory `reason`.
//!
//! ## Why TWO thresholds
//!
//! The 5k tier is the hard "must split" bar — rust-analyzer slows
//! perceptibly above that and merge conflicts grow exponential. The
//! 2k tier is the soft "consider splitting" bar — common Rails-style
//! convention (ActiveRecord modules tend to live under 1k Ruby LOC).
//! Splitting incrementally is easier than waiting for the 5k cliff.
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::internal_hygiene::file_size_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
//! let findings = check(&files);
//! // At W3 landing: ~5 files > 10k LOC, all flagged at Error tier.
//! for f in &findings {
//!     println!("{} {} LOC (tier {:?})", f.path.display(), f.loc_count, f.tier);
//! }
//! ```
//!
//! ## See also
//!
//! - [`crate::internal_hygiene::walker`] — source of input.
//! - `docs/proposals/tdd-bdd-first-2026-05-23.md` §W3.

use std::path::PathBuf;

use crate::internal_hygiene::walker::RustSourceFile;

/// Soft threshold — files above this fire `Warning` by default.
pub const WARN_THRESHOLD: usize = 2_000;
/// Hard threshold — files above this fire `Error` by default.
pub const ERROR_THRESHOLD: usize = 5_000;

/// Which threshold tier a finding crossed. Used by the dispatcher to
/// pick severity at default (pre-preset) resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// File is between [`WARN_THRESHOLD`] and [`ERROR_THRESHOLD`].
    Warn,
    /// File is above [`ERROR_THRESHOLD`].
    Error,
}

/// One file flagged by the rule.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// Total line count.
    pub loc_count: usize,
    /// Which threshold the file crossed.
    pub tier: Tier,
}

impl Finding {
    /// Stable rule code surfaced in `DoctorDiagnostic.code` + JSON output.
    pub const CODE: &'static str = "INTERNAL-FILE-SIZE-001";

    /// Human-readable message for the diagnostic. Names the path,
    /// the actual LOC count, and which threshold was crossed so the
    /// author can decide between "split now" and "warn-only".
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::internal_hygiene::file_size_001::{Finding, Tier};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("crates/big/src/lib.rs"),
    ///     loc_count: 6000,
    ///     tier: Tier::Error,
    ///  };
    /// assert!(f.message().contains("6000"));
    /// assert!(f.message().contains("Split required"));
    /// ```
    pub fn message(&self) -> String {
        match self.tier {
            Tier::Warn => format!(
                "{} is {} LOC (> {} soft threshold). Consider splitting into submodules; \
                 see Rails-style modularization in CLAUDE.md.",
                self.path.display(),
                self.loc_count,
                WARN_THRESHOLD,
            ),
            Tier::Error => format!(
                "{} is {} LOC (> {} hard threshold). rust-analyzer slows perceptibly at this \
                 size and merge conflicts compound exponentially. Split required.",
                self.path.display(),
                self.loc_count,
                ERROR_THRESHOLD,
            ),
        }
    }
}

/// Walk every file, emit findings for those above [`WARN_THRESHOLD`].
///
/// Generated `.rs` files (under `dist/`, `target/`, or any file whose
/// first 200 chars contain `// @generated` / `// auto-generated`) are
/// skipped — they're not human-authored source.
///
/// ## Examples
///
/// ```no_run
/// use lazuli_doctor::internal_hygiene::file_size_001::check;
/// use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
/// use std::path::Path;
///
/// let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
/// for f in check(&files) {
///     eprintln!("{} {} LOC", f.path.display(), f.loc_count);
/// }
/// ```
pub fn check(files: &[RustSourceFile]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in files {
        if file.loc_count <= WARN_THRESHOLD {
            continue;
        }
        if is_generated(&file.source) {
            continue;
        }
        let tier = if file.loc_count > ERROR_THRESHOLD {
            Tier::Error
        } else {
            Tier::Warn
        };
        findings.push(Finding {
            path: file.relative_path.clone(),
            loc_count: file.loc_count,
            tier,
        });
    }
    findings
}

/// `true` if the file's preamble identifies it as generated. Conservative
/// detection — only the first ~200 bytes are scanned.
fn is_generated(source: &str) -> bool {
    let head = source.chars().take(200).collect::<String>().to_ascii_lowercase();
    head.contains("@generated") || head.contains("auto-generated")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(loc: usize, source: &str) -> RustSourceFile {
        RustSourceFile {
            crate_name: "lazuli_test".to_owned(),
            relative_path: PathBuf::from("crates/lazuli_test/src/lib.rs"),
            absolute_path: PathBuf::from("/abs/crates/lazuli_test/src/lib.rs"),
            source: source.to_owned(),
            loc_count: loc,
            is_library_src: true,
        }
    }

    #[test]
    fn under_warn_threshold_no_finding() {
        assert!(check(&[file(WARN_THRESHOLD, "fn x() {}")]).is_empty());
        assert!(check(&[file(500, "fn x() {}")]).is_empty());
    }

    #[test]
    fn over_warn_under_error_fires_warn_tier() {
        let findings = check(&[file(WARN_THRESHOLD + 1, "fn x() {}")]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tier, Tier::Warn);
    }

    #[test]
    fn over_error_threshold_fires_error_tier() {
        let findings = check(&[file(ERROR_THRESHOLD + 1, "fn x() {}")]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].tier, Tier::Error);
    }

    #[test]
    fn generated_files_are_skipped() {
        let mut f = file(ERROR_THRESHOLD + 1000, "fn x() {}");
        f.source = "// @generated by codegen\n".to_owned() + &f.source;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn auto_generated_marker_also_skipped() {
        let mut f = file(ERROR_THRESHOLD + 1000, "fn x() {}");
        f.source = "// auto-generated; do not edit\n".to_owned() + &f.source;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn message_includes_path_and_count() {
        let f = file(ERROR_THRESHOLD + 100, "fn x() {}");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("5100"));
        assert!(msg.contains("lib.rs"));
    }
}
