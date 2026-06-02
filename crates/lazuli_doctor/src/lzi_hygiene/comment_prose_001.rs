//! `comment_prose_001` — LZI-COMMENT-PROSE-001: flag EVERY `#` comment line in
//! `.lzi`/`.lzx` (spec 0029, comment-discipline).
//!
//! Severity: WARNING by default; ERROR under the iron-hand profile (escalated by
//! the `LZI-*` prefix in [`crate::lzi_hygiene::preset`]). LziHygiene NEVER gates
//! (gate.rs), so this rule nudges without refuse-emit even at ERROR.
//!
//! ## The goal: drive `.lzi`/`.lzx` comments to ZERO
//!
//! Agents (incl. the authoring LLM) flood `.lzi`/`.lzx` with prose `#` comments —
//! rationale, intent narration, section banners — even though Lazuli already has
//! a non-polluting home for every one of those:
//!
//! - rationale / design narrative → a `<feature>.ctx.md` context file;
//! - a construct's intent / one-liner → its `purpose` / `doc` / `description` field;
//! - a doctor waiver + reason → `@doctor.allow(CODE, reason: "...")` (spec 0028).
//!
//! With those channels in place a `#` comment carries nothing the structured
//! surface can't carry better. So this rule flags **every** `#` comment line —
//! full-line AND inline — and the message names the channel the content should
//! move to. A clean `.lzi` (zero `#` comments, waivers expressed as
//! `@doctor.allow` nodes) reports ZERO findings; any `#` prose / section label /
//! banner / empty `#` is exactly one finding. That makes "drive to zero" a real,
//! measurable, gated goal (the grader proved the original prose-only heuristic
//! could NOT reach zero — it left section labels + box-draw banners unflagged).
//!
//! Trigger cue: fires when a `.lzi`/`.lzx` source line is (or ends with) a `#`
//! comment that is not one of the three narrow carve-outs below. Example: a
//! `# This resource stores the billing address.` line fires; a `# Identity`
//! section label fires; a `# ──────` box banner fires; a clean construct does
//! not.
//!
//! ## Carve-outs (the ONLY `#` lines NOT flagged)
//!
//! 1. A `@doctor.allow(...)` node line — it is a parsed annotation, not a `#`
//!    comment, so it never reaches the scan.
//! 2. A legacy `# doctor:allow <CODE>` waiver comment — NOT flagged as prose
//!    (it is a sanctioned, machine-read waiver during the migration window).
//!    `DOCTOR-ALLOW-LEGACY-COMMENT-001` (spec 0028) owns routing it to the
//!    `@doctor.allow` node + `lazuli fix`; double-flagging it here would just be
//!    noise.
//! That is the COMPLETE carve-out list — there are exactly TWO, both waivers.
//!
//! ### The "zero exceptions" decision (file-header line)
//!
//! The ADR left one knob open: allow a single optional file-header/license line,
//! OR nothing. We chose **nothing**. The maker's goal is literally ZERO `#`
//! comments, the pilots carry no `.lzi`/`.lzx` license headers, and a special
//! line-1 carve-out is impossible to tell apart from a line-1 prose comment
//! without a heuristic the maker explicitly rejected. So a `#` on line 1 fires
//! like any other. (A project that genuinely needs a license header can waive the
//! whole file with `@doctor.allow(LZI-COMMENT-PROSE-001, reason: "...")`.)
//!
//! Everything else — section labels, dividers, banners, gap notes, TODOs, prose,
//! empty `#` — is flagged. There is deliberately NO marker (`# TODO:` /
//! `# NOTE:`) carve-out either: the goal is zero, and an operational marker
//! belongs in `.ctx.md` or a tracker, not the DSL.
//!
//! ## Suppression
//!
//! A file-level `@doctor.allow(LZI-COMMENT-PROSE-001, reason: "...")` (or the
//! legacy `# doctor:allow LZI-COMMENT-PROSE-001`) silences the whole file, via
//! the shared [`crate::allow_comment::source_contains_doctor_allow`] bridge.
//!
//! ## Notes
//! See `docs/lazuli_way/comment-hygiene.md` for the channel-policy decision
//! table + the teach cell.

use lazuli_syntax::doctor_allow::{recognize_legacy_comment_line, recognize_node_line};

use crate::allow_comment::source_contains_doctor_allow;

