//! Shared `# doctor:allow <CODE>` opt-out helper.
//!
//! Several rules surface the canonical escape hatch
//! `# doctor:allow <CODE> — reason "..."` in their diagnostic message
//! (or in the rule docs). Without a shared parser, each rule either
//! duplicates the scan or — worse — leaves the wired opt-out as a
//! `// TODO: opt-out wiring lands in a follow-up cell` while the
//! message lies about the escape hatch existing. This module is the
//! single source of truth for that scan: pass it the file path + the
//! rule code, get back `true` when the file carries the comment.
//!
//! Severity discipline: this helper is INTENDED to silence advisory
//! findings (info / hint / warning) when the author explicitly opted
//! out with a reason. Several correctness rules (e.g.
//! `CREATES-EMPTY-BINDINGS-001`) DO consult it for the
//! intentional-no-op-marker shape; the module doc's older "hard-error
//! rules should NOT consult this" note is aspirational, not enforced.
//!
//! ## Spec 0028 — both waiver forms (node + legacy comment)
//!
//! [`source_contains_doctor_allow`] recognizes BOTH the legacy
//! `# doctor:allow <CODE>` comment AND the first-class
//! `@doctor.allow(<CODE>, reason: "...")` node — at the STRING level, via
//! the shared recognizer [`lazuli_syntax::doctor_allow::recognize_node_line`].
//! This is the back-compat bridge: all ~37 `path`/`source` consumers (across
//! `lazuli_doctor` AND `lazuli_doctor_run`) honor node-form waivers with no
//! call-site changes, so a `#→node` migration keeps a previously-suppressed
//! finding suppressed. The structured registry
//! ([`crate::allow_registry`]) is the typed read API for rules that hold a
//! `&Module`.
//!
//! ## Examples
//!
//! ```ignore
//! use std::path::Path;
//! use lazuli_doctor::allow_comment::file_contains_doctor_allow;
//!
//! if !file_contains_doctor_allow(Path::new("billing.lzi"), "MY-RULE-001") {
//!     // emit finding
//! }
//! ```

use std::path::Path;

/// Return `true` when `path` contains a line shaped like
/// `# doctor:allow <CODE>` (optional `— reason "..."` tail). Read
/// failures degrade silently to `false` (no opt-out applied).
///
/// Matching is case-insensitive on both the literal `doctor:allow`
/// keyword and the `CODE` token, so authors can write either
/// `# doctor:allow MY-RULE-001` or `# Doctor:Allow my-rule-001`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::allow_comment::file_contains_doctor_allow;
///
/// // Returns false when the file doesn't exist (degrades silently):
/// assert!(!file_contains_doctor_allow(Path::new("/nope/x.lzi"), "Z"));
/// ```
pub fn file_contains_doctor_allow(path: &Path, code: &str) -> bool {
    let Ok(source) = std::fs::read_to_string(path) else {
        return false;
    };
    source_contains_doctor_allow(&source, code)
}

