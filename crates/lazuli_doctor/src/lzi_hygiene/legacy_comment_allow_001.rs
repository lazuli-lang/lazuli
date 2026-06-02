//! `legacy_comment_allow_001` — DOCTOR-ALLOW-LEGACY-COMMENT-001: nudge off the
//! deprecated `# doctor:allow` comment waiver onto the first-class node (spec 0028).
//!
//! Severity: Advisory (preventive; NEVER gates — mirrors `LZI-COMMENT-NOISE-001`
//! and `DOCTOR-ALLOW-NO-REASON-001`).
//!
//! Spec 0028 makes `@doctor.allow(CODE, reason: "...")` the canonical waiver.
//! The legacy `# doctor:allow <CODE>` comment is still HONORED during the
//! migration window (the doctor suppresses either form), but it should move to
//! the node so 0029's comment-discipline lint can ban prose comments without
//! carving a permanent waiver hole. This rule fires once per legacy comment-form
//! waiver, pointing at the node form + `lazuli fix`.
//!
//! Trigger cue: fires when a `.lzi`/`.lzx` source line carries a legacy
//! `# doctor:allow <CODE>` waiver comment. Silent on the `@doctor.allow(...)`
//! node form.
//!
//! ## Suppression
//!
//! Honors a file-level `@doctor.allow(DOCTOR-ALLOW-LEGACY-COMMENT-001, ...)` or
//! the legacy `# doctor:allow DOCTOR-ALLOW-LEGACY-COMMENT-001` via the shared
//! [`crate::allow_comment::source_contains_doctor_allow`] bridge.
//!
//! ## Notes
//! See `docs/lazuli_way/comment-hygiene.md`. Banning the comment form outright
//! is 0029's job, NOT this rule's — 0028 only nudges.

use lazuli_syntax::doctor_allow::recognize_legacy_comment_line;

use crate::allow_comment::source_contains_doctor_allow;

/// The diagnostic code this rule emits. Declared as `&str` (elided lifetime),
/// matching the `LZI-COMMENT-NOISE-001` / `DOCTOR-ALLOW-NO-REASON-001` advisory
/// precedent so it stays out of the `&'static str`-scanning diagnostics-registry
/// bridge (advisory rules have no capability home).
pub const CODE: &str = "DOCTOR-ALLOW-LEGACY-COMMENT-001";

/// A single legacy-comment-waiver finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 1-based line number the legacy comment sits on.
    pub line: usize,
    /// The waived rule code (used to render the suggested node form).
    pub code: String,
    /// Human-readable message naming the node form + `lazuli fix`.
    pub message: String,
}

/// Scan `.lzi`/`.lzx` `source` for legacy `# doctor:allow <CODE>` waiver
/// comments. Advisory; the caller must never let these contribute to gate
/// aggregation (mirrors `LZI-COMMENT-NOISE-001`).
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::legacy_comment_allow_001::scan_legacy_comment_allows;
///
/// // The legacy comment form fires.
/// let comment = "# doctor:allow LZI-FILE-SIZE-001 — reason \"gen\"\nfeature x\n";
/// assert_eq!(scan_legacy_comment_allows(comment).len(), 1);
///
/// // The node form does NOT fire.
/// let node = "@doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")\nfeature x\n";
/// assert!(scan_legacy_comment_allows(node).is_empty());
/// ```
pub fn scan_legacy_comment_allows(source: &str) -> Vec<Finding> {
    // Meta opt-out: a waiver (either form) for THIS rule silences it.
    if source_contains_doctor_allow(source, CODE) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let Some((code, reason)) = recognize_legacy_comment_line(line) else {
            continue;
        };
        // Never flag the rule's own legacy opt-out comment.
        if code.eq_ignore_ascii_case(CODE) {
            continue;
        }
        let node = match &reason {
            Some(r) => format!("@doctor.allow({code}, reason: \"{r}\")"),
            None => format!("@doctor.allow({code})"),
        };
        findings.push(Finding {
            line: idx + 1,
            code: code.clone(),
            message: format!(
                "`# doctor:allow {code}` is the deprecated comment waiver form; \
                 replace it with the first-class node `{node}` on the line above \
                 the construct (run `lazuli fix --rule {CODE}` to migrate mechanically)"
            ),
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_comment_form() {
        let src = "# doctor:allow LZI-FILE-SIZE-001 — reason \"gen\"\nfeature x\n";
        let f = scan_legacy_comment_allows(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "LZI-FILE-SIZE-001");
        assert_eq!(f[0].line, 1);
        assert!(f[0].message.contains("@doctor.allow"));
        assert!(f[0].message.contains("lazuli fix"));
    }

    #[test]
    fn silent_on_node_form() {
        let src = "@doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")\nfeature x\n";
        assert!(scan_legacy_comment_allows(src).is_empty());
    }

    #[test]
    fn message_carries_reason_into_node_suggestion() {
        let src = "# doctor:allow X-1 — reason \"because\"\n";
        let f = scan_legacy_comment_allows(src);
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("@doctor.allow(X-1, reason: \"because\")"));
    }

    #[test]
    fn bare_comment_suggests_bare_node() {
        let src = "# doctor:allow Y-2\n";
        let f = scan_legacy_comment_allows(src);
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("@doctor.allow(Y-2)"));
    }

    #[test]
    fn meta_suppression_silences_via_node() {
        // A node waiver for THIS rule silences the nudge even with a legacy
        // comment present (proves the bridge honors the node form here too).
        let src =
            "@doctor.allow(DOCTOR-ALLOW-LEGACY-COMMENT-001, reason: \"migrating\")\n# doctor:allow A-1\n";
        assert!(scan_legacy_comment_allows(src).is_empty());
    }

    #[test]
    fn never_gates_is_caller_contract() {
        // Advisory-only: the dispatcher must exclude CODE from gate aggregation.
        assert_eq!(CODE, "DOCTOR-ALLOW-LEGACY-COMMENT-001");
    }
}
