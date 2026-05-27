//! `INTERNAL-ERROR-NAMING-001` — pub error types must end in `Error`.
//!
//! Scans every `crates/lazuli_*/src/*.rs` library file for a
//! `pub struct` or `pub enum` whose preceding attribute stack declares
//! a `thiserror::Error` derive (or any `Error` token inside a `derive`
//! list), and verifies the type's identifier ends in `Error`. Types
//! that implement the error trait but aren't named with the canonical
//! `Error` suffix surprise consumers — `Result<_, ParseFail>` reads
//! ambiguously, `Result<_, ParseError>` reads as "this is an error".
//!
//! ## Why
//!
//! Rust ecosystem convention (mirrored by `std::io::Error`,
//! `serde::de::Error`, `anyhow::Error`, `thiserror`'s own examples) is
//! that any concrete type whose role is "the failure case in a
//! `Result`" carries the `Error` suffix. Lazuli's CLI surface shows
//! these names directly in `lazuli doctor` JSON output and in agent
//! traces — uniformity is a UX feature, not a stylistic preference.
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## What's NOT flagged
//!
//! - Private types (`struct Foo`, `enum Foo`) — only `pub` items are
//!   API surface.
//! - Types whose derive list doesn't contain a bare `Error` token
//!   (e.g. `#[derive(Debug, Clone)]` with a manual `impl
//!   std::error::Error` further down the file — out of scope for the
//!   line-based heuristic).
//! - Test/example/bench files (`is_library_src = false`).
//! - Types already ending in `Error` (`ParseError`, `ApiError`, ...).
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::error_handling::error_naming_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("c:/Users/lucas/lazuli"));
//! let findings = check(&files);
//! for f in &findings {
//!     println!("{}:{} {}", f.path.display(), f.line, f.type_name);
//! }
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::error_non_exhaustive_001`] — same scope,
//!   different shape (SemVer-stability guard).
//! - [`crate::error_handling::error_variant_doc_001`] — variant-level
//!   companion (every error variant carries doc or `#[error("...")]`).

use std::path::{Path, PathBuf};

use crate::internal_hygiene::walker::RustSourceFile;

/// One pub error type whose identifier doesn't end in `Error`.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// 1-based line number of the `pub struct` / `pub enum` line.
    pub line: usize,
    /// The offending identifier (`ParseFail`, `ApiProblem`, ...).
    pub type_name: String,
    /// `"struct"` or `"enum"`.
    pub kind: &'static str,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "INTERNAL-ERROR-NAMING-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::error_handling::error_naming_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("crates/lazuli_x/src/lib.rs"),
    ///     line: 12,
    ///     type_name: "ParseFail".to_string(),
    ///     kind: "enum",
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains("ParseFail"));
    /// assert!(msg.contains(":12"));
    /// assert!(msg.contains("enum"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} pub {} `{}` derives `thiserror::Error` but its name \
             doesn't end in `Error`. Rename to `{}Error` (or similar) \
             so `Result<_, {}>` reads as a typed failure per Lazuli \
             error-handling discipline.",
            self.path.display(),
            self.line,
            self.kind,
            self.type_name,
            self.type_name,
            self.type_name,
        )
    }
}

/// Run the rule against a slice of pre-walked library files.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::error_handling::error_naming_001::check;
/// use lazuli_doctor::internal_hygiene::walker::RustSourceFile;
/// use std::path::PathBuf;
///
/// let src = "#[derive(Debug, thiserror::Error)]\npub enum ParseFail { Bad }\n";
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
/// assert_eq!(findings[0].type_name, "ParseFail");
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
    let mut pending_derive_error = false;
    for (idx, raw_line) in source.lines().enumerate() {
        let line = strip_line_comments(raw_line);
        let trimmed = line.trim_start();

        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            // Doc comments don't break the attribute streak.
            continue;
        }
        if trimmed.is_empty() {
            // Blank line between attribute and item is permitted by
            // rustfmt-default style; don't reset.
            continue;
        }

        if is_derive_with_error(trimmed) {
            pending_derive_error = true;
            continue;
        }
        // Any other attribute line keeps the streak alive (e.g.
        // `#[non_exhaustive]` between the derive and the item).
        if trimmed.starts_with("#[") {
            continue;
        }

        if pending_derive_error {
            if let Some((kind, name)) = parse_pub_struct_or_enum(trimmed) {
                if !name.ends_with("Error") {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        line: idx + 1,
                        type_name: name.to_owned(),
                        kind,
                    });
                }
                pending_derive_error = false;
            } else {
                // Streak broken without consuming a pub item — reset.
                pending_derive_error = false;
            }
        }
    }
}

