//! `INTERNAL-ERROR-VARIANT-DOC-001` — every variant of a pub error
//! enum carries either a `///` doc comment or a `#[error("...")]`
//! attribute (preferably both).
//!
//! Scans every `crates/lazuli_*/src/*.rs` library file for variants
//! inside a `pub enum Name` that derives `thiserror::Error` (or any
//! `Error` token inside a `derive` list). Each variant must be
//! preceded by EITHER a `///` doc line OR a `#[error("...")]`
//! attribute. A variant with neither is opaque to both downstream
//! readers and to `thiserror`'s `Display` impl.
//!
//! ## Why
//!
//! When a downstream pilot sees `Err(LazuliError::FileMissing)` in
//! a trace, they need to read either rustdoc or the `Display` text
//! to know what went wrong. `#[error("file missing: {path}")]` gives
//! the runtime Display; `///` gives the API-doc context. Variants
//! lacking BOTH are guaranteed to surface as bare identifiers in
//! every consumer-facing log.
//!
//! Default severity: `Warning`. Under `tdd-iron-hand` preset: `Error`.
//!
//! ## What's NOT flagged
//!
//! - Variants inside a non-`pub` enum.
//! - Variants inside an enum whose derive list lacks an `Error` token.
//! - Variants with EITHER `///` doc OR `#[error("...")]` (only the
//!   joint absence fires).
//! - Test/example/bench files (`is_library_src = false`).
//!
//! ## Examples
//!
//! ```no_run
//! use lazuli_doctor::error_handling::error_variant_doc_001::check;
//! use lazuli_doctor::internal_hygiene::walker::walk_workspace_rust_sources;
//! use std::path::Path;
//!
//! let files = walk_workspace_rust_sources(Path::new("/path/to/lazuli"));
//! let findings = check(&files);
//! for f in &findings {
//!     println!("{}:{} {}::{}", f.path.display(), f.line, f.enum_name, f.variant_name);
//! }
//! ```
//!
//! ## See also
//!
//! - [`crate::error_handling::error_naming_001`] — same-prefix naming
//!   guard.
//! - [`crate::error_handling::error_non_exhaustive_001`] — enum-level
//!   SemVer guard.

use std::path::{Path, PathBuf};

use crate::internal_hygiene::walker::RustSourceFile;

/// One enum variant lacking both `///` and `#[error("...")]`.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Path relative to the workspace root.
    pub path: PathBuf,
    /// 1-based line number of the variant declaration.
    pub line: usize,
    /// Containing enum identifier.
    pub enum_name: String,
    /// Variant identifier.
    pub variant_name: String,
}

impl Finding {
    /// Stable rule code.
    pub const CODE: &'static str = "INTERNAL-ERROR-VARIANT-DOC-001";

    /// Render the doctor-formatted diagnostic message.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use lazuli_doctor::error_handling::error_variant_doc_001::Finding;
    /// use std::path::PathBuf;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("crates/lazuli_x/src/lib.rs"),
    ///     line: 9,
    ///     enum_name: "ParseError".to_string(),
    ///     variant_name: "Bad".to_string(),
    /// };
    /// let msg = f.message();
    /// assert!(msg.contains("ParseError"));
    /// assert!(msg.contains("Bad"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{}:{} variant `{}::{}` lacks both `///` doc and \
             `#[error(\"...\")]`. Add a Display string via \
             `#[error(\"...\")]` so logs surface a human-readable \
             reason, and/or a `///` doc so the API surface is \
             self-explanatory.",
            self.path.display(),
            self.line,
            self.enum_name,
            self.variant_name,
        )
    }
}

/// Run the rule against a slice of pre-walked library files.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::error_handling::error_variant_doc_001::check;
/// use lazuli_doctor::internal_hygiene::walker::RustSourceFile;
/// use std::path::PathBuf;
///
/// let src = "\
/// #[derive(Debug, thiserror::Error)]\n\
/// pub enum ParseError {\n\
///     Bad,\n\
/// }\n";
/// let f = RustSourceFile {
///     crate_name: "lazuli_test".to_string(),
///     relative_path: PathBuf::from("crates/lazuli_test/src/lib.rs"),
///     absolute_path: PathBuf::from("/abs/crates/lazuli_test/src/lib.rs"),
///     source: src.to_string(),
///     loc_count: 4,
///     is_library_src: true,
/// };
/// let findings = check(&[f]);
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].variant_name, "Bad");
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

/// State machine: walk lines, track whether we're inside a `pub enum`
/// that derives `Error`. Track brace depth so we know when we exit.
/// For each candidate variant line, walk backwards through the
/// immediately preceding attribute/doc lines and check for either
/// a `///` line or a `#[error("...")]` line.
fn scan_file(path: &Path, source: &str, out: &mut Vec<Finding>) {
    let lines: Vec<&str> = source.lines().collect();

    // Pass 1: identify line ranges of pub error enum bodies.
    let bodies = find_error_enum_bodies(&lines);

    // Pass 2: for each body, walk the variant lines.
    for body in &bodies {
        scan_enum_body(path, &lines, body, out);
    }
}

/// `(enum_name, body_start_line_idx, body_end_line_idx)` — `start` is
/// the line index of the opening `{`, `end` is the line index of the
/// matching `}`. Inclusive of both braces.
#[derive(Debug)]
struct EnumBody {
    enum_name: String,
    body_start: usize,
    body_end: usize,
}

