//! `design-token-shadow-leak` — `boxShadow` string literal in style prop.
//!
//! Fires when an inline style block contains a `boxShadow: "..."`
//! string literal — any literal counts (multi-layer shadows, single-layer
//! shadows, or `"none"`). The user should declare a `shadow` token in
//! `design.lzi` and use `tokens.shadow.<name>` instead.
//!
//! Severity: warning
//! (`docs/proposals/design-tokens.md` §6.1).

use std::path::{Path, PathBuf};

use super::helpers::{is_allowed_by_escape_comment, iter_style_spans, walk_tsx_files};

/// One DESIGN-TOKEN-SHADOW-LEAK finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.tsx` file path that contains the leaked shadow.
    pub path: PathBuf,
    /// 1-based line number where the value was found.
    pub line: usize,
    /// The raw boxShadow value as authored.
    pub value: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "design-token-shadow-leak";

    /// Render the user-facing diagnostic body — surfaces the literal
    /// and prompts for a declared `shadow` token (or Tailwind class).
    pub fn message(&self) -> String {
        format!(
            "boxShadow value `{}` is a raw literal — declare a `shadow` token in `design.lzi` \
             and use `tokens.shadow.<name>` (or the `shadow-<name>` Tailwind class).",
            self.value,
        )
    }
}

/// Run DESIGN-TOKEN-SHADOW-LEAK across every `.tsx` file under `root`.
/// Results are stably sorted by `(path, line)`.
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
/// Honors `lazuli-allow: design-token-shadow-leak` escape comments.
pub fn check_file(path: &Path, content: &str, lines: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for span in iter_style_spans(content, lines) {
        for value in extract_boxshadow_strings(span.segment) {
            if is_allowed_by_escape_comment(lines, span.line_idx_0based, Finding::CODE) {
                continue;
            }
            findings.push(Finding {
                path: path.to_path_buf(),
                line: span.line_idx_0based + 1,
                value,
            });
        }
    }
    findings
}

/// Returns each `boxShadow: "..."` value in `segment`.
fn extract_boxshadow_strings(segment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let key = "boxShadow";
    let mut start = 0;
    while let Some(rel) = segment[start..].find(key) {
        let pos = start + rel;
        let preceding_ok = match pos.checked_sub(1).and_then(|p| segment.as_bytes().get(p).copied())
        {
            None => true,
            Some(b) => !is_ident_byte(b),
        };
        let after = pos + key.len();
        if !preceding_ok {
            start = after;
            continue;
        }
        let bytes = segment.as_bytes();
        let mut j = after;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if bytes.get(j).copied() != Some(b':') {
            start = after;
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        let Some(&q) = bytes.get(j) else {
            break;
        };
        if q != b'"' && q != b'\'' {
            start = after;
            continue;
        }
        j += 1;
        let value_start = j;
        let mut value_end = j;
        while value_end < bytes.len() && bytes[value_end] != q {
            if bytes[value_end] == b'\\' && value_end + 1 < bytes.len() {
                value_end += 2;
                continue;
            }
            value_end += 1;
        }
        if value_end >= bytes.len() {
            break;
        }
        out.push(segment[value_start..value_end].to_string());
        start = value_end + 1;
    }
    out
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
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
    fn trigger_single_layer_shadow() {
        let f = run(&[r#"<div style={{ boxShadow: "0 1px 3px rgba(0,0,0,0.1)" }} />"#]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].value, "0 1px 3px rgba(0,0,0,0.1)");
    }

    #[test]
    fn trigger_multi_layer_shadow() {
        let f = run(&[
            r#"<div style={{ boxShadow: "0 1px 2px rgba(0,0,0,0.05), 0 4px 6px rgba(0,0,0,0.1)" }} />"#,
        ]);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn allow_clean_jsx() {
        let f = run(&[r#"<div className="shadow-md" />"#]);
        assert!(f.is_empty());
    }

    #[test]
    fn escape_comment_suppresses() {
        let f = run(&[
            "// lazuli-allow: design-token-shadow-leak — vendor widget chrome",
            r#"<div style={{ boxShadow: "0 1px 3px rgba(0,0,0,0.1)" }} />"#,
        ]);
        assert!(f.is_empty(), "got: {:?}", f);
    }

    #[test]
    fn similar_property_not_matched() {
        let f = run(&[r#"<div style={{ myBoxShadow: "0 1px 3px black" }} />"#]);
        assert!(f.is_empty(), "got: {:?}", f);
    }
}
