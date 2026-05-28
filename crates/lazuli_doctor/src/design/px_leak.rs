//! `design-token-px-leak` — `px`/`rem`/`em` literal inside an inline style.
//!
//! Scans `style={{ ... }}` regions for string values that end in `px`,
//! `rem`, or `em`. Values like `"0"`, `"0px"`, and `"auto"` are allowed —
//! the rule fires only when a NON-ZERO dimensional literal appears.
//!
//! Severity: warning
//! (`docs/proposals/design-tokens.md` §6.1).

use std::path::{Path, PathBuf};

use super::helpers::{is_allowed_by_escape_comment, iter_style_spans, walk_tsx_files};

/// One DESIGN-TOKEN-PX-LEAK finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.tsx` file path that contains the leaked dimension.
    pub path: PathBuf,
    /// 1-based line number where the literal was found.
    pub line: usize,
    /// The raw literal (e.g. `"12px"`, `"1.5rem"`).
    pub literal: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "design-token-px-leak";

    /// Render the user-facing diagnostic body — surfaces the literal
    /// and prompts for a declared `space`/Tailwind utility class.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::design::px_leak::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("App.tsx"),
    ///     line: 12,
    ///     literal: "12px".into(),
    /// };
    /// assert!(f.message().contains("12px"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "dimensional literal `{}` used in an inline style prop. \
             Use a declared token (e.g. `tokens.space[\"3\"]` or a `p-3` class).",
            self.literal,
        )
    }
}

/// Run DESIGN-TOKEN-PX-LEAK across every `.tsx` file under `root`.
/// Zero-valued and `auto` literals are filtered before reporting.
/// Results are stably sorted by `(path, line)`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::design::px_leak::check;
/// let findings = check(Path::new("src"));
/// ```
pub fn check(root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for path in walk_tsx_files(root) {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = content.lines().collect();
        findings.extend(check_file(&path, &content, &lines));
    }
    findings.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    findings
}

/// Single-file variant of [`check`]; preferred entry point when the
/// caller already has the file's content in memory (e.g. the LSP).
/// Honors `lazuli-allow: design-token-px-leak` escape comments.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::design::px_leak::check_file;
///
/// let body = r#"<div style={{ padding: "12px" }} />"#;
/// let lines: Vec<&str> = body.lines().collect();
/// let findings = check_file(Path::new("App.tsx"), body, &lines);
/// assert!(!findings.is_empty());
/// ```
pub fn check_file(path: &Path, content: &str, lines: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for span in iter_style_spans(content, lines) {
        for literal in extract_dimensional_literals(span.segment) {
            if is_allowed_by_escape_comment(lines, span.line_idx_0based, Finding::CODE) {
                continue;
            }
            findings.push(Finding {
                path: path.to_path_buf(),
                line: span.line_idx_0based + 1,
                literal,
            });
        }
    }
    findings
}

/// Returns each `<N>(px|rem|em)` string literal in `segment` (e.g. `"12px"`,
/// `"1.5rem"`). Zero-valued literals and `"auto"` are skipped.
fn extract_dimensional_literals(segment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = segment.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        let c = bytes[i];
        if c != b'"' && c != b'\'' {
            i += 1;
            continue;
        }
        let quote = c;
        let after = i + 1;
        let mut end = after;
        while end < bytes.len() && bytes[end] != quote {
            if bytes[end] == b'\\' && end + 1 < bytes.len() {
                end += 2;
                continue;
            }
            end += 1;
        }
        if end >= bytes.len() {
            break;
        }
        let inner = &segment[after..end];
        if let Some(lit) = check_literal(inner) {
            out.push(lit.to_string());
        }
        i = end + 1;
    }
    out
}

/// `Some(s)` when `s` is `<N>(px|rem|em)` and N != 0; `None` otherwise.
fn check_literal(s: &str) -> Option<&str> {
    let s = s.trim();
    if s.is_empty() || s == "auto" {
        return None;
    }
    let unit = if let Some(b) = s.strip_suffix("rem") {
        b
    } else if let Some(b) = s.strip_suffix("em") {
        b
    } else if let Some(b) = s.strip_suffix("px") {
        b
    } else {
        return None;
    };
    if unit.is_empty() {
        return None;
    }
    // Numeric portion: allow optional `-`, digits, single `.`. Bail on anything
    // else so `calc(...)` and shorthand multi-value strings don't false-trigger
    // (they would not pass parsing here).
    let mut chars = unit.chars().peekable();
    if matches!(chars.peek(), Some('-' | '+')) {
        chars.next();
    }
    let mut saw_digit = false;
    let mut saw_dot = false;
    let mut value_is_zero = true;
    for c in chars {
        if c.is_ascii_digit() {
            saw_digit = true;
            if c != '0' {
                value_is_zero = false;
            }
        } else if c == '.' && !saw_dot {
            saw_dot = true;
        } else {
            return None;
        }
    }
    if !saw_digit || value_is_zero {
        return None;
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lines: &[&str]) -> Vec<Finding> {
        let content = lines.join("\n");
        let line_refs: Vec<&str> = content.lines().collect();
        check_file(Path::new("x.tsx"), &content, &line_refs)
    }

    #[test]
    fn trigger_px_literal() {
        let f = run(&[r#"<div style={{ padding: "12px" }} />"#]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].literal, "12px");
    }

    #[test]
    fn trigger_rem_literal() {
        let f = run(&[r#"<div style={{ margin: "1.5rem" }} />"#]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].literal, "1.5rem");
    }

    #[test]
    fn trigger_em_literal() {
        let f = run(&[r#"<div style={{ letterSpacing: "0.025em" }} />"#]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].literal, "0.025em");
    }

    #[test]
    fn allow_zero_and_auto() {
        let f = run(&[r#"<div style={{ padding: "0", margin: "auto", border: "0px" }} />"#]);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn escape_comment_suppresses() {
        let f = run(&[
            "// lazuli-allow: design-token-px-leak — vendor widget",
            r#"<div style={{ padding: "12px" }} />"#,
        ]);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn multi_dimensional_literals_each_fire() {
        let f = run(&[r#"<div style={{ padding: "12px", margin: "1rem", gap: "0.5em" }} />"#]);
        assert_eq!(f.len(), 3, "got: {:?}", f);
    }

    #[test]
    fn ignores_non_dimensional_strings() {
        let f = run(&[r#"<div style={{ content: "12 pixels of text" }} />"#]);
        assert!(f.is_empty(), "got: {:?}", f);
    }
}
