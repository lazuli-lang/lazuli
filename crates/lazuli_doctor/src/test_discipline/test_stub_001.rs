//! TEST-STUB-001 — unresolved stub-state marker inside a `tests` block.
//!
//! ## Rule statement
//!
//! Fires when an `.lzi`/`.lzx`/handler `_test.go` source contains a
//! stub-state marker inside or adjacent to a `tests` block. Two
//! marker classes:
//!
//! 1. **Canonical scaffold marker** — `@TODO authored:` emitted by
//!    `lazuli generate command/view/rule/transition` and
//!    `lazuli generate handler` (Wave 3 scaffolds) so the scaffolded
//!    TDD pair ships RED. Available as the public constant
//!    [`Finding::MARKER`].
//! 2. **Extended stub-state vocabulary** — hand-typed markers that
//!    surface the same intent without the codegen prefix (e.g. lines
//!    shaped like `# TODO`, `// FIXME`, `// not implemented`,
//!    `// stub`, `// WIP`, `// Phase 1`). The list lives in
//!    [`EXTENDED_STUB_MARKERS`] and was added on 2026-05-27 after
//!    auditing pilot capsules where the codegen marker had been
//!    replaced by free-form vocab during in-flight refactors.
//!
//! The rule clears once the author replaces the marker with a real
//! assertion.
//!
//! ## Severity
//!
//! `warning` at strict and production profiles. `info` at prototype.
//! Stays a warning even in production: a stub marker is a prompt to
//! the author, not a structural bug — production gates would be cruel
//! during in-flight refactors.
//!
//! ## Detection
//!
//! Source-based scan (not IR walk) because authored comments are not
//! preserved in IR. The rule walks the raw `.lzi` / `.lzx` /
//! `_test.go` source and reports:
//!
//! - any line containing the canonical [`Finding::MARKER`] substring;
//! - any **comment-prefixed** line whose comment body matches one of
//!   the [`EXTENDED_STUB_MARKERS`] tokens (case-insensitive substring
//!   on the comment content only — string literals carrying the same
//!   word in non-comment context are not flagged by this rule; see
//!   the sibling rule `TEST-PINS-STUB-VOCAB-001` for that case).
//!
//! Anchored at the line/column of the marker for span-precise
//! diagnostics.

use std::path::{Path, PathBuf};

// ── output ────────────────────────────────────────────────────────────────────

/// One TEST-STUB-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source path (`.lzi`, `.lzx`, or `_test.go`).
    pub path: PathBuf,
    /// 1-indexed source line.
    pub line: usize,
    /// 1-indexed source column.
    pub column: usize,
    /// The full marker comment text (trimmed), for message hint.
    pub marker_text: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-STUB-001";
    /// Marker text emitted by `lazuli generate` test scaffolding.
    pub const MARKER: &'static str = "@TODO authored:";

    /// Render the user-facing diagnostic body — surfaces the marker
    /// line so the author can find the unresolved stub quickly.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_stub_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("post.lzi"),
    ///     line: 12,
    ///     column: 5,
    ///     marker_text: "# @TODO authored: cover policy".into(),
    /// };
    /// assert!(f.message().contains("@TODO authored:"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "unresolved `@TODO authored:` stub from `lazuli generate` — replace with real \
             assertion. Marker: `{}`",
            self.marker_text
        )
    }
}

// ── extended catalog ──────────────────────────────────────────────────────────

/// Hand-typed stub-state vocab tokens that surface inside comments
/// without the codegen `@TODO authored:` prefix. Matched
/// case-insensitively against the comment body only — non-comment
/// string literals that incidentally contain these tokens are out of
/// scope for this rule (sibling rule `TEST-PINS-STUB-VOCAB-001`
/// covers the assertion-pinning case for Go test sources).
///
/// Closed list — additions require a proposal amendment.
pub const EXTENDED_STUB_MARKERS: &[&str] = &[
    "todo",
    "fixme",
    "not implemented",
    "unimplemented",
    "not yet implemented",
    "stub",
    "wip",
    "phase 1",
    "phase 1.1",
    "phase 2",
    "coming soon",
    "placeholder",
];

// ── detection ─────────────────────────────────────────────────────────────────

/// Run TEST-STUB-001 over a `.lzi`, `.lzx`, or Go `_test.go` source.
/// Returns one finding per marker occurrence (per line). Scans both
/// the canonical [`Finding::MARKER`] and the [`EXTENDED_STUB_MARKERS`]
/// catalog (comment-body matches only); each line emits at most one
/// finding even when multiple tokens hit (the first-matched token
/// surfaces in `marker_text`).
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_stub_001::check;
///
/// let source = "# @TODO authored: cover @policy.update predicate\n";
/// let findings = check(source, Path::new("post.lzi"));
/// assert_eq!(findings.len(), 1);
/// ```
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        // Canonical marker first — preserves the existing behaviour
        // and gives the canonical marker priority over the broader
        // catalog when both match on the same line.
        if let Some(col) = line.find(Finding::MARKER) {
            findings.push(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                column: col + 1,
                marker_text: line.trim().to_string(),
            });
            continue;
        }
        if let Some((col, _matched)) = match_extended_marker_in_comment(line) {
            findings.push(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                column: col + 1,
                marker_text: line.trim().to_string(),
            });
        }
    }
    findings
}

