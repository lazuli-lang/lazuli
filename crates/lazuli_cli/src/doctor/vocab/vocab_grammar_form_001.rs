//! VOCAB-GRAMMAR-FORM-001 - legacy curly-brace dialect in `.lzi` source.
//!
//! Fires when a block keyword opens with `{` rather than the canonical
//! indentation form. Two forms for one concept is a Rule Zero violation:
//! agents trained on either form drift.
//!
//! NOTE: this rule walks the RAW SOURCE TEXT, not the IR. Lowering strips
//! the dialect signal.
//!
//! Severity: `warning` (strict), `warning` (production).

use std::path::{Path, PathBuf};

const KEYWORDS_WITH_BLOCK: &[&str] = &[
    "aggregate",
    "resource",
    "feature",
    "domain",
    "policies",
    "command",
    "query.list",
    "query.lookup",
    "query.sql",
    "workflow",
    "job",
    "webhook",
    "notification",
];

// -- output -------------------------------------------------------------------

/// One VOCAB-GRAMMAR-FORM-001 finding: a block opened with `{`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzi` file.
    pub path: PathBuf,
    /// 1-indexed source line.
    pub line: usize,
    /// Offending block keyword.
    pub keyword: String,
    /// Original source line.
    pub raw_line: String,
}

impl Finding {
    pub const CODE: &'static str = "VOCAB-GRAMMAR-FORM-001";

    pub fn message(&self) -> String {
        format!(
            "line {} uses curly-brace dialect (`{} ... {{`) - Lazuli's canonical \
             form is indentation-based. Rewrite to indented children. \
             Rule Zero forbids two grammatical forms for one concept.",
            self.line, self.keyword
        )
    }
}

// -- detection ----------------------------------------------------------------

/// Run VOCAB-GRAMMAR-FORM-001 over raw `.lzi` source text.
///
/// `path` is the source `.lzi` file - used to anchor findings; no I/O is
/// performed here. This rule intentionally does not accept IR because lowering
/// strips the legacy grammar form.
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    source
        .lines()
        .enumerate()
        .filter_map(|(idx, raw_line)| {
            let line_without_comment = strip_comment_outside_string(raw_line);
            let trimmed = line_without_comment.trim();
            if trimmed.is_empty() || !line_ends_with_open_brace(trimmed) {
                return None;
            }

            let keyword = leading_keyword(trimmed)?;
            if !KEYWORDS_WITH_BLOCK.contains(&keyword.as_str()) {
                return None;
            }

            Some(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                keyword,
                raw_line: raw_line.to_owned(),
            })
        })
        .collect()
}

// -- internals ----------------------------------------------------------------

fn line_ends_with_open_brace(line: &str) -> bool {
    let line_without_comment = strip_comment_outside_string(line);
    let trimmed = line_without_comment.trim_end();
    trimmed.ends_with('{') && !brace_is_inside_string(trimmed)
}

fn leading_keyword(line: &str) -> Option<String> {
    let line_without_comment = strip_comment_outside_string(line);
    let mut parts = line_without_comment.split_whitespace();
    let first = parts.next()?;

    // Require at least one whitespace-separated token after the keyword so
    // `aggregate {` and `aggregate Customer {` are accepted, but `aggregate{`
    // is not treated as a grammar form.
    parts.next()?;

    if first
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
    {
        Some(first.to_owned())
    } else {
        None
    }
}

fn strip_comment_outside_string(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_string = false;
    let mut escaped = false;

    for ch in line.chars() {
        if ch == '#' && !in_string {
            break;
        }

        out.push(ch);

        if ch == '"' && !escaped {
            in_string = !in_string;
        }

        escaped = ch == '\\' && !escaped;
        if ch != '\\' {
            escaped = false;
        }
    }

    out
}

fn brace_is_inside_string(line: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;

    for ch in line.chars() {
        if ch == '{' && !in_string {
            return false;
        }

        if ch == '"' && !escaped {
            in_string = !in_string;
        }

        escaped = ch == '\\' && !escaped;
        if ch != '\\' {
            escaped = false;
        }
    }

    true
}

// -- tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn check_src(source: &str) -> Vec<Finding> {
        check(source, Path::new("features/test/test.lzi"))
    }

    #[test]
    fn positive_aggregate_open_brace_fires() {
        let findings = check_src("aggregate Customer {\n  ...\n}");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 1);
        assert_eq!(findings[0].keyword, "aggregate");
        assert_eq!(Finding::CODE, "VOCAB-GRAMMAR-FORM-001");
    }

    #[test]
    fn positive_resource_open_brace_fires() {
        let findings = check_src("resource Post {\n  title: Text\n}");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 1);
        assert_eq!(findings[0].keyword, "resource");
    }

    #[test]
    fn positive_command_with_args_open_brace_fires() {
        let findings = check_src("command update_status {\n  ...\n}");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 1);
        assert_eq!(findings[0].keyword, "command");
    }

    #[test]
    fn negative_indented_resource_does_not_fire() {
        assert!(check_src("resource Post\n  title: Text\n").is_empty());
    }

    #[test]
    fn negative_default_inline_literal_does_not_fire() {
        assert!(check_src("  status: Text default { value: \"draft\" }").is_empty());
    }

    #[test]
    fn negative_comment_with_brace_does_not_fire() {
        assert!(check_src("# aggregate Customer { ... }").is_empty());
    }

    #[test]
    fn negative_string_literal_brace_does_not_fire() {
        assert!(check_src("  body: Text default \"resource {x}\"").is_empty());
    }

    #[test]
    fn multiple_fires_one_per_line() {
        let findings = check_src(
            "feature crm {\n  domain {\n    resource Customer {\n      name: Text\n    }\n  }\n}",
        );

        assert_eq!(findings.len(), 3);
        assert_eq!(
            findings.iter().map(|f| f.line).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            findings
                .iter()
                .map(|f| f.keyword.as_str())
                .collect::<Vec<_>>(),
            vec!["feature", "domain", "resource"]
        );
    }

    #[test]
    fn negative_comment_after_canonical_line_does_not_fire() {
        assert!(check_src("resource Post # legacy example {").is_empty());
    }

    #[test]
    fn positive_trailing_comment_after_open_brace_fires() {
        let findings = check_src("resource Post { # legacy dialect");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].keyword, "resource");
    }
}
