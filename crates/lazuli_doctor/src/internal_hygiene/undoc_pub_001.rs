//! `INTERNAL-UNDOC-PUB-001` — flag `pub` items missing rustdoc.
//!
//! Walks every `.rs` source line looking for `pub fn`, `pub struct`,
//! `pub enum`, `pub trait`, and `pub const` declarations. For each,
//! checks whether the immediately-preceding lines contain at least one
//! `///` doc comment. Items with `#[doc(hidden)]` are skipped.
//!
//! This is a line-based heuristic, not a syntax-tree walk. The
//! tradeoff: zero dep on `syn` (which would add ~30s to LSP cold start)
//! at the cost of some false positives on edge cases like macros that
//! emit `pub fn`. Edge cases are surfaced as a finding the author can
//! `severity_override` with reason.
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## Why line-based vs syn-based
//!
//! `lazuli_doctor` is imported by `lazuli_lsp` for live squiggles.
//! Adding `syn` to the LSP cold-start path was unacceptable — every
//! `.lzi` file open in the editor would pay the syn-parser cost.
//! Line-based heuristic is good enough for ~95% of cases and surfaces
//! the rest as findings the author tags with an override.
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::internal_hygiene::undoc_pub_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("c:/Users/lucas/lazuli"));
//! let findings = check(&files);
//! // At W3 landing: lazuli_ir.rs ~370 undocumented pub items.
//! for f in &findings {
//!     println!("{}:{} {}", f.path.display(), f.line, f.item);
//! }
//! ```
//!
//! ## See also
//!
//! - [`crate::internal_hygiene::no_example_001`] — the next-tier rule
//!   that fires when an item HAS a doc but lacks `## Examples`.

use std::path::PathBuf;

use crate::internal_hygiene::walker::RustSourceFile;

/// One `pub` item without a leading `///` doc comment.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// 1-based line number of the `pub` declaration.
    pub line: usize,
    /// The item identifier itself (e.g. `"pub fn frobnicate"`).
    pub item: String,
}

impl Finding {
    /// Stable diagnostic code emitted with every finding.
    pub const CODE: &'static str = "INTERNAL-UNDOC-PUB-001";

    /// Human-readable message naming the file, line, and offending
    /// `pub` item — pre-formatted with the canonical "add a `///`
    /// summary" remediation hint.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::internal_hygiene::undoc_pub_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("crates/lazuli_widget/src/lib.rs"),
    ///     line: 7,
    ///     item: "pub fn frobnicate".into(),
    /// };
    /// assert!(f.message().contains("frobnicate"));
    /// assert!(f.message().contains("///"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} `{}` is public but has no rustdoc. Add a `///` summary \
             (and `## Examples` for non-trivial fns per CLAUDE.md Rustdoc conventions).",
            self.path.display(),
            self.line,
            self.item.trim(),
        )
    }
}

/// Walk every library `.rs` file, emit findings for undocumented `pub` items.
/// Test/example/bench files are skipped — those are internal scaffolds, not
/// part of the public surface.
///
/// ## Examples
///
/// ```no_run
/// use lazuli_doctor::internal_hygiene::undoc_pub_001::check;
/// use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
/// use std::path::Path;
///
/// let files = walk_workspace_rust_sources(Path::new("c:/Users/lucas/lazuli"));
/// for f in check(&files) {
///     eprintln!("{}:{} {}", f.path.display(), f.line, f.item);
/// }
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

fn scan_file(path: &std::path::Path, source: &str, out: &mut Vec<Finding>) {
    let lines: Vec<&str> = source.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !is_pub_item_declaration(trimmed) {
            continue;
        }
        if has_doc_or_hidden_above(&lines, idx) {
            continue;
        }
        out.push(Finding {
            path: path.to_path_buf(),
            line: idx + 1,
            item: extract_item_signature(trimmed),
        });
    }
}

/// `true` if the trimmed line begins a `pub`-visible item declaration we
/// expect to be documented.
///
/// Skipped:
/// - `pub use ...` (re-exports; documented elsewhere)
/// - `pub mod ...` (module-level docs sit inside the module via `//!`)
/// - `pub(crate) ...` / `pub(super) ...` (not public surface)
/// - `pub static` / `pub type` (not in the W3 v0.1 scope — add later)
fn is_pub_item_declaration(line: &str) -> bool {
    // Reject pub(...) visibility modifiers and pub re-exports / mods.
    if line.starts_with("pub(") {
        return false;
    }
    if !line.starts_with("pub ") {
        return false;
    }
    let rest = line["pub ".len()..].trim_start();
    if rest.starts_with("use ") || rest.starts_with("mod ") {
        return false;
    }
    rest.starts_with("fn ")
        || rest.starts_with("struct ")
        || rest.starts_with("enum ")
        || rest.starts_with("trait ")
        || rest.starts_with("const ")
        || rest.starts_with("async fn ")
        || rest.starts_with("unsafe fn ")
}

/// Look upward from `idx` and return `true` if we hit a `///` doc line
/// or a `#[doc(hidden)]` attribute before any non-blank, non-attribute
/// line.
fn has_doc_or_hidden_above(lines: &[&str], idx: usize) -> bool {
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim_start();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("///") || t.starts_with("//!") {
            return true;
        }
        if t.starts_with("#[doc(hidden)]") {
            return true;
        }
        if t.starts_with("#[") {
            // Other attribute — keep scanning upward.
            continue;
        }
        // Hit a non-doc, non-attribute, non-blank line — no docstring.
        return false;
    }
    false
}

/// Extract a short identifier from the declaration line (everything up
/// to `{` or `(` or `<`, whichever comes first).
fn extract_item_signature(line: &str) -> String {
    let end = line
        .find(|c: char| c == '{' || c == '(' || c == '<' || c == '=' || c == ';')
        .unwrap_or(line.len());
    line[..end].trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    fn undocumented_pub_fn_fires() {
        let f = file("pub fn frobnicate() {}\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].item.contains("pub fn frobnicate"));
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn documented_pub_fn_silent() {
        let f = file("/// Does the thing.\npub fn frobnicate() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn doc_hidden_attribute_silences() {
        let f = file("#[doc(hidden)]\npub fn internal() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn pub_struct_enum_trait_all_caught() {
        let f = file("pub struct S;\npub enum E {}\npub trait T {}\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 3);
    }

    #[test]
    fn pub_use_and_pub_mod_skipped() {
        let f = file("pub use foo::bar;\npub mod inner;\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn pub_crate_visibility_skipped() {
        let f = file("pub(crate) fn internal() {}\npub(super) fn also_internal() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn doc_comment_with_intervening_attribute_still_silences() {
        let f = file(
            "/// Does the thing.\n#[inline]\n#[must_use]\npub fn frobnicate() -> u32 { 0 }\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn non_library_src_files_are_skipped() {
        let mut f = file("pub fn frobnicate() {}\n");
        f.is_library_src = false;
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn message_contains_path_line_and_item() {
        let f = file("pub fn frobnicate() {}\n");
        let finding = check(&[f]).into_iter().next().unwrap();
        let msg = finding.message();
        assert!(msg.contains("lib.rs"));
        assert!(msg.contains(":1"));
        assert!(msg.contains("pub fn frobnicate"));
    }
}
