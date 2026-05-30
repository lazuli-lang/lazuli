//! TEST-EVAL-VERB-RETIRED-001 — retired eval assertion verb.
//!
//! Severity: error (iron-hand — there is exactly one canonical eval
//! polarity form). Fires when an agent `evals` `case` authors
//! `requires <predicate>` or `forbids <predicate>` instead of the
//! folded authored `allows <predicate>` / `denies <predicate>` form
//! (SPEC-08).
//!
//! ## Why a doctor rule when the parser already hard-errors?
//!
//! The parser emits `E-EVAL-REQUIRES-RETIRED` / `E-EVAL-FORBIDS-RETIRED`
//! the moment it sees the retired spelling, so a normal `lazuli check`
//! never reaches IR with these verbs. This rule is the IR-bypass
//! backstop: tooling that scans `.lzi` sources directly (partial
//! migrations, batch linters that skip the parser) still gets a
//! precise, code-stable diagnostic. It is **scope-guarded** to the
//! `evals` block — the feature-header `requires integration <slot>` and
//! the command `requires @policy.x` precondition share the verb but are
//! a different construct and are NOT flagged.
//!
//! ## Detection
//!
//! Source-based scan. The walker tracks `evals` block membership by
//! indentation (an `evals` line at four-space indent opens the block;
//! any later four-space line that is not `evals` closes it) and flags
//! assertion-depth lines (≥ six-space indent) whose trimmed body starts
//! with `requires ` or `forbids `. See sibling
//! [`super::test_view_extension_verb_retired_001`] for the `.lzx` view
//! analogue and [`super::test_matrix_verb_misplaced_001`] for the
//! inverse (generated verbs leaking into authored scopes).

use std::path::{Path, PathBuf};

/// One TEST-EVAL-VERB-RETIRED-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source path (`.lzi`).
    pub path: PathBuf,
    /// 1-indexed source line of the offending assertion.
    pub line: usize,
    /// The retired verb that fired (`requires` or `forbids`).
    pub verb: &'static str,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "TEST-EVAL-VERB-RETIRED-001";

    /// Render the user-facing diagnostic body, naming the canonical
    /// replacement.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::test_discipline::test_eval_verb_retired_001::Finding;
    ///
    /// let f = Finding { path: PathBuf::from("a.lzi"), line: 9, verb: "requires" };
    /// assert!(f.message().contains("allows"));
    /// ```
    pub fn message(&self) -> String {
        let canonical = if self.verb == "requires" {
            "allows"
        } else {
            "denies"
        };
        format!(
            "eval assertion verb `{}` was retired (SPEC-08); use `{} <predicate>` — agent eval \
             polarity folded into the authored allows/denies dialect, the predicate subject names \
             the dimension",
            self.verb, canonical
        )
    }
}

/// Run TEST-EVAL-VERB-RETIRED-001 over a `.lzi` source. Returns one
/// finding per retired eval-assertion verb, scoped to `evals` blocks.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::test_discipline::test_eval_verb_retired_001::check;
///
/// let src = "\
/// feature c
///   agent a
///     evals
///       case k
///         requires output contains \"ok\"
/// ";
/// let findings = check(src, Path::new("c.lzi"));
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].verb, "requires");
/// ```
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut in_evals = false;
    for (idx, raw) in source.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let leading = raw.len() - trimmed.len();
        // `evals` opens at four-space indent; any other four-space line
        // closes the block (next agent statement or sibling section).
        if leading <= 4 {
            in_evals = trimmed == "evals";
            continue;
        }
        if !in_evals {
            continue;
        }
        let verb = if trimmed.starts_with("requires ") {
            "requires"
        } else if trimmed.starts_with("forbids ") {
            "forbids"
        } else {
            continue;
        };
        findings.push(Finding {
            path: path.to_path_buf(),
            line: idx + 1,
            verb,
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "\
feature customer
  agent summarize
    policy @policy.read
    output stream Text
    model @llm.default
    temperature 0
    seed 1
    prompt \"./p.md\"
    evals
      case redacts_email
        requires customer.email = \"x\"
        forbids output contains @semantic.Email
";

    #[test]
    fn fires_on_retired_eval_verbs() {
        let f = check(SRC, Path::new("customer.lzi"));
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].verb, "requires");
        assert_eq!(f[1].verb, "forbids");
        assert!(f[0].message().contains("allows"));
        assert!(f[1].message().contains("denies"));
    }

    #[test]
    fn silent_on_canonical_allows_denies() {
        let src = SRC
            .replace("requires customer", "allows customer")
            .replace("forbids output", "denies output");
        assert!(check(&src, Path::new("customer.lzi")).is_empty());
    }

    #[test]
    fn does_not_flag_feature_header_requires_integration() {
        // The dependency line shares the verb but is NOT in an evals block.
        let src = "\
feature billing
  requires integration payments: PaymentGateway
  agent a
    evals
      case k
        allows output contains \"ok\"
";
        assert!(check(src, Path::new("billing.lzi")).is_empty());
    }

    #[test]
    fn does_not_flag_command_precondition_requires() {
        let src = "\
feature post
  command delete
    requires @policy.delete
    tests
      allows as @role.editor
";
        assert!(check(src, Path::new("post.lzi")).is_empty());
    }
}