/// Same as [`file_contains_doctor_allow`] but takes the source text
/// directly — useful when the caller already has the file content
/// in memory (avoids a second disk read).
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::allow_comment::source_contains_doctor_allow;
///
/// let src = "# doctor:allow MY-RULE-001 — reason \"justified\"\nfeature x\n";
/// assert!(source_contains_doctor_allow(src, "MY-RULE-001"));
/// assert!(!source_contains_doctor_allow(src, "OTHER-RULE-001"));
/// ```
pub fn source_contains_doctor_allow(source: &str, code: &str) -> bool {
    let needle_lower = format!("doctor:allow {}", code.to_ascii_lowercase());
    for line in source.lines() {
        let trimmed = line.trim_start();
        // (A) Legacy comment form: `# doctor:allow <CODE>`.
        if trimmed.starts_with('#')
            && trimmed.to_ascii_lowercase().contains(&needle_lower)
        {
            return true;
        }
        // (B) Spec 0028 node form: `@doctor.allow(<CODE>, ...)`. Recognized at
        // the STRING level (grader R1a) so all ~37 path/source consumers honor
        // node-form waivers with ZERO call-site changes — including the
        // correctness rules that hold only a `path`/`source`, never a `&Module`.
        // After a `#→node` migration, a previously-suppressed finding STAYS
        // suppressed (the spec's own gate).
        if let Some((node_code, _reason)) =
            lazuli_syntax::doctor_allow::recognize_node_line(trimmed)
            && node_code.eq_ignore_ascii_case(code)
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_canonical_form_with_reason() {
        let src = "# doctor:allow MY-RULE-001 — reason \"justified\"\nfeature x\n";
        assert!(source_contains_doctor_allow(src, "MY-RULE-001"));
    }

    #[test]
    fn matches_without_reason_tail() {
        assert!(source_contains_doctor_allow("# doctor:allow X-1\n", "X-1"));
    }

    #[test]
    fn case_insensitive_keyword_and_code() {
        let src = "# Doctor:Allow my-rule-001\n";
        assert!(source_contains_doctor_allow(src, "MY-RULE-001"));
    }

    #[test]
    fn matches_indented_comment_inside_block() {
        let src = "feature x\n  # doctor:allow Y-2\n  defaults\n";
        assert!(source_contains_doctor_allow(src, "Y-2"));
    }

    #[test]
    fn does_not_match_different_code() {
        let src = "# doctor:allow A-1\n";
        assert!(!source_contains_doctor_allow(src, "B-2"));
    }

    #[test]
    fn ignores_non_comment_lines_that_mention_the_phrase() {
        // `doctor:allow X` MUST live in a `#`-prefixed comment.
        let src = "purpose \"doctor:allow X-1 is the canonical opt-out\"\n";
        assert!(!source_contains_doctor_allow(src, "X-1"));
    }

    #[test]
    fn file_read_failure_returns_false_silently() {
        use std::path::Path;
        // Non-existent path → file_contains_doctor_allow degrades to
        // false without panicking. Matches the rule contract: missing
        // file means "no opt-out applied", not "error".
        assert!(!file_contains_doctor_allow(
            Path::new("/this/path/does/not/exist.lzi"),
            "ANYTHING"
        ));
    }

    #[test]
    fn empty_source_does_not_match() {
        assert!(!source_contains_doctor_allow("", "X"));
    }

    // ── Spec 0028: node-form `@doctor.allow(CODE, ...)` recognition ──

    #[test]
    fn recognizes_node_form_with_reason() {
        let src = "@doctor.allow(MY-RULE-001, reason: \"justified\")\nfeature x\n";
        assert!(source_contains_doctor_allow(src, "MY-RULE-001"));
        assert!(!source_contains_doctor_allow(src, "OTHER-RULE-001"));
    }

    #[test]
    fn recognizes_node_form_without_reason() {
        let src = "@doctor.allow(X-1)\nfeature x\n";
        assert!(source_contains_doctor_allow(src, "X-1"));
    }

    #[test]
    fn recognizes_indented_node_form() {
        let src = "feature x\n  @doctor.allow(Y-2)\n  command create\n";
        assert!(source_contains_doctor_allow(src, "Y-2"));
    }

    #[test]
    fn node_form_case_insensitive_on_code() {
        let src = "@doctor.allow(my-rule-001)\n";
        assert!(source_contains_doctor_allow(src, "MY-RULE-001"));
    }

    #[test]
    fn bridge_ors_node_and_comment() {
        // The bridge honors BOTH forms (grader R1a). A correctness-category
        // code (CREATES-EMPTY-BINDINGS-001) — the consumer class that holds
        // only `source`, never a `&Module` — is suppressed by either form, so a
        // `#→node` migration keeps a previously-suppressed finding suppressed.
        let comment = "# doctor:allow CREATES-EMPTY-BINDINGS-001 — reason \"marker\"\n";
        let node = "@doctor.allow(CREATES-EMPTY-BINDINGS-001, reason: \"marker\")\n";
        assert!(source_contains_doctor_allow(comment, "CREATES-EMPTY-BINDINGS-001"));
        assert!(source_contains_doctor_allow(node, "CREATES-EMPTY-BINDINGS-001"));
    }

    #[test]
    fn node_form_does_not_match_different_code() {
        let src = "@doctor.allow(A-1)\n";
        assert!(!source_contains_doctor_allow(src, "B-2"));
    }

    #[test]
    fn at_prefixed_code_works() {
        // Codes like `@info.record_column_jsonb` round-trip via the
        // case-insensitive substring match.
        let src = "# doctor:allow @info.record_column_jsonb\n";
        assert!(source_contains_doctor_allow(
            src,
            "@info.record_column_jsonb"
        ));
    }
}