/// Returns `true` when the line is a `#[derive(...)]` attribute whose
/// list contains a bare `Error` token (so `thiserror::Error` and the
/// bare `Error` re-export both match).
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

/// Parse a `pub struct Name` / `pub enum Name` line. Returns
/// `Some(("struct" | "enum", name))` on match.
fn parse_pub_struct_or_enum(line: &str) -> Option<(&'static str, &str)> {
    let after_pub = if let Some(rest) = line.strip_prefix("pub struct ") {
        Some(("struct", rest))
    } else if let Some(rest) = line.strip_prefix("pub enum ") {
        Some(("enum", rest))
    } else if let Some(rest) = line.strip_prefix("pub(crate) struct ") {
        Some(("struct", rest))
    } else if let Some(rest) = line.strip_prefix("pub(crate) enum ") {
        Some(("enum", rest))
    } else {
        None
    };
    let (kind, rest) = after_pub?;
    // Identifier ends at first non-ident char.
    let ident_end = rest
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    if ident_end == 0 {
        return None;
    }
    Some((kind, &rest[..ident_end]))
}

fn strip_line_comments(line: &str) -> String {
    if let Some(idx) = line.find("//") {
        // `///` is a doc comment, not a line comment — keep it.
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
    fn enum_without_error_suffix_fires() {
        let f = file("#[derive(Debug, thiserror::Error)]\npub enum ParseFail { Bad }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "ParseFail");
        assert_eq!(findings[0].kind, "enum");
    }

    #[test]
    fn struct_without_error_suffix_fires() {
        let f = file(
            "#[derive(Debug, thiserror::Error)]\npub struct ApiProblem { code: u16 }\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "ApiProblem");
        assert_eq!(findings[0].kind, "struct");
    }

    #[test]
    fn enum_with_error_suffix_is_silent() {
        let f = file("#[derive(Debug, thiserror::Error)]\npub enum ParseError { Bad }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn struct_with_error_suffix_is_silent() {
        let f = file(
            "#[derive(Debug, thiserror::Error)]\npub struct ConfigError { msg: String }\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn bare_error_derive_token_matches() {
        // `use thiserror::Error;` then `#[derive(Error)]` is the
        // idiomatic form. The bare `Error` token must also fire.
        let f = file("#[derive(Debug, Error)]\npub enum LoadFail {}\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "LoadFail");
    }

    #[test]
    fn derive_without_error_token_does_not_fire() {
        // No `Error` in the derive list → out of scope.
        let f = file("#[derive(Debug, Clone)]\npub enum Status { Open }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn private_type_does_not_fire() {
        let f = file("#[derive(Debug, thiserror::Error)]\nenum ParseFail { Bad }\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn non_exhaustive_between_derive_and_item_still_fires() {
        let f = file(
            "#[derive(Debug, thiserror::Error)]\n#[non_exhaustive]\npub enum ParseFail { Bad }\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "ParseFail");
    }

    #[test]
    fn blank_line_between_derive_and_item_still_fires() {
        let f = file("#[derive(Debug, thiserror::Error)]\n\npub enum ParseFail { Bad }\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn doc_comment_between_derive_and_item_still_fires() {
        let f = file(
            "#[derive(Debug, thiserror::Error)]\n/// The failure mode.\npub enum ParseFail { Bad }\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn non_library_files_are_skipped() {
        let mut f = file("#[derive(Debug, thiserror::Error)]\npub enum ParseFail {}\n");
        f.is_library_src = false;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn message_includes_name_and_path() {
        let f = file("#[derive(Debug, thiserror::Error)]\npub enum ParseFail {}\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("ParseFail"));
        assert!(msg.contains("lib.rs"));
        assert!(msg.contains(":2"));
        assert!(msg.contains("enum"));
    }

    #[test]
    fn pub_crate_visibility_also_fires() {
        let f = file(
            "#[derive(Debug, thiserror::Error)]\npub(crate) enum LoadFail { Bad }\n",
        );
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].type_name, "LoadFail");
    }

    #[test]
    fn unrelated_pub_item_after_derive_resets_streak() {
        // `#[derive(...Error)]` then a `fn` (not struct/enum) — no fire,
        // and the streak resets so a later naked enum is silent.
        let f = file(
            "#[derive(Debug, thiserror::Error)]\npub fn weird() {}\npub enum NotErr { A }\n",
        );
        assert!(check(&[f]).is_empty());
    }
}
