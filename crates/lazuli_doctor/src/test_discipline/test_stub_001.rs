//! TEST-STUB-001 — unresolved `@TODO authored:` marker inside a `tests` block.
//!
//! ## Rule statement
//!
//! Fires when an `.lzi`/`.lzx`/handler `_test.go` source contains
//! `@TODO authored:` markers inside or adjacent to a `tests` block. The
//! markers are emitted by `lazuli generate command/view/rule/transition`
//! and `lazuli generate handler` (Wave 3 scaffolds) so the scaffolded
//! TDD pair ships RED. The rule clears once the author replaces the
//! `@TODO authored:` comments with real assertions.
//!
//! ## Severity
//!
//! `warning` at strict and production profiles. `info` at prototype.
//! Stays a warning even in production: a `@TODO authored:` marker is a
//! prompt to the author, not a structural bug — production gates would
//! be cruel during in-flight refactors.
//!
//! ## Detection
//!
//! Source-based scan (not IR walk) because authored comments are not
//! preserved in IR. The rule walks the raw `.lzi` / `.lzx` source and
//! reports any line containing the marker substring. Anchored at the
//! line/column of the marker for span-precise diagnostics.

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

// ── detection ─────────────────────────────────────────────────────────────────

/// Run TEST-STUB-001 over a `.lzi`, `.lzx`, or Go `_test.go` source.
/// Returns one finding per marker occurrence (per line).
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
        if let Some(col) = line.find(Finding::MARKER) {
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
}
