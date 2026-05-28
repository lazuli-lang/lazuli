//! `design-token-hex-leak` — hex literal in `.tsx` style prop or arbitrary
//! Tailwind class value.
//!
//! Two patterns:
//!   1. Arbitrary value inside a className: `bg-[#fff]`, `text-[#7c3aed]`.
//!   2. Inline `style={{ ... }}` with a `"#..."` value literal.
//!
//! Severity: warning in strict, error in production
//! (`docs/proposals/design-tokens.md` §6.1).

use std::path::{Path, PathBuf};

use super::helpers::{
    is_allowed_by_escape_comment, iter_class_strings, iter_style_spans, walk_tsx_files,
};

/// One DESIGN-TOKEN-HEX-LEAK finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.tsx` file path that contains the leaked hex.
    pub path: PathBuf,
    /// 1-based line number where the hex was found.
    pub line: usize,
    /// Hex digits (without the leading `#`).
    pub hex: String,
    /// Whether the leak is in a Tailwind arbitrary-class value or
    /// inside an inline `style={{ ... }}` prop.
    pub site: Site,
}

/// Which JSX surface leaked the hex literal — drives the wording of the
/// remediation hint in [`Finding::message`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    /// `bg-[#fff]` inside a className attribute.
    ArbitraryClass,
    /// `style={{ color: "#fff" }}` inline style.
    InlineStyle,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "design-token-hex-leak";

    /// Render the user-facing diagnostic body. Wording branches on
    /// [`Site`] so the remediation hint points at the correct surface.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::design::hex_leak::{Finding, Site};
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("App.tsx"),
    ///     line: 12,
    ///     hex: "7c3aed".into(),
    ///     site: Site::InlineStyle,
    /// };
    /// assert!(f.message().contains("7c3aed"));
    /// ```
    pub fn message(&self) -> String {
        match self.site {
            Site::ArbitraryClass => format!(
                "hex literal `{}` used inside an arbitrary Tailwind class value. \
                 Declare the color in `design.lzi` and use the named token.",
                self.hex,
            ),
            Site::InlineStyle => format!(
                "hex literal `{}` used in an inline style prop. \
                 Declare the color in `design.lzi` and use `tokens.color.<name>.base`.",
                self.hex,
            ),
        }
    }
}

/// Run DESIGN-TOKEN-HEX-LEAK across every `.tsx` file under `root`.
/// Results are stably sorted by `(path, line)`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::design::hex_leak::check;
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
/// Honors `lazuli-allow: design-token-hex-leak` escape comments.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::design::hex_leak::check_file;
///
/// let body = r##"<div style={{ color: "#7c3aed" }} />"##;
/// let lines: Vec<&str> = body.lines().collect();
/// let findings = check_file(Path::new("App.tsx"), body, &lines);
/// assert_eq!(findings.len(), 1);
/// ```
pub fn check_file(path: &Path, content: &str, lines: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();

    // 1. Arbitrary-class values: scan className strings for `[#...]`.
    for (line_idx, line) in lines.iter().enumerate() {
        for class_str in iter_class_strings(line) {
            for tok in class_str.split_ascii_whitespace() {
                if let Some(hex) = extract_arbitrary_class_hex(tok) {
                    if is_allowed_by_escape_comment(lines, line_idx, Finding::CODE) {
                        continue;
                    }
                    findings.push(Finding {
                        path: path.to_path_buf(),
                        line: line_idx + 1,
                        hex,
                        site: Site::ArbitraryClass,
                    });
                }
            }
        }
    }

    // 2. Inline style hex literals.
    for span in iter_style_spans(content, lines) {
        for hex in extract_hex_literals(span.segment) {
            if is_allowed_by_escape_comment(lines, span.line_idx_0based, Finding::CODE) {
                continue;
            }
            findings.push(Finding {
                path: path.to_path_buf(),
                line: span.line_idx_0based + 1,
                hex,
                site: Site::InlineStyle,
            });
        }
    }

    findings.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(format!("{:?}", a.site).cmp(&format!("{:?}", b.site)))
    });
    findings
}

/// Returns the hex value (without `#`) when `class_token` is an arbitrary
/// Tailwind class containing a hex (`bg-[#fff]`, `text-[#7c3aed]`).
fn extract_arbitrary_class_hex(class_token: &str) -> Option<String> {
    // Look for `[#` pattern.
    let open = class_token.find("[#")?;
    let after = &class_token[open + 2..];
    let close = after.find(']')?;
    let hex = &after[..close];
    if is_valid_hex(hex) {
        Some(hex.to_string())
    } else {
        None
    }
}

/// Scan a style-span segment for `"#xxxxxx"` / `'#xxxxxx'` literals.
/// Returns each hex (without `#`) in order encountered.
fn extract_hex_literals(segment: &str) -> Vec<String> {
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
        // Find the matching quote.
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
        if let Some(stripped) = inner.strip_prefix('#') {
            if is_valid_hex(stripped) {
                out.push(stripped.to_string());
            }
        }
        i = end + 1;
    }
    out
}

fn is_valid_hex(s: &str) -> bool {
    let n = s.len();
    // CSS hex is 3, 4, 6 or 8 chars; we accept the same set.
    matches!(n, 3 | 4 | 6 | 8) && s.chars().all(|c| c.is_ascii_hexdigit())
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
    fn trigger_inline_style_hex() {
        let f = run(&[r##"<div style={{ color: "#7c3aed" }} />"##]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].hex, "7c3aed");
        assert_eq!(f[0].site, Site::InlineStyle);
    }

    #[test]
    fn trigger_arbitrary_class_hex() {
        let f = run(&[r##"<div className="bg-[#7c3aed]" />"##]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].hex, "7c3aed");
        assert_eq!(f[0].site, Site::ArbitraryClass);
    }

    #[test]
    fn allow_no_hex_in_clean_jsx() {
        let f = run(&[r#"<div className="bg-primary" style={{ padding: "12px" }} />"#]);
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn escape_comment_suppresses() {
        let f = run(&[
            "// lazuli-allow: design-token-hex-leak — third-party Mapbox marker",
            r##"<MapMarker color="#FF6B35" />"##,
            r##"<div style={{ color: "#7c3aed" }} /> // lazuli-allow: design-token-hex-leak — note"##,
        ]);
        // Line 1 has `color="#FF6B35"` but it's NOT inside a style={{ }} — it's
        // a JSX attribute value. The rule operates on inline style only, so
        // this attribute is not in scope. Line 2 has both a style hex and an
        // escape comment on the same line — should be suppressed.
        assert!(
            f.is_empty(),
            "every flagged location should be suppressed; got: {:?}",
            f
        );
    }

    #[test]
    fn multi_line_style_block_detects_hex() {
        let lines = vec![
            "<div",
            "  style={{",
            r##"    color: "#7c3aed","##,
            r##"    backgroundColor: "#fff","##,
            "  }}",
            "/>",
        ];
        let f = run(&lines);
        assert_eq!(f.len(), 2, "expected two hex findings; got: {:?}", f);
    }

    #[test]
    fn ignores_string_without_hash() {
        let f = run(&[r#"<div style={{ content: "7c3aed" }} />"#]);
        assert!(f.is_empty(), "no `#` prefix should not match; got: {:?}", f);
    }

    #[test]
    fn ignores_short_invalid_hex() {
        let f = run(&[r##"<div style={{ content: "#xyz" }} />"##]);
        assert!(f.is_empty(), "non-hex chars after `#` should not match");
    }
}