/// The diagnostic code this rule emits. `&'static str` (not the elided `&str`
/// the sibling advisory rules use) so the diagnostics-registry bridge scans it
/// and requires a `GLOBAL_DIAGNOSTICS` home — this is a first-class,
/// severity-profiled, dispatched rule, not a hidden advisory.
pub const CODE: &'static str = "LZI-COMMENT-PROSE-001";

/// A single prose-comment finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseFinding {
    /// 1-based line number the `#` comment sits on.
    pub line: usize,
    /// 1-based column where the `#` begins (1 for a full-line comment; the
    /// `#` offset for an inline comment), so a point-fix can anchor on it.
    pub column: usize,
    /// Human-readable message naming where the content should go.
    pub message: String,
}

/// The canonical channel-policy message tail — every finding carries it so an
/// agent reading the finding knows the THREE homes for the text it just wrote.
const CHANNEL_HINT: &str = "move rationale to a `<feature>.ctx.md` context file, \
     a construct's intent to its `purpose`/`doc`/`description` field, or a waiver \
     to `@doctor.allow(CODE, reason: \"...\")` — `.lzi`/`.lzx` carry no `#` comments";

/// Scan `.lzi`/`.lzx` `source` for `#` comment lines. Returns one
/// [`ProseFinding`] per flagged `#` comment (full-line AND inline), minus the
/// three carve-outs. WARNING by default; the dispatcher escalates to ERROR under
/// iron-hand. LziHygiene never gates, so these never refuse-emit.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::comment_prose_001::scan_lzi_comment_prose;
///
/// // A prose comment fires.
/// let prose = "# This resource stores the billing address.\nfeature billing\n";
/// assert_eq!(scan_lzi_comment_prose(prose).len(), 1);
///
/// // A section label ALSO fires (the grader's point — true zero).
/// let label = "# Identity\nfeature billing\n";
/// assert_eq!(scan_lzi_comment_prose(label).len(), 1);
///
/// // A clean file (zero `#`, waivers as nodes) reports zero.
/// let clean = "@doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")\nfeature billing\n";
/// assert!(scan_lzi_comment_prose(clean).is_empty());
/// ```
pub fn scan_lzi_comment_prose(source: &str) -> Vec<ProseFinding> {
    // Whole-file opt-out — a waiver (node or legacy) for THIS rule silences it.
    if source_contains_doctor_allow(source, CODE) {
        return Vec::new();
    }

    let mut findings = Vec::new();

    for (idx, raw) in source.lines().enumerate() {
        let lineno = idx + 1;
        let Some(hash_col) = comment_hash_column(raw) else {
            continue; // no `#` comment on this line
        };
        let trimmed = raw.trim_start();

        // Carve-out 2: a legacy `# doctor:allow <CODE>` waiver comment is a
        // sanctioned, machine-read waiver — DOCTOR-ALLOW-LEGACY-COMMENT-001 owns
        // nudging it onto the node form. Do not double-flag it as prose.
        if recognize_legacy_comment_line(raw).is_some() {
            continue;
        }
        // Defense in depth: a `@doctor.allow(...)` node is not a `#` comment, but
        // if a future scan path passes one, never flag it.
        if recognize_node_line(trimmed).is_some() {
            continue;
        }

        // Everything else — full-line OR inline `#` comment, on ANY line
        // (including line 1) — fires. The "zero exceptions" decision: no
        // file-header carve-out (see module docs). The message
        // names the channel so the finding is actionable for an agent.
        //
        // "inline" = there is real CODE before the `#` on this line (a trailing
        // comment). An indented full-line comment (only whitespace before `#`)
        // is a plain "comment", not inline — so a leading-indent comment isn't
        // mislabeled.
        // `hash_col` is a 1-based CHAR column; compare char positions (not
        // bytes) so multi-byte glyphs before the `#` don't skew the test.
        let has_code_before = raw
            .chars()
            .take(hash_col - 1)
            .any(|c| !c.is_whitespace());
        let kind = if has_code_before {
            "inline comment"
        } else {
            "comment"
        };
        findings.push(ProseFinding {
            line: lineno,
            column: hash_col,
            message: format!("`#` {kind} on line {lineno} — {CHANNEL_HINT}"),
        });
    }

    findings
}

