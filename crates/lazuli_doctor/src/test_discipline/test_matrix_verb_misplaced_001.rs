//! TEST-MATRIX-VERB-MISPLACED-001 — generated verbs in authored scope.
//!
//! Severity: warning (tdd-strict; promote to iron-hand after the W7
//! sweep). Warns when `permits` / `forbids` — the GENERATED command
//! actor-matrix verbs — appear in an AUTHORED scope where they do not
//! belong: an agent `evals` `case` (`.lzi`) or a view `tests` block
//! (`.lzx`). The generated-vs-authored split is the one test-vocabulary
//! axis SPEC-08 keeps (see `docs/invariants.md`): `permits`/`forbids`
//! signal "this row is machine-derived from `policy @policy.*`, do not
//! hand-edit". Leaking them into an authored scope erases that signal.
//!
//! ## Relationship to the existing matrix smell
//!
//! `crates/lazuli_lsp/src/test_blocks.rs` already fires the INVERSE —
//! hand-authored `permits @` / `forbids @` INSIDE a command `tests`
//! block (where the matrix is generated). This rule covers the other
//! direction: the same generated verbs surfacing in eval / view scopes
//! that have their own authored `allows`/`denies` vocabulary. A command
//! `tests` block is the ONE place `permits`/`forbids` are legitimate, so
//! it is never flagged here.
//!
//! ## Detection
//!
//! Source-based scan, mode-selected by file extension. For `.lzx` the
//! walker tracks view `tests` blocks; for `.lzi` it tracks `evals`
//! blocks. Inside the tracked scope it flags assertion lines whose
//! trimmed body starts with `permits ` or `forbids `.

use std::path::{Path, PathBuf};

/// One TEST-MATRIX-VERB-MISPLACED-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source path (`.lzi` eval scope or `.lzx` view-tests scope).
    pub path: PathBuf,
    /// 1-indexed source line of the misplaced verb.
    pub line: usize,
    /// The generated verb that leaked (`permits` or `forbids`).
    pub verb: &'static str,
    /// The authored scope it leaked into (`evals` or `view tests`).
    pub scope: &'static str,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-MATRIX-VERB-MISPLACED-001";

    /// Render the user-facing diagnostic body.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_matrix_verb_misplaced_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("a.lzi"), line: 7, verb: "forbids", scope: "evals",
    /// };
    /// assert!(f.message().contains("allows"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "`{}` is a GENERATED command actor-matrix verb; it does not belong in an authored {} \
             scope. Use the authored `allows`/`denies` dialect here — `permits`/`forbids` are \
             reserved for the machine-derived `policy @policy.*` matrix you almost never hand-write",
            self.verb, self.scope
        )
    }
}

/// Run TEST-MATRIX-VERB-MISPLACED-001 over a `.lzi` or `.lzx` source.
/// The scope tracked is selected by file extension: `evals` blocks for
/// `.lzi`, view `tests` blocks for `.lzx`.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_matrix_verb_misplaced_001::check;
///
/// let src = "\
/// experience c
///   view list p
///     tests
///       forbids @role.viewer
/// ";
/// let findings = check(src, Path::new("c.lzx"));
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].scope, "view tests");
/// ```
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    let is_lzx = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("lzx"));
    if is_lzx {
        check_scope(source, path, "tests", "view tests")
    } else {
        check_scope(source, path, "evals", "evals")
    }
}

/// Shared block-scoped walker. `opener` is the trimmed line that opens
/// the authored scope; `scope_label` names it in the finding.
fn check_scope(source: &str, path: &Path, opener: &str, scope_label: &'static str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut scope_indent: Option<usize> = None;
    for (idx, raw) in source.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = raw.len() - trimmed.len();
        if let Some(open) = scope_indent {
            if leading <= open {
                scope_indent = if trimmed == opener {
                    Some(leading)
                } else {
                    None
                };
                continue;
            }
            let verb = if trimmed.starts_with("permits ") {
                "permits"
            } else if trimmed.starts_with("forbids ") {
                "forbids"
            } else {
                continue;
            };
            findings.push(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                verb,
                scope: scope_label,
            });
        } else if trimmed == opener {
            scope_indent = Some(leading);
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_forbids_in_view_tests() {
        let src = "\
experience catalog
  view list posts
    tests
      forbids @role.viewer
      allows extension catalog_tags
";
        let f = check(src, Path::new("catalog.lzx"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].verb, "forbids");
        assert_eq!(f[0].scope, "view tests");
    }

    #[test]
    fn fires_on_permits_in_evals() {
        let src = "\
feature c
  agent a
    evals
      case k
        permits @role.admin
";
        let f = check(src, Path::new("c.lzi"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].verb, "permits");
        assert_eq!(f[0].scope, "evals");
    }

    #[test]
    fn silent_on_command_tests_matrix() {
        // A command `tests` block is the ONE legitimate home of the
        // generated matrix verbs — `.lzi` mode tracks `evals`, not
        // command `tests`, so this never fires.
        let src = "\
feature post
  command publish
    tests
      permits @role.editor
      forbids @role.viewer
";
        assert!(check(src, Path::new("post.lzi")).is_empty());
    }

    #[test]
    fn silent_on_authored_allows_denies() {
        let src = "\
experience catalog
  view list posts
    tests
      allows extension catalog_tags
      denies extension billing
";
        assert!(check(src, Path::new("catalog.lzx")).is_empty());
    }
}
