//! `allow_no_reason` — DOCTOR-ALLOW-NO-REASON-001: advisory reason-tail lint.
//!
//! Severity: Advisory (does not gate CI; teaches, never blocks).
//!
//! A `# doctor:allow <CODE>` directive may carry a human justification:
//!
//! ```text
//! key = "v"  # doctor:allow CONFIG-NOISE-001 — reason "legacy, see #1234"
//! ```
//!
//! Without that `— reason "..."` tail, suppressions accrete into cargo-culted
//! mystery opt-outs that nobody can audit. This rule nudges (does not force)
//! authors to explain *why* a diagnostic is suppressed.
//!
//! Trigger cue: fires when a `# doctor:allow <CODE>` directive has no
//! `— reason "..."` tail.
//!
//! ## Suppression
//!
//! Meta-suppressible: honors a file-level `# doctor:allow
//! DOCTOR-ALLOW-NO-REASON-001` (reuses [`crate::allow_comment`]'s
//! `source_contains_doctor_allow`). A directive that suppresses this very rule
//! is itself never reported as reason-less.
//!
//! ## Relationship to the opt-out helper
//!
//! This rule does NOT change opt-out semantics: a bare `# doctor:allow CODE`
//! still suppresses `CODE`. This is a *parallel* advisory about the bare allow
//! itself — it nudges toward adding a reason, without voiding the suppression.
//!
//! ## Notes
//! See `docs/lazuli_way/comment-hygiene.md` for the teach cell. Separator set
//! (`—` / `--` / whitespace before `reason`) matches the 0006 grammar.

use crate::allow_comment::source_contains_doctor_allow;

/// The diagnostic code this rule emits.
pub const CODE: &str = "DOCTOR-ALLOW-NO-REASON-001";

/// A single reason-less-allow finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonFinding {
    /// 1-based line number the directive sits on.
    pub line: usize,
    /// The code whose suppression lacks a reason.
    pub code: String,
    /// Human-readable message naming the fix.
    pub message: String,
}

/// True when `line` carries a `# doctor:allow <CODE>` with a `reason "..."`
/// tail. The separator before `reason` may be `—` (em dash), `--`, or plain
/// whitespace. Case-insensitive on `doctor:allow` and `reason`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::allow_no_reason::line_allow_has_reason;
///
/// assert!(line_allow_has_reason("# doctor:allow X-1 — reason \"ok\""));
/// assert!(line_allow_has_reason("# doctor:allow X-1 -- reason \"ok\""));
/// assert!(line_allow_has_reason("# doctor:allow X-1 reason \"ok\""));
/// assert!(!line_allow_has_reason("# doctor:allow X-1"));
/// ```
pub fn line_allow_has_reason(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let Some(pos) = lower.find("doctor:allow") else {
        return false;
    };
    let tail = &lower[pos + "doctor:allow".len()..];
    // Find a `reason` keyword followed (eventually) by a `"`.
    let Some(rpos) = tail.find("reason") else {
        return false;
    };
    let after = &tail[rpos + "reason".len()..];
    after.contains('"')
}

/// Extract the `<CODE>` token from a `# doctor:allow <CODE> ...` comment line.
/// Returns `None` when the line is not such a directive.
fn directive_code(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let pos = lower.find("doctor:allow")?;
    // Slice the original (case-preserving) text after the keyword.
    let after = trimmed[pos + "doctor:allow".len()..].trim_start();
    // The code is the first whitespace-delimited token.
    let code = after.split_whitespace().next()?;
    if code.is_empty() {
        None
    } else {
        Some(code.to_string())
    }
}

