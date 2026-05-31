//! `allow_no_reason` — DOCTOR-ALLOW-NO-REASON-001: advisory reason-tail lint for suppressions.
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
//! Meta-suppressible: honors `# doctor:allow DOCTOR-ALLOW-NO-REASON-001` anywhere
//! in the file. (The directive parser reads one directive per line, so file-level
//! opt-out is the meta escape hatch.) A directive that suppresses this very rule
//! is itself never reported as reason-less.
//!
//! ## Notes
//! See `docs/lazuli_way/comment-hygiene.md` for the teach cell.

use crate::allow_comment::{scan_allow_directives, AllowSet};

/// The diagnostic code this rule emits.
pub const CODE: &str = "DOCTOR-ALLOW-NO-REASON-001";

/// A single reason-less-allow finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonFinding {
    /// 1-based line number the directive sits on.
    pub line: usize,
    /// The code whose suppression lacks a reason.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// Scan `source` for `# doctor:allow` directives missing a reason tail.
pub fn scan_allow_no_reason(source: &str) -> Vec<ReasonFinding> {
    let allow = AllowSet::from_source(source);
    // Meta opt-out: a file-level `# doctor:allow DOCTOR-ALLOW-NO-REASON-001`
    // silences the whole rule.
    if allow.is_allowed_anywhere(CODE) {
        return Vec::new();
    }
    let mut findings = Vec::new();
    for d in scan_allow_directives(source) {
        // This rule only flags *other* suppressions. A directive that suppresses
        // DOCTOR-ALLOW-NO-REASON-001 itself is the meta opt-out, not a target.
        if d.code == CODE {
            continue;
        }
        if d.reason.is_some() {
            continue;
        }
        findings.push(ReasonFinding {
            line: d.line,
            code: d.code.clone(),
            message: format!(
                "`# doctor:allow {}` on line {} has no `— reason \"...\"` tail \
                 (explain why it is suppressed)",
                d.code, d.line
            ),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_reasonless_allow() {
        let src = "key = 1  # doctor:allow CONFIG-NOISE-001\n";
        let f = scan_allow_no_reason(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "CONFIG-NOISE-001");
        assert_eq!(f[0].line, 1);
    }

    #[test]
    fn silent_when_reason_present() {
        let src = "key = 1  # doctor:allow CONFIG-NOISE-001 — reason \"legacy\"\n";
        let f = scan_allow_no_reason(src);
        assert!(f.is_empty());
    }

    #[test]
    fn silent_when_reason_present_double_hyphen() {
        let src = "key = 1  # doctor:allow CONFIG-NOISE-001 -- reason \"legacy\"\n";
        let f = scan_allow_no_reason(src);
        assert!(f.is_empty());
    }

    #[test]
    fn honors_self_suppression_file_level() {
        // A file-level opt-out for THIS rule silences the whole nudge, even
        // though a reason-less CONFIG-NOISE-001 directive is present elsewhere.
        let src = "key = 1  # doctor:allow CONFIG-NOISE-001\n\
                   key = 2  # doctor:allow DOCTOR-ALLOW-NO-REASON-001\n";
        let f = scan_allow_no_reason(src);
        assert!(f.is_empty());
    }

    #[test]
    fn own_directive_never_flagged() {
        // A `# doctor:allow DOCTOR-ALLOW-NO-REASON-001` is the meta opt-out, so
        // it is never itself reported as a reason-less suppression.
        let src = "key = 2  # doctor:allow DOCTOR-ALLOW-NO-REASON-001\n";
        let f = scan_allow_no_reason(src);
        assert!(f.is_empty());
    }

    #[test]
    fn clean_source_is_silent() {
        let src = "key = 1\nother = 2\n";
        let f = scan_allow_no_reason(src);
        assert!(f.is_empty());
    }
}