/// Return `(column, matched_token)` when `line` carries a comment
/// (`#`, `//`, or `--`) whose body matches one of
/// [`EXTENDED_STUB_MARKERS`]. The column points to the comment-marker
/// character (e.g. the `#`). Returns `None` for non-comment lines or
/// for comments whose body contains none of the catalog tokens.
fn match_extended_marker_in_comment(line: &str) -> Option<(usize, &'static str)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let leading_ws = line.len() - trimmed.len();
    let (body_offset, body) = if let Some(rest) = trimmed.strip_prefix("//") {
        (leading_ws + 2, rest)
    } else if let Some(rest) = trimmed.strip_prefix('#') {
        (leading_ws + 1, rest)
    } else if let Some(rest) = trimmed.strip_prefix("--") {
        (leading_ws + 2, rest)
    } else {
        return None;
    };
    let body_lower = body.to_ascii_lowercase();
    for token in EXTENDED_STUB_MARKERS {
        if body_lower.contains(token) {
            // Column points to the comment-marker start so the
            // diagnostic anchors on the comment itself.
            return Some((leading_ws, token));
        }
    }
    let _ = body_offset; // retained for potential token-precise columns in future.
    None
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_lzi_tests_block_marker() {
        let source = "\
feature post
  purpose \"...\"

  command publish
    policy @policy.update
    tests
      # @TODO authored: cover @policy.update predicate
      allows as @role.editor
";
        let findings = check(source, Path::new("features/post/post.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 7);
        assert!(findings[0].marker_text.contains("@TODO authored:"));
    }

    #[test]
    fn multiple_markers_each_report() {
        let source = "\
  command publish
    tests
      # @TODO authored: cover policy
      # @TODO authored: cover predicate
      allows when self.id != null
";
        let findings = check(source, Path::new("post.lzi"));
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].line, 3);
        assert_eq!(findings[1].line, 4);
    }

    #[test]
    fn no_marker_no_finding() {
        let source = "\
feature post
  command publish
    tests
      allows as @role.editor
      denies as @role.viewer
";
        assert!(check(source, Path::new("post.lzi")).is_empty());
    }

    #[test]
    fn fires_in_lzx_view_tests_block() {
        let source = "\
surface post web
  view list recent
    tests
      # @TODO authored: list features whose `extends` should be accepted
      accepted by post_extras
";
        let findings = check(source, Path::new("post.web.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 4);
    }

    #[test]
    fn fires_in_handler_test_go() {
        let source = "\
package posthandlers

import \"testing\"

func TestVerifyPassword(t *testing.T) {
\t// @TODO authored: invoke VerifyPassword and assert
\tt.Skip(\"...\")
}
";
        let findings = check(source, Path::new("verify_password_test.go"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 6);
    }

    #[test]
    fn code_constant_is_stable() {
        assert_eq!(Finding::CODE, "TEST-STUB-001");
        assert_eq!(Finding::MARKER, "@TODO authored:");
    }

    // ── extended stub-vocab marker scan (added 2026-05-27) ───────────────────

    #[test]
    fn extended_marker_todo_in_hash_comment_fires() {
        let source = "\
feature post
  command publish
    tests
      # TODO: cover happy path
      allows as @role.editor
";
        let findings = check(source, Path::new("post.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 4);
    }

    #[test]
    fn extended_marker_not_implemented_in_slash_comment_fires() {
        let source = "\
package posthandlers

func TestVerifyPassword(t *testing.T) {
\t// not implemented — refactor pending
\tt.Skip(\"see ticket\")
}
";
        let findings = check(source, Path::new("verify_password_test.go"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 4);
        assert!(findings[0].marker_text.to_lowercase().contains("not implemented"));
    }

    #[test]
    fn extended_marker_fixme_case_insensitive_fires() {
        let source = "\
package h

// FIXME: replace stub when real impl lands
func TestX(t *testing.T) {}
";
        let findings = check(source, Path::new("x_test.go"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 3);
    }

    #[test]
    fn extended_marker_phase_one_one_fires() {
        // Lazuli-specific milestone vocab — Phase 1.1.
        let source = "\
feature post
  # Phase 1.1 placeholder — replace once policy lands
  command publish
";
        let findings = check(source, Path::new("post.lzi"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
    }

    #[test]
    fn extended_marker_wip_fires() {
        let source = "\
package h
// WIP — coverage incoming
func TestX(t *testing.T) {}
";
        let findings = check(source, Path::new("x_test.go"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
    }

    #[test]
    fn extended_marker_canonical_takes_priority_when_both_match_same_line() {
        // Single line carrying both the canonical scaffold marker AND
        // an extended vocab token — only one finding, anchored on the
        // canonical marker. Guards against double-fire regression.
        let source = "      # @TODO authored: TODO replace with real assertion\n";
        let findings = check(source, Path::new("post.lzi"));
        assert_eq!(findings.len(), 1);
        // Canonical marker column (the `@` of `@TODO authored:`),
        // not the leading whitespace of the comment.
        assert!(findings[0].marker_text.contains("@TODO authored:"));
    }

    #[test]
    fn extended_marker_non_comment_line_silent() {
        // Domain string literal that mentions a catalog token but is
        // NOT inside a comment — out of scope for this rule. The
        // sibling TEST-PINS-STUB-VOCAB-001 covers the assertion case.
        let source = "\
package h
func TestX(t *testing.T) {
\tvar x = \"stub\"
\t_ = x
}
";
        assert!(check(source, Path::new("x_test.go")).is_empty());
    }

    #[test]
    fn extended_marker_empty_line_silent() {
        let source = "\n\n\n";
        assert!(check(source, Path::new("post.lzi")).is_empty());
    }
}
