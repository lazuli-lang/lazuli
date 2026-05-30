//! PREDICATE-EQ-OPERATOR-001 — bare `=` used as equality in a predicate.
//!
//! Severity: error (iron-hand — the retired single-`=` comparison must
//! not compile; warn-only under tdd-strict for a grace window).
//!
//! Fires when a closed-predicate context uses a bare `=` as equality
//! instead of the canonical `==` (SPEC-05); the fix is a one-token
//! rewrite `=` → `==`.
//!
//! ## Why a doctor rule when the parser already rejects it?
//!
//! After SPEC-05 a bare `=` matches no comparison operator, so the
//! predicate lowers to `Unparsed` / drops the filter rather than emitting
//! a precise "you wanted `==`" message. This rule restores that precision:
//! it scans the raw source and names the offending line with a fix-it.
//!
//! ## Scope — predicate contexts only
//!
//! Fires on a bare `=` inside: a ` when ` clause (rule `deny … when`,
//! `tests allows/denies when`, `unique … when`, `invariant when`,
//! conditional policy atom `… when`, webhook `emits … when`), a `filters`
//! block line, or a `.lzx` route-guard `requires <f>.lookup_my.<x> = …`.
//!
//! It deliberately does NOT fire on the three non-comparison `=` roles —
//! assignment/payload binding (`name = input.name`), field default
//! (`tier: T = free`), enum storage (`lead = 10`) — nor on the lifecycle
//! STATE bindings `requires_lifecycle X = state` / `only_when lifecycle
//! X = state` (the underscore `only_when` never matches the space-bounded
//! ` when ` cue). `==` / `!=` / `<=` / `>=` are never flagged.

use std::path::{Path, PathBuf};

/// One PREDICATE-EQ-OPERATOR-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source path (`.lzi` or `.lzx`).
    pub path: PathBuf,
    /// 1-indexed source line of the offending predicate.
    pub line: usize,
    /// The offending line, trimmed, for the message hint.
    pub snippet: String,
}

impl Finding {
    /// Stable diagnostic code used by the dispatcher and JSON output.
    pub const CODE: &'static str = "PREDICATE-EQ-OPERATOR-001";

    /// Render the user-facing diagnostic body with the `=` → `==` fix-it.
    ///
    /// ## Examples
    ///
    /// ```rust
    /// use std::path::PathBuf;
    /// use lazuli_doctor::correctness::predicate_eq_operator_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("f.lzi"),
    ///     line: 12,
    ///     snippet: "deny X when self.s = a".into(),
    /// };
    /// assert!(f.message().contains("=="));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "predicate uses bare `=` as equality — use `==` (SPEC-05). `=` is assignment / \
             field-default / enum-storage only; lifecycle state bindings keep `=`. Offending: `{}`",
            self.snippet
        )
    }
}

/// Run PREDICATE-EQ-OPERATOR-001 over a `.lzi` / `.lzx` source. One
/// finding per predicate-context line carrying a bare `=`.
///
/// ## Examples
///
/// ```rust
/// use std::path::Path;
/// use lazuli_doctor::correctness::predicate_eq_operator_001::check;
///
/// let src = "feature f\n  rule r\n    deny X when self.s = a\n";
/// let findings = check(src, Path::new("f.lzi"));
/// assert_eq!(findings.len(), 1);
/// ```
pub fn check(source: &str, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut in_filters = false;
    let mut filt_indent = 0usize;
    for (idx, raw) in source.lines().enumerate() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - trimmed.len();
        // Close the filters block on dedent.
        if in_filters && indent <= filt_indent {
            in_filters = false;
        }
        if trimmed == "filters" || trimmed.starts_with("filters ") {
            in_filters = true;
            filt_indent = indent;
            continue;
        }
        let is_predicate_ctx = raw.contains(" when ")
            || in_filters
            || (trimmed.starts_with("requires ") && raw.contains(".lookup_my."));
        if is_predicate_ctx && has_bare_eq(raw) {
            findings.push(Finding {
                path: path.to_path_buf(),
                line: idx + 1,
                snippet: trimmed.to_string(),
            });
        }
    }
    findings
}

/// True when `line` contains a standalone `=` that is NOT part of `==`,
/// `!=`, `<=`, or `>=` — i.e. a bare equality `=`.
fn has_bare_eq(line: &str) -> bool {
    let b = line.as_bytes();
    for i in 0..b.len() {
        if b[i] != b'=' {
            continue;
        }
        let prev = if i > 0 { b[i - 1] } else { b' ' };
        let next = if i + 1 < b.len() { b[i + 1] } else { b' ' };
        // Exclude ==, !=, <=, >= (the `=` is adjacent to another operator char).
        if prev == b'=' || prev == b'!' || prev == b'<' || prev == b'>' || next == b'=' {
            continue;
        }
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_rule_when_predicate() {
        let src = "feature f\n  rule r\n    deny X when self.status = archived\n";
        let f = check(src, Path::new("f.lzi"));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 3);
        assert!(f[0].message().contains("=="));
    }

    #[test]
    fn fires_in_filters_block() {
        let src = "feature f\n  query.list q\n    filters\n      status = approved\n";
        assert_eq!(check(src, Path::new("f.lzi")).len(), 1);
    }

    #[test]
    fn fires_on_lookup_my_route_guard() {
        let src = "  view detail\n    requires user.lookup_my.verified = true\n";
        assert_eq!(check(src, Path::new("f.lzx")).len(), 1);
    }

    #[test]
    fn silent_on_canonical_eqeq() {
        let src = "feature f\n  rule r\n    deny X when self.status == archived AND self.owner != nil\n";
        assert!(check(src, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn silent_on_assignment_default_enum() {
        // None of these are predicate contexts, so a bare `=` is fine.
        let src = "\
feature f
  resource R
    tier: CustomerTier = free
    status enum
      lead = 10
  command create
    creates
      name = input.name
";
        assert!(check(src, Path::new("f.lzi")).is_empty());
    }

    #[test]
    fn silent_on_lifecycle_state_bindings() {
        // `requires_lifecycle` / `only_when lifecycle` keep `=` and have no
        // space-bounded ` when ` cue.
        let src = "\
  view detail
    requires_lifecycle Host = basic_details_pending
    forbid_when @role.guest only_when lifecycle Host = complete
";
        assert!(check(src, Path::new("f.lzx")).is_empty());
    }
}
