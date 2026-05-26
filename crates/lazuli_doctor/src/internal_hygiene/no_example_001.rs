//! `INTERNAL-NO-EXAMPLE-001` — flag `pub fn` whose rustdoc lacks
//! a `## Examples` section.
//!
//! Once an item HAS a docstring (so [`crate::internal_hygiene::undoc_pub_001`]
//! is silent on it), this rule checks whether the docstring includes a
//! `## Examples` heading or a `` ```rust ``-tagged code block. The
//! Rails analog: rdoc files typically lead with prose THEN an `==
//! Examples` section.
//!
//! Default severity: `Info`. Under `tdd-iron-hand` preset: `Error`.
//! The default is intentionally soft because W5 is a sweep wave —
//! flooding the developer with errors during the sweep would block
//! Hostpoint work. After W5 sweep completes per-crate, the temporary
//! `severity_override` is removed and the preset takes the rule to
//! `Error` (cf. W6 in the plan).
//!
//! ## What counts as an example
//!
//! Either:
//! - A `## Examples` heading anywhere in the docstring
//! - A `` ```rust ``, `` ```no_run ``, `` ```compile_fail ``, or
//!   `` ```ignore `` fenced code block
//!
//! Bare `` ``` `` blocks (no language tag) are NOT counted — they
//! won't compile via `cargo test --doc` and don't satisfy the Rails
//! "executable example" intent.
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::internal_hygiene::no_example_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("c:/Users/lucas/lazuli"));
//! let findings = check(&files);
//! // After W5 sweep on lazuli_ir: this count drops to ~0 for that crate.
//! ```
//!
//! ## Scope
//!
//! Only `pub fn` is checked. `pub struct` / `pub enum` / `pub trait`
//! often have meaningful `## Examples` at the module level (`//!`) and
//! requiring per-type examples is over-prescription. W6+ may extend
//! this to `pub trait`.

use std::path::PathBuf;

use crate::internal_hygiene::walker::RustSourceFile;

/// One `pub fn` whose rustdoc lacks a runnable example.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Library `.rs` file that contains the offending `pub fn`.
    pub path: PathBuf,
    /// 1-based line number of the `pub fn` declaration.
    pub line: usize,
    /// Signature snippet (everything up to `{` / `(` / `<` / `;`) so the
    /// message can name the function without ambiguity.
    pub fn_signature: String,
}

impl Finding {
    /// Stable rule code surfaced in `DoctorDiagnostic.code` + JSON output.
    pub const CODE: &'static str = "INTERNAL-NO-EXAMPLE-001";

    /// Human-readable message naming the file, line, and function whose
    /// rustdoc is missing a runnable example.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::internal_hygiene::no_example_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("crates/lazuli_widget/src/lib.rs"),
    ///     line: 42,
    ///     fn_signature: "pub fn frobnicate".into(),
    /// };
    /// assert!(f.message().contains("## Examples"));
    /// assert!(f.message().contains("frobnicate"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} `{}` has rustdoc but no `## Examples` or `` ```rust `` block. \
             Add a compilable example per CLAUDE.md Rustdoc conventions \
             (validated by `cargo test --doc`).",
            self.path.display(),
            self.line,
            self.fn_signature.trim(),
        )
    }
}

/// Walk every library `.rs` file, emit findings for documented `pub fn`s
/// whose docstring lacks a `## Examples` heading or a language-tagged
/// fenced block. Files where `is_library_src` is `false` (tests, benches)
/// are skipped — they're internal scaffolds, not part of the surface.
///
/// ## Examples
///
/// ```no_run
/// use lazuli_doctor::internal_hygiene::no_example_001::check;
/// use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
/// use std::path::Path;
///
/// let files = walk_workspace_rust_sources(Path::new("c:/Users/lucas/lazuli"));
/// let findings = check(&files);
/// // After W5 sweep on lazuli_ir: ~0 findings remaining for that crate.
/// eprintln!("undocumented examples: {}", findings.len());
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
        if !is_pub_fn(trimmed) {
            continue;
        }
        let doc_block = collect_doc_block_above(&lines, idx);
        if doc_block.is_empty() {
            // Caught by undoc_pub_001 — skip here to avoid double-firing.
            continue;
        }
        if doc_block_has_example(&doc_block) {
            continue;
        }
        out.push(Finding {
            path: path.to_path_buf(),
            line: idx + 1,
            fn_signature: extract_signature(trimmed),
        });
    }
}

