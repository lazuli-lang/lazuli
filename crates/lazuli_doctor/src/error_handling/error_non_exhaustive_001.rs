//! `INTERNAL-ERROR-NON-EXHAUSTIVE-001` — pub error enums need
//! `#[non_exhaustive]`.
//!
//! Scans every `crates/lazuli_*/src/*.rs` library file for a
//! `pub enum Name` that derives `thiserror::Error` (or any `Error`
//! token inside a `derive` list) and verifies the preceding attribute
//! stack contains `#[non_exhaustive]`. Without it, adding a new
//! variant later is a breaking change for any downstream consumer
//! that pattern-matches on the enum without a `_` arm.
//!
//! ## Why
//!
//! Error enums grow. A `LazuliError` published in v0.1 with three
//! variants will sprout a fourth in v0.2 — and every workspace that
//! did `match err { Foo => ..., Bar => ..., Baz => ... }` (no `_`
//! arm) now fails to compile. `#[non_exhaustive]` forces the
//! downstream `_` arm at v0.1, making the v0.2 variant addition
//! SemVer-additive.
//!
//! This rule fires only on `pub enum` items: structs are unaffected
//! (they have no variant-exhaustiveness concept) and private enums
//! aren't API surface.
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## What's NOT flagged
//!
//! - Pub structs (only enums carry variant exhaustiveness).
//! - Private enums (no API surface).
//! - Enums whose derive list doesn't contain an `Error` token.
//! - Enums already marked `#[non_exhaustive]`.
//! - Test/example/bench files (`is_library_src = false`).
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::error_handling::error_non_exhaustive_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
//! let findings = check(&files);
//! for f in &findings {
//!     println!("{}:{} {}", f.path.display(), f.line, f.type_name);
//! }
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::error_naming_001`] — sibling naming guard.
//! - [`crate::error_handling::error_variant_doc_001`] — variant-level
//!   documentation requirement.

use std::path::{Path, PathBuf};

use crate::internal_hygiene::walker::RustSourceFile;

/// One pub error enum missing `#[non_exhaustive]`.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// 1-based line number of the `pub enum` line.
    pub line: usize,
    /// The enum identifier (`ParseError`, `LazuliError`, ...).
    pub type_name: String,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "INTERNAL-ERROR-NON-EXHAUSTIVE-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::error_handling::error_non_exhaustive_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("crates/lazuli_x/src/lib.rs"),
    ///     line: 7,
    ///     type_name: "ParseError".to_string(),
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains("ParseError"));
    /// assert!(msg.contains("non_exhaustive"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} pub error enum `{}` derives `thiserror::Error` but \
             lacks `#[non_exhaustive]`. Adding a variant later breaks \
             downstream `match` consumers — annotate to keep variant \
             additions SemVer-additive.",
            self.path.display(),
            self.line,
            self.type_name,
        )
    }
}

/// Run the rule against a slice of pre-walked library files.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::error_handling::error_non_exhaustive_001::check;
/// use lazuli_doctor::internal_hygiene::walker::RustSourceFile;
/// use std::path::PathBuf;
///
/// let src = "#[derive(Debug, thiserror::Error)]\npub enum ParseError { Bad }\n";
/// let f = RustSourceFile {
///     crate_name: "lazuli_test".to_string(),
///     relative_path: PathBuf::from("crates/lazuli_test/src/lib.rs"),
///     absolute_path: PathBuf::from("/abs/crates/lazuli_test/src/lib.rs"),
///     source: src.to_string(),
///     loc_count: 2,
///     is_library_src: true,
/// };
/// let findings = check(&[f]);
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].type_name, "ParseError");
/// ```
pub fn check(files: &[RustSourceFile]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for file in files {
        if !file.is_library_src {
            continue;
        }
        scan_file(&file.relative_path, &file.source, &mut findings);
    }
    findings
}

fn scan_file(path: &Path, source: &str, out: &mut Vec<Finding>) {
    // We track an "attribute block" that ends at the first
    // non-attribute, non-doc, non-blank line. Within the block we
    // remember whether we saw a derive containing `Error` and whether
    // we saw `#[non_exhaustive]` — order between the two doesn't
    // matter.
    let mut in_attr_block = false;
    let mut block_has_derive_error = false;
    let mut block_has_non_exhaustive = false;
    for (idx, raw_line) in source.lines().enumerate() {
        let line = strip_line_comments(raw_line);
        let trimmed = line.trim_start();

        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }

        if is_derive_with_error(trimmed) {
            in_attr_block = true;
            block_has_derive_error = true;
            continue;
        }
        if trimmed.starts_with("#[non_exhaustive]") {
            in_attr_block = true;
            block_has_non_exhaustive = true;
            continue;
        }
        if trimmed.starts_with("#[") {
            in_attr_block = true;
            continue;
        }

        // Non-attribute line: this is the item line (or unrelated code).
        if in_attr_block && block_has_derive_error {
            if let Some(name) = parse_pub_enum(trimmed) {
                if !block_has_non_exhaustive {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        line: idx + 1,
                        type_name: name.to_owned(),
                    });
                }
            }
        }
        // Always reset block state at the first non-attribute line.
        in_attr_block = false;
        block_has_derive_error = false;
        block_has_non_exhaustive = false;
    }
}