/// Scan `source` for `# doctor:allow` directives missing a reason tail.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::allow_no_reason::scan_allow_no_reason;
///
/// let bare = scan_allow_no_reason("key = 1  # doctor:allow CONFIG-NOISE-001\n");
/// assert_eq!(bare.len(), 1);
///
/// let reasoned =
///     scan_allow_no_reason("key = 1  # doctor:allow CONFIG-NOISE-001 — reason \"ok\"\n");
/// assert!(reasoned.is_empty());
/// ```
pub fn scan_allow_no_reason(source: &str) -> Vec<ReasonFinding> {
    // Meta opt-out: a file-level `# doctor:allow DOCTOR-ALLOW-NO-REASON-001`
    // silences the whole rule.
    if source_contains_doctor_allow(source, CODE) {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let Some(code) = directive_code(line) else {
            continue;
        };
        // A directive that suppresses THIS rule is the meta opt-out, never a
        // reason-less target. (Also already short-circuited above, but keep the
        // guard so the rule never flags its own opt-out marker.)
        if code.eq_ignore_ascii_case(CODE) {
            continue;
        }
        if line_allow_has_reason(line) {
            continue;
        }
        findings.push(ReasonFinding {
            line: idx + 1,
            code: code.clone(),
            message: format!(
                "`# doctor:allow {code}` has no `— reason \"...\"` tail; \
                 add `— reason \"<why this suppression is justified>\"`"
            ),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_bare_allow() {
        let f = scan_allow_no_reason("key = 1  # doctor:allow CONFIG-NOISE-001\n");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "CONFIG-NOISE-001");
        assert_eq!(f[0].line, 1);
        assert!(f[0].message.contains("reason"));
    }

    #[test]
    fn silent_on_reasoned_allow() {
        let f = scan_allow_no_reason(
            "key = 1  # doctor:allow CONFIG-NOISE-001 — reason \"legacy\"\n",
        );
        assert!(f.is_empty());
    }

    #[test]
    fn accepts_ascii_dash_and_ws_separator() {
        assert!(scan_allow_no_reason("# doctor:allow X-1 -- reason \"x\"\n").is_empty());
        assert!(scan_allow_no_reason("# doctor:allow X-1 reason \"x\"\n").is_empty());
    }

    #[test]
    fn pilots_clean() {
        // Representative of the (already-reasoned) 85-use pilot corpus shape:
        // every allow carries a reason → zero findings. This rule is preventive.
        let corpus = "\
# doctor:allow VOCAB-TESTS-MISSING-001 — reason \"covered by integration suite\"
# doctor:allow LZI-FILE-SIZE-001 — reason \"cohesive customer capsule\"
# doctor:allow CONFIG-NOISE-001 -- reason \"legacy block, tracked in #12\"
";
        assert!(scan_allow_no_reason(corpus).is_empty());
    }

    #[test]
    fn honors_self_suppression_file_level() {
        // A file-level opt-out for THIS rule silences the whole nudge, even
        // though a reason-less CONFIG-NOISE-001 directive is present elsewhere.
        let src = "key = 1  # doctor:allow CONFIG-NOISE-001\n\
                   key = 2  # doctor:allow DOCTOR-ALLOW-NO-REASON-001\n";
        assert!(scan_allow_no_reason(src).is_empty());
    }

    #[test]
    fn own_directive_never_flagged() {
        // The opt-out marker for this rule is never itself reported.
        let src = "key = 2  # doctor:allow DOCTOR-ALLOW-NO-REASON-001\n";
        assert!(scan_allow_no_reason(src).is_empty());
    }

    #[test]
    fn multiple_bare_allows_on_different_lines() {
        let src = "a = 1  # doctor:allow A-1\nb = 2  # doctor:allow B-2\n";
        let f = scan_allow_no_reason(src);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].line, 1);
        assert_eq!(f[1].line, 2);
    }

    #[test]
    fn ignores_non_directive_comments() {
        let f = scan_allow_no_reason("# just a note\nkey = 1\n");
        assert!(f.is_empty());
    }

    #[test]
    fn line_allow_has_reason_unit() {
        assert!(line_allow_has_reason("# doctor:allow X-1 — reason \"ok\""));
        assert!(line_allow_has_reason("# doctor:allow X-1 -- reason \"ok\""));
        assert!(line_allow_has_reason("# doctor:allow X-1 reason \"ok\""));
        assert!(!line_allow_has_reason("# doctor:allow X-1"));
        assert!(!line_allow_has_reason("# doctor:allow X-1 reason no-quote"));
    }
}