fn is_pub_fn(line: &str) -> bool {
    if line.starts_with("pub(") {
        return false;
    }
    let rest = match line.strip_prefix("pub ") {
        Some(r) => r.trim_start(),
        None => return false,
    };
    rest.starts_with("fn ") || rest.starts_with("async fn ") || rest.starts_with("unsafe fn ")
}

fn collect_doc_block_above(lines: &[&str], idx: usize) -> Vec<String> {
    let mut doc = Vec::new();
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim_start();
        if t.is_empty() {
            // Blank lines inside an attribute-stack are allowed when
            // separating attrs from doc; keep scanning.
            continue;
        }
        if let Some(rest) = t.strip_prefix("/// ") {
            doc.push(rest.to_owned());
            continue;
        }
        if let Some(rest) = t.strip_prefix("///") {
            doc.push(rest.to_owned());
            continue;
        }
        if t.starts_with("#[") {
            // Intervening attribute — keep scanning upward.
            continue;
        }
        break;
    }
    doc.reverse();
    doc
}

fn doc_block_has_example(doc_lines: &[String]) -> bool {
    let mut in_fenced_block = false;
    for line in doc_lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("## Examples") || trimmed.starts_with("# Examples") {
            return true;
        }
        if let Some(rest) = trimmed.strip_prefix("```") {
            // Detect the language tag right after the fence opener.
            if !in_fenced_block {
                let lang = rest.trim();
                if matches!(lang, "rust" | "no_run" | "compile_fail" | "ignore" | "should_panic")
                    || lang.starts_with("rust,")
                    || lang.starts_with("no_run,")
                {
                    return true;
                }
            }
            in_fenced_block = !in_fenced_block;
        }
    }
    false
}

fn extract_signature(line: &str) -> String {
    let end = line
        .find(|c: char| c == '{' || c == '(' || c == '<' || c == ';')
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
    fn doc_without_examples_fires() {
        let f = file("/// Does the thing.\npub fn x() {}\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn doc_with_examples_heading_silent() {
        let f = file(
            "/// Does the thing.\n\
             ///\n\
             /// ## Examples\n\
             ///\n\
             /// ```rust\n\
             /// let x = 1;\n\
             /// ```\n\
             pub fn x() {}\n",
        );
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn rust_code_fence_alone_silences() {
        let f = file("/// Quick example.\n/// ```rust\n/// foo();\n/// ```\npub fn x() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn no_run_fence_silences() {
        let f = file("/// Doc.\n/// ```no_run\n/// foo();\n/// ```\npub fn x() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn untagged_fence_does_not_silence() {
        // A bare ``` block won't run via cargo test --doc.
        let f = file("/// Doc.\n/// ```\n/// foo();\n/// ```\npub fn x() {}\n");
        let findings = check(&[f]);
        assert_eq!(findings.len(), 1);
    }

    #[test]
    fn pub_without_doc_skipped() {
        // Caught by undoc_pub_001; this rule must not double-fire.
        let f = file("pub fn x() {}\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn pub_struct_not_checked() {
        let f = file("/// Doc.\npub struct S;\n");
        assert!(check(&[f]).is_empty());
    }

    #[test]
    fn intervening_attribute_does_not_break_doc_detection() {
        let f = file(
            "/// Doc.\n/// ```rust\n/// foo();\n/// ```\n#[inline]\n#[must_use]\npub fn x() {}\n",
        );
        assert!(check(&[f]).is_empty());
    }
}
