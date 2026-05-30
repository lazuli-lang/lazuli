//! TEST-VIEW-EXTENSION-VERB-RETIRED-001 — retired view-test verb.
//!
//! Severity: error (iron-hand — exactly one canonical form). Fires when
//! a `.lzx` view `tests` block authors `accepted by <feature>` or
//! `rejected by <feature>` instead of the folded authored
//! `allows extension <feature>` / `denies extension <feature>` form
//! (SPEC-08). The typed `extension` subject carries the extensibility
//! dimension; the verb is the same authored `allows`/`denies` used
//! everywhere else.
//!
//! ## Why a doctor rule when the parser already hard-errors?
//!
//! The parser emits `E-TEST-ACCEPTED-BY-RETIRED` /
//! `E-TEST-REJECTED-BY-RETIRED` on sight, so `lazuli check` never lowers
//! these. This rule is the IR-bypass backstop for tooling that scans
//! `.lzx` sources directly (partial migrations). It scopes to the view
//! `tests` block so unrelated prose is never flagged. See sibling
//! [`super::test_eval_verb_retired_001`] for the agent-eval analogue.
//!
//! ## Detection
//!
//! Source-based scan. A `tests` line opens the block; assertion lines
//! deeper than the `tests` indent are checked, and the block closes when
//! indentation returns to the `tests` level or shallower. Flags trimmed
//! bodies starting with `accepted by ` or `rejected by `.

use std::path::{Path, PathBuf};

/// One TEST-VIEW-EXTENSION-VERB-RETIRED-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source path (`.lzx`).
    pub path: PathBuf,
    /// 1-indexed source line of the offending assertion.
    pub line: usize,
    /// The retired verb phrase that fired (`accepted by` or `rejected by`).
    pub verb: &'static str,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-VIEW-EXTENSION-VERB-RETIRED-001";

    /// Render the user-facing diagnostic body, naming the canonical
    /// replacement.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_view_extension_verb_retired_001::Finding;
    ///
    /// let f = Finding { path: PathBuf::from("a.lzx"), line: 5, verb: "accepted by" };
    /// assert!(f.message().contains("allows extension"));
    /// ```
    pub fn message(&self) -> String {
        let canonical = if self.verb == "accepted by" {
            "allows extension"
        } else {
            "denies extension"
        };
        format!(
            "view test verb `{}` was retired (SPEC-08); use `{} <feature>` — the typed `extension` \
             subject folds view extensibility into the authored allows/denies dialect",
            self.verb, canonical
        )
    }
}

/// Run TEST-VIEW-EXTENSION-VERB-RETIRED-001 over a `.lzx` source.
/// Returns one finding per retired view-test verb, scoped to `tests`
/// blocks.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_view_extension_verb_retired_001::check;
///
/// let src = "\
/// experience catalog
///   view list posts
///     tests
///       accepted by catalog_tags
/// ";
/// let findings = check(src, Path::new("catalog.lzx"));
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].verb, "accepted by");
/// ```
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut tests_indent: Option<usize> = None;
    for (idx, raw) in source.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = raw.len() - trimmed.len();
        if let Some(open) = tests_indent {
            if leading <= open {
                // Block closed; this line may itself open a new `tests`.
                tests_indent = if trimmed == "tests" {
                    Some(leading)
                } else {
                    None
                };
                continue;
            }
            let verb = if trimmed.starts_with("accepted by ") {
                "accepted by"
            } else if trimmed.starts_with("rejected by ") {
                "rejected by"
            } else {
                continue;
            };
            findings.push(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                verb,
            });
        } else if trimmed == "tests" {
            tests_indent = Some(leading);
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "\
experience catalog
  view list posts at \"/posts\"
    anchor @anchor.posts_list
    extensible_by
    tests
      accepted by catalog_tags
      rejected by billing
";

    #[test]
    fn fires_on_retired_view_verbs() {
        let f = check(SRC, Path::new("catalog.lzx"));
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].verb, "accepted by");
        assert_eq!(f[1].verb, "rejected by");
        assert!(f[0].message().contains("allows extension"));
        assert!(f[1].message().contains("denies extension"));
    }

    #[test]
    fn silent_on_canonical_extension_form() {
        let src = SRC
            .replace("accepted by", "allows extension")
            .replace("rejected by", "denies extension");
        assert!(check(&src, Path::new("catalog.lzx")).is_empty());
    }

    #[test]
    fn block_closes_at_dedent() {
        // `accepted by` appearing AFTER the tests block (dedented) is not
        // a view-test assertion and must not fire.
        let src = "\
experience catalog
  view list posts
    tests
      allows extension catalog_tags
  view detail one
    source x.query.by_id
";
        assert!(check(src, Path::new("catalog.lzx")).is_empty());
    }
}