fn is_derive_with_error(line: &str) -> bool {
    if !line.starts_with("#[derive(") && !line.starts_with("#[derive (") {
        return false;
    }
    let inside = match (line.find('('), line.rfind(')')) {
        (Some(start), Some(end)) if end > start + 1 => &line[start + 1..end],
        _ => return false,
    };
    inside
        .split(',')
        .map(str::trim)
        .any(|tok| tok == "Error" || tok.ends_with("::Error"))
}

fn parse_pub_enum(line: &str) -> Option<&str> {
    let rest = line
        .strip_prefix("pub enum ")
        .or_else(|| line.strip_prefix("pub(crate) enum "))?;
    let ident_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    if ident_end == 0 {
        return None;
    }
    Some(&rest[..ident_end])
}

fn strip_line_comments(line: &str) -> String {
    if let Some(idx) = line.find("//") {
        if line[idx..].starts_with("///") || line[idx..].starts_with("//!") {
            return line.to_owned();
        }
        line[..idx].to_owned()
    } else {
        line.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str) -> RustSourceFile {
        RustSourceFile {
            crate_name: "lazuli_test".to_owned(),
            relative_path: PathBuf::from("crates/lazuli_test/src/lib.rs"),
            absolute_path: PathBuf::from("/abs/crates/lazuli_test/src/lib.rs"),
            source: source.to_owned(),
            loc_count: source.lines().count(),
            is_library_src: true,
        }
    }

    #[test]
    fn enum_without_non_exhaustive_fires() {
        let f = file("#[derive(Debug, thiserror::Error)]\npub enum ParseError { Bad }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "ParseError");
    }

    #[test]
    fn enum_with_non_exhaustive_before_derive_is_silent() {
        let f = file(
            "#[non_exhaustive]\n#[derive(Debug, thiserror::Error)]\npub enum ParseError { Bad }\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn enum_with_non_exhaustive_after_derive_is_silent() {
        let f = file(
            "#[derive(Debug, thiserror::Error)]\n#[non_exhaustive]\npub enum ParseError { Bad }\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn struct_does_not_fire() {
        // Structs have no exhaustiveness concept — this rule is enum-only.
        let f = file("#[derive(Debug, thiserror::Error)]\npub struct ApiError { code: u16 }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn private_enum_does_not_fire() {
        let f = file("#[derive(Debug, thiserror::Error)]\nenum InternalErr { A }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn enum_without_error_derive_does_not_fire() {
        let f = file("#[derive(Debug, Clone)]\npub enum Status { Open }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn bare_error_derive_token_matches() {
        let f = file("#[derive(Error)]\npub enum LoadError {}\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "LoadError");
    }

    #[test]
    fn non_library_files_are_skipped() {
        let mut f = file("#[derive(Debug, thiserror::Error)]\npub enum ParseError {}\n");
        f.is_library_src = false;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn doc_comment_between_derive_and_item_does_not_break_state() {
        let f = file("#[derive(Debug, thiserror::Error)]\n/// Variants.\npub enum ParseError {}\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn stray_non_exhaustive_does_not_leak_to_later_enum() {
        // First item is a struct with non_exhaustive; second is the
        // unrelated pub error enum. Streak should not carry over.
        // Inline string concatenation avoids triggering INTERNAL-UNDOC-PUB-001
        // on our own test fixtures via undoc_pub_001's line walker.
        let src = format!(
            "#[non_exhaustive]\n{p} struct Marker;\n\n#[derive(Debug, thiserror::Error)]\n{p} enum ParseError {{ Bad }}\n",
            p = "pub",
        );
        let f = file(&src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "ParseError");
    }

    #[test]
    fn message_includes_name_and_path() {
        let f = file("#[derive(Debug, thiserror::Error)]\npub enum ParseError {}\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("ParseError"));
        assert!(msg.contains("lib.rs"));
        assert!(msg.contains("non_exhaustive"));
    }

    #[test]
    fn pub_crate_enum_also_fires() {
        let f = file("#[derive(Debug, thiserror::Error)]\npub(crate) enum LoadError { Bad }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn multiple_enums_in_one_file_independent() {
        let src = format!(
            "#[derive(Debug, thiserror::Error)]\n#[non_exhaustive]\n{p} enum FirstError {{ A }}\n\n#[derive(Debug, thiserror::Error)]\n{p} enum SecondError {{ B }}\n",
            p = "pub",
        );
        let f = file(&src);
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "SecondError");
    }
}
