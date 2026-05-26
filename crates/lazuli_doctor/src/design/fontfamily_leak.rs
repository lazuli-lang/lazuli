//! `design-token-fontfamily-leak` — raw `fontFamily` string in style prop.
//!
//! Fires when an inline style block contains `fontFamily: "..."` whose
//! quoted value is NOT a declared family name from `design.lzi`'s
//! `typography.family` group (proxied via the `font` bucket in
//! `allowlist.json`).
//!
//! Severity: warning in strict, error in production
//! (`docs/proposals/design-tokens.md` §6.1).

use std::path::{Path, PathBuf};

use super::helpers::{Allowlist, is_allowed_by_escape_comment, iter_style_spans, walk_tsx_files};

/// One DESIGN-TOKEN-FONTFAMILY-LEAK finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `.tsx` file path that contains the leaked family.
    pub path: PathBuf,
    /// 1-based line number where the value was found.
    pub line: usize,
    /// The raw fontFamily string as authored.
    pub value: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "design-token-fontfamily-leak";

    /// Render the user-facing diagnostic body — surfaces the offending
    /// family literal and prompts for a `typography.family` declaration.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::design::fontfamily_leak::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("App.tsx"),
    ///     line: 12,
    ///     value: "Helvetica".into(),
    /// };
    /// assert!(f.message().contains("Helvetica"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "fontFamily value `{}` is not a declared family token. \
             Add it to `typography.family` in `design.lzi` or reference an existing family.",
            self.value,
        )
    }
}

/// Run DESIGN-TOKEN-FONTFAMILY-LEAK across every `.tsx` file under
/// `root`. Families listed in `allowlist.json` `font` bucket are
/// silenced. Results are stably sorted by `(path, line)`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::design::fontfamily_leak::check;
/// let findings = check(Path::new("src"), &allowlist);
/// ```
pub fn check(root: &Path, allowlist: &Allowlist) -> Vec<Finding> {
    let mut findings = Vec::new();
    for path in walk_tsx_files(root) {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let lines: Vec<&str> = content.lines().collect();
        findings.extend(check_file(&path, &content, &lines, allowlist));
    }
    findings.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
    findings
}

/// Single-file variant of [`check`]; preferred entry point when the
/// caller already has the file's content in memory (e.g. the LSP).
/// Honors `lazuli-allow: design-token-fontfamily-leak` escape comments.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::design::fontfamily_leak::check_file;
///
/// let lines: Vec<&str> = content.lines().collect();
/// let findings = check_file(Path::new("App.tsx"), content, &lines, &allowlist);
/// ```
pub fn check_file(
    path: &Path,
    content: &str,
    lines: &[&str],
    allowlist: &Allowlist,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for span in iter_style_spans(content, lines) {
        for value in extract_fontfamily_strings(span.segment) {
            if allowlist.is_known_font_token(&value) {
                continue;
            }
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

/// Walk the style segment, looking for `fontFamily : "..."` or
/// `fontFamily : '...'`. Returns each captured string value.
fn extract_fontfamily_strings(segment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let key = "fontFamily";
    let mut start = 0;
    while let Some(rel) = segment[start..].find(key) {
        let pos = start + rel;
        // Require preceding char to be non-identifier or boundary.
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
        // Skip whitespace, then expect `:`.
        let mut j = after;
        let bytes = segment.as_bytes();
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
        // Expect `"` or `'`.
        let Some(&q) = bytes.get(j) else {
            break;
        };
        if q != b'"' && q != b'\'' {
            start = after;
            continue;
        }
        j += 1;
        // Find matching quote.
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
    use std::collections::HashMap;

    fn al(font_tokens: &[&str]) -> Allowlist {
        let mut buckets = HashMap::new();
        buckets.insert(
            "font".to_string(),
            font_tokens.iter().map(|s| s.to_string()).collect(),
        );
        Allowlist { buckets }
    }

    fn run(lines: &[&str], allowlist: &Allowlist) -> Vec<Finding> {
        let content = lines.join("\n");
        let line_refs: Vec<&str> = content.lines().collect();
        check_file(Path::new("x.tsx"), &content, &line_refs, allowlist)
    }

    #[test]
    fn trigger_undeclared_family() {
        let allowlist = al(&["sans", "mono"]);
        let f = run(
            &[r#"<div style={{ fontFamily: "Comic Sans, system-ui" }} />"#],
            &allowlist,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].value, "Comic Sans, system-ui");
    }

    #[test]
    fn allow_declared_token_name() {
        let allowlist = al(&["sans", "mono"]);
        let f = run(
            &[r#"<div style={{ fontFamily: "sans" }} />"#],
            &allowlist,
        );
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn escape_comment_suppresses() {
        let allowlist = al(&["sans"]);
        let f = run(
            &[
                "// lazuli-allow: design-token-fontfamily-leak — embedded PDF widget",
                r#"<div style={{ fontFamily: "Times New Roman" }} />"#,
            ],
            &allowlist,
        );
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn ignores_non_fontfamily_strings() {
        let allowlist = al(&["sans"]);
        let f = run(
            &[r##"<div style={{ color: "#fff", padding: "12px" }} />"##],
            &allowlist,
        );
        assert!(f.is_empty(), "found: {:?}", f);
    }

    #[test]
    fn similar_property_name_not_matched() {
        // `notFontFamily` (hypothetical) must not be matched as fontFamily.
        let allowlist = al(&["sans"]);
        let f = run(
            &[r#"<div style={{ myFontFamily: "Arial" }} />"#],
            &allowlist,
        );
        assert!(f.is_empty(), "found: {:?}", f);
    }
}