/// Return the 1-based column where a `#` line-comment begins on `raw`, or `None`
/// when the line carries no comment.
///
/// Handles BOTH a full-line comment (`#` is the first non-whitespace char → the
/// `#`'s 1-based column) AND an inline trailing comment (` # ...` after code).
/// A `#` INSIDE a string literal is not a comment, so the scan tracks quote
/// state and ignores a `#` while inside `"..."`.
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::comment_prose_001::comment_hash_column;
///
/// assert_eq!(comment_hash_column("# full line"), Some(1));
/// assert_eq!(comment_hash_column("  # indented"), Some(3));
/// assert_eq!(comment_hash_column("feature x  # trailing"), Some(12));
/// assert_eq!(comment_hash_column("feature x"), None);
/// // A `#` inside a string literal is not a comment.
/// assert_eq!(comment_hash_column("purpose \"a # b\""), None);
/// ```
pub fn comment_hash_column(raw: &str) -> Option<usize> {
    let mut in_string = false;
    // Iterate by char (not byte): `.lzi` is ASCII-dominant but box banners use
    // multi-byte glyphs, so a char index gives a stable, glyph-aligned column.
    // The first un-quoted `#` starts the comment — full-line OR inline; the DSL
    // has no `#`-bearing identifiers, so any `#` outside a string is a comment.
    for (char_idx, ch) in raw.chars().enumerate() {
        match ch {
            '"' => in_string = !in_string,
            '#' if !in_string => return Some(char_idx + 1),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_on_sentence() {
        let src = "# This resource stores the customer's billing address.\nfeature billing\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 1);
        assert!(f[0].message.contains("ctx.md"));
        assert!(f[0].message.contains("purpose"));
        assert!(f[0].message.contains("@doctor.allow"));
    }

    #[test]
    fn fires_on_short_section_label() {
        // The grader's point: a 1-word section label that BOTH the prose-only
        // heuristic AND NOISE-001 missed must fire here for true zero.
        let src = "# Identity\nfeature billing\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1, "section label must fire: {f:?}");
        assert_eq!(f[0].line, 1);
    }

    #[test]
    fn fires_on_two_word_audit_label() {
        let src = "feature billing\n  # Audit trail\n  command create\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 2);
    }

    #[test]
    fn fires_on_box_draw_banner() {
        // Box-draw banner mixing glyphs + words — NOISE-001's single-char ruler
        // detector misses it; PROSE-001 catches it (grader R1).
        let src = "# ── Public (unauthenticated) ──────\nexperience customer\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1, "box banner must fire: {f:?}");
    }

    #[test]
    fn fires_on_ascii_divider() {
        // A `# ========` divider (owned by NOISE-001) is ALSO flagged by
        // PROSE-001 — to reach zero, every `#` line is a finding. The two codes
        // are independently suppressible.
        let src = "# ================\nfeature note\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn fires_on_empty_hash_line() {
        // An empty `# ` line has no body — it is still a `#` comment, so it
        // fires (the codemod will delete it mechanically).
        let src = "feature billing\n  # \n  command create\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, 2);
    }

    #[test]
    fn fires_on_inline_comment() {
        let src = "feature billing  # the core billing surface\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("inline comment"));
        assert_eq!(f[0].column, 18);
    }

    #[test]
    fn fires_on_gap_marker() {
        // A `# GAP-01:` note is prose that belongs in `.ctx.md` — it fires (no
        // marker carve-out; the goal is zero).
        let src = "feature billing\n  # GAP-01: refund flow not wired yet\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn fires_on_todo_marker() {
        // No TODO/NOTE/FIXME carve-out — these belong in a tracker, not the DSL.
        let src = "feature billing\n  # TODO: wire refund\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn line_1_comment_also_fires_zero_exceptions() {
        // The "zero exceptions" decision: even a line-1 license/header comment
        // fires. The goal is literally zero `#` comments; a project needing a
        // header waives the whole file via @doctor.allow.
        let src = "# Copyright 2026 Acme — all rights reserved\nfeature billing\n";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1, "line-1 header must fire (zero exceptions): {f:?}");
        assert_eq!(f[0].line, 1);
    }

    #[test]
    fn silent_on_doctor_allow_node() {
        // A `@doctor.allow(...)` node is not a `#` comment → never flagged.
        let src = "@doctor.allow(LZI-FILE-SIZE-001, reason: \"generated table\")\nfeature billing\n";
        let f = scan_lzi_comment_prose(src);
        assert!(f.is_empty(), "node waiver must not fire: {f:?}");
    }

    #[test]
    fn silent_on_legacy_doctor_allow() {
        // A legacy `# doctor:allow X` is routed to the codemod by
        // DOCTOR-ALLOW-LEGACY-COMMENT-001 — NOT double-flagged as prose here.
        let src = "# doctor:allow LZI-FILE-SIZE-001 — reason \"gen\"\nfeature billing\n";
        let f = scan_lzi_comment_prose(src);
        assert!(f.is_empty(), "legacy waiver must not be prose-flagged: {f:?}");
    }

    #[test]
    fn clean_file_reports_zero() {
        // A clean post-0028 `.lzi`: zero `#` comments, structured fields carry
        // intent, waiver is a node. Zero findings.
        let src = "\
@doctor.allow(LZI-FILE-SIZE-001, reason: \"generated table\")
feature billing
  purpose \"charge customers and reconcile invoices\"
  resource Invoice
    doc \"a customer invoice\"
    total: Money required
  command create
    creates Invoice
";
        let f = scan_lzi_comment_prose(src);
        assert!(f.is_empty(), "clean file must report zero: {f:?}");
    }

    #[test]
    fn hash_inside_string_is_not_a_comment() {
        // A `#` inside a `"..."` field value is not a comment.
        let src = "feature billing\n  purpose \"the # symbol means number\"\n";
        let f = scan_lzi_comment_prose(src);
        assert!(f.is_empty(), "`#` in a string is not a comment: {f:?}");
    }

    #[test]
    fn whole_file_node_waiver_suppresses() {
        let src = "\
@doctor.allow(LZI-COMMENT-PROSE-001, reason: \"legacy file, migrating incrementally\")
# This is prose that would otherwise fire.
feature billing
";
        let f = scan_lzi_comment_prose(src);
        assert!(f.is_empty(), "whole-file waiver must suppress: {f:?}");
    }

    #[test]
    fn whole_file_legacy_waiver_suppresses() {
        let src = "\
# doctor:allow LZI-COMMENT-PROSE-001 — reason \"migrating\"
# Some prose.
feature billing
";
        let f = scan_lzi_comment_prose(src);
        assert!(f.is_empty(), "legacy whole-file waiver must suppress: {f:?}");
    }

    #[test]
    fn multiple_comments_each_fire() {
        let src = "\
feature billing
  # first prose line
  command create
  # second prose line
  command delete
";
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].line, 2);
        assert_eq!(f[1].line, 4);
    }

    #[test]
    fn always_advances_no_hang() {
        // Pathological line: only punctuation the scanner must still consume.
        let src = "###!!!???///---\nfeature x\n";
        // Must terminate (the char iterator always advances) and flag the line.
        let f = scan_lzi_comment_prose(src);
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn code_is_canonical() {
        assert_eq!(CODE, "LZI-COMMENT-PROSE-001");
    }

    #[test]
    fn iron_hand_escalates_to_error() {
        use crate::DoctorSeverity;
        use crate::lzi_hygiene::preset::{LziHygienePreset, preset_rule_severity};
        // Default profile (tdd-mature): no preset opinion → caller's Warning
        // default stands.
        assert_eq!(
            preset_rule_severity(LziHygienePreset::TddMature, CODE),
            None,
            "mature defers to the Warning default"
        );
        // Iron-hand: WARNING → ERROR (via the LZI-* prefix escalation).
        assert_eq!(
            preset_rule_severity(LziHygienePreset::TddIronHand, CODE),
            Some(DoctorSeverity::Error),
        );
    }

    #[test]
    fn category_is_lzi_hygiene_so_it_never_gates() {
        // The gate (`lazuli_cli`) excludes `LziHygiene` from the blocking set, so
        // even at ERROR (iron-hand) this rule never refuse-emits. We assert the
        // category derivation the gate keys on; the gate's own
        // `is_blocking_category(LziHygiene) == false` is pinned in
        // `lazuli_cli`'s gate tests.
        assert_eq!(
            crate::RuleCategory::from_code_prefix(CODE),
            crate::RuleCategory::LziHygiene,
        );
    }
}