fn find_error_enum_bodies(lines: &[&str]) -> Vec<EnumBody> {
    let mut out = Vec::new();
    let mut pending_derive_error = false;
    let mut i = 0;
    while i < lines.len() {
        let line = strip_line_comments(lines[i]);
        let trimmed = line.trim_start();

        if trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.is_empty() {
            i += 1;
            continue;
        }
        if is_derive_with_error(trimmed) {
            pending_derive_error = true;
            i += 1;
            continue;
        }
        if trimmed.starts_with("#[") {
            i += 1;
            continue;
        }

        if pending_derive_error {
            if let Some(name) = parse_pub_enum(trimmed) {
                // Find matching close brace. The opening `{` might be
                // on this line or a following line.
                let mut depth: i32 = 0;
                let mut opened = false;
                let mut j = i;
                while j < lines.len() {
                    let (opens, closes) = count_braces(&strip_line_comments(lines[j]));
                    if opens > 0 {
                        opened = true;
                    }
                    depth = depth + opens as i32 - closes as i32;
                    if opened && depth == 0 {
                        out.push(EnumBody {
                            enum_name: name.to_owned(),
                            body_start: i,
                            body_end: j,
                        });
                        break;
                    }
                    j += 1;
                }
                // Advance past this enum to avoid re-scanning attrs inside.
                i = j + 1;
                pending_derive_error = false;
                continue;
            }
            pending_derive_error = false;
        }
        i += 1;
    }
    out
}

fn scan_enum_body(path: &Path, lines: &[&str], body: &EnumBody, out: &mut Vec<Finding>) {
    // Walk body_start..=body_end. Track depth and consider a line
    // a variant-candidate when depth_before == 1 (just inside the
    // enum's outer `{...}`).
    let mut depth: i32 = 0;
    for i in body.body_start..=body.body_end {
        let stripped = strip_line_comments(lines[i]);
        let trimmed = stripped.trim_start();
        let (opens, closes) = count_braces(&stripped);
        let depth_before = depth;
        depth = depth + opens as i32 - closes as i32;

        if depth_before != 1 {
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("///") || trimmed.starts_with("//!") {
            continue;
        }
        if trimmed.starts_with("#[") {
            continue;
        }
        if trimmed.starts_with('}') {
            continue;
        }

        let Some(variant_name) = parse_variant_ident(trimmed) else {
            continue;
        };

        // Look upward through preceding doc/attr lines (and blanks);
        // stop at the first non-doc, non-attr line. Handles multi-line
        // attributes like
        //
        //   #[error(
        //       "long message ..."
        //   )]
        //   VariantName { ... }
        //
        // by tracking attribute-bracket depth: a closing `)]` / `]` line
        // opens an attribute body that may span multiple physical lines.
        let mut has_doc = false;
        let mut has_error_attr = false;
        let mut attr_depth: i32 = 0; // count of `]` we've seen waiting for `#[`
        let mut k = i;
        while k > body.body_start {
            k -= 1;
            let prev_stripped = strip_line_comments(lines[k]);
            let prev_trimmed = prev_stripped.trim_start();
            if prev_trimmed.is_empty() {
                continue;
            }
            // Multi-line attribute body continuation. Each line ending
            // in `]` (likely `)]`) opens a back-walk through the attr
            // body until we see the matching `#[` line.
            if attr_depth > 0 {
                if prev_trimmed.starts_with("#[error(") {
                    has_error_attr = true;
                    attr_depth -= 1;
                    continue;
                }
                if prev_trimmed.starts_with("#[") {
                    attr_depth -= 1;
                    continue;
                }
                // Still inside the attribute body (string literal line,
                // etc.) — keep walking.
                continue;
            }
            if prev_trimmed.starts_with("///") {
                has_doc = true;
                continue;
            }
            if prev_trimmed.starts_with("//!") {
                continue;
            }
            if prev_trimmed.starts_with("#[error(") {
                has_error_attr = true;
                continue;
            }
            if prev_trimmed.starts_with("#[") {
                continue;
            }
            // Closing bracket of a multi-line attribute, or a comma-only
            // continuation of a previous variant — descend into attr-body
            // mode.
            if prev_trimmed.ends_with("]") || prev_trimmed.ends_with(")]") {
                attr_depth += 1;
                continue;
            }
            break;
        }

        if !has_doc && !has_error_attr {
            out.push(Finding {
                path: path.to_path_buf(),
                line: i + 1,
                enum_name: body.enum_name.clone(),
                variant_name: variant_name.to_owned(),
            });
        }
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

/// Extract the leading identifier of a variant-shape line.
fn parse_variant_ident(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let first = bytes[0] as char;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let end = line
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(line.len());
    if end == 0 {
        return None;
    }
    Some(&line[..end])
}

fn count_braces(line: &str) -> (usize, usize) {
    let mut in_string = false;
    let mut prev = ' ';
    let mut opens = 0;
    let mut closes = 0;
    for c in line.chars() {
        if c == '"' && prev != '\\' {
            in_string = !in_string;
        } else if !in_string {
            if c == '{' {
                opens += 1;
            } else if c == '}' {
                closes += 1;
            }
        }
        prev = c;
    }
    (opens, closes)
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
    include!("error_variant_doc_001_tests.rs");
}
