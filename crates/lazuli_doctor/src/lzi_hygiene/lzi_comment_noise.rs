//! `lzi_comment_noise` — LZI-COMMENT-NOISE-001: comment-noise lint for `.lzi`/`.lzx`.
//!
//! Severity: Advisory (preventive; NEVER gates — mirrors `CONFIG-NOISE-001`).
//!
//! Generalizes `CONFIG-NOISE-001` (TOML) onto the `.lzi`/`.lzx` design surface.
//! Flags two kinds of low-signal noise:
//!
//! 1. **Decorative dividers** — comment lines whose body is a run of one
//!    repeated ruler char (e.g. `# ======`, `# ------`, `# ######`, `# ////`).
//! 2. **High comment-to-semantic ratio** — when comment lines outnumber the
//!    semantic (non-comment, non-blank) lines, the file is mostly prose
//!    (the `CONFIG-NOISE-001` dominance rule).
//!
//! Trigger cue: fires when a `.lzi`/`.lzx` file is comment-dominant
//! (`comment_lines > semantic_lines`) or carries a decorative divider.
//!
//! ## Preventive, not remedial
//!
//! The pilots' `.lzi` files are *already clean* — this rule ships before the
//! problem exists, to keep the design surface signal-dense as AI authors more
//! features. The fixtures in this module are SYNTHETIC; they do not reflect any
//! real pilot file.
//!
//! ## Suppression
//!
//! Honors a file-level `# doctor:allow LZI-COMMENT-NOISE-001` (reuses
//! [`crate::allow_comment`]'s `source_contains_doctor_allow`), mirroring
//! [`crate::lzi_hygiene::file_size_001`].
//!
//! ## Notes
//! See `docs/lazuli_way/comment-hygiene.md` for the teach cell. `.lzi`/`.lzx`
//! comments use the `#` line-comment convention (same as `# doctor:allow`).

use crate::allow_comment::source_contains_doctor_allow;

/// The diagnostic code this rule emits.
pub const CODE: &str = "LZI-COMMENT-NOISE-001";

/// Ruler chars that, repeated, make a decorative divider.
const RULER_CHARS: &[char] = &['-', '=', '*', '#', '/'];

/// Minimum repeated-char run length for a comment body to count as a divider.
const DIVIDER_MIN_LEN: usize = 8;

/// A single comment-noise finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoiseFinding {
    /// 1-based line number; `0` denotes the file-level ratio finding.
    pub line: usize,
    /// Human-readable message.
    pub message: String,
}

/// Scan `.lzi`/`.lzx` `source` for comment-noise findings. Advisory; the caller
/// must never let these contribute to gate aggregation (see module docs).
///
/// ## Examples
///
/// ```rust
/// use lazuli_doctor::lzi_hygiene::lzi_comment_noise::scan_lzi_comment_noise;
///
/// // A clean, signal-dense feature does not fire.
/// let clean = "feature note\n  command create\n";
/// assert!(scan_lzi_comment_noise(clean).is_empty());
///
/// // A decorative divider fires.
/// let noisy = "# ============\nfeature note\n";
/// assert!(!scan_lzi_comment_noise(noisy).is_empty());
/// ```
pub fn scan_lzi_comment_noise(source: &str) -> Vec<NoiseFinding> {
    // Whole-file opt-out — silence the advisory when the source carries the
    // canonical allow comment (mirrors file_size_001's discipline).
    if source_contains_doctor_allow(source, CODE) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let mut comment_lines = 0usize;
    let mut semantic_lines = 0usize;

    for (idx, raw) in source.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            comment_lines += 1;
            if is_decorative_divider(trimmed) {
                findings.push(NoiseFinding {
                    line: idx + 1,
                    message: format!(
                        "decorative divider comment on line {} — low signal; \
                         delete the ruler or replace with a real comment",
                        idx + 1
                    ),
                });
            }
            continue;
        }
        semantic_lines += 1;
    }

    // Dominance rule (CONFIG-NOISE-001): strictly more comments than semantics.
    if comment_lines > semantic_lines && semantic_lines > 0 {
        findings.push(NoiseFinding {
            line: 0,
            message: format!(
                "comment lines ({comment_lines}) exceed semantic lines \
                 ({semantic_lines}); the file is mostly prose"
            ),
        });
    }

    findings
}

/// True when a `#`-prefixed comment line's body is a decorative ruler — a run
/// of `>= DIVIDER_MIN_LEN` of a single [`RULER_CHARS`] character (ignoring
/// interior whitespace).
fn is_decorative_divider(trimmed: &str) -> bool {
    let body: String = trimmed
        .trim_start_matches('#')
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if body.len() < DIVIDER_MIN_LEN {
        return false;
    }
    let first = body.chars().next().unwrap();
    if !RULER_CHARS.contains(&first) {
        return false;
    }
    body.chars().all(|c| c == first)
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: every fixture below is SYNTHETIC. The pilots' real `.lzi` files are
    // already clean; this rule is preventive, so we construct noise by hand.

    #[test]
    fn fires_on_decorative_divider() {
        let src = "# =================\nfeature note\n";
        let f = scan_lzi_comment_noise(src);
        assert!(f.iter().any(|x| x.message.contains("decorative divider")));
    }

    #[test]
    fn fires_on_each_ruler_char() {
        for ruler in ["--------", "========", "********", "########", "////////"] {
            let src = format!("# {ruler}\nfeature note\n");
            assert!(
                !scan_lzi_comment_noise(&src).is_empty(),
                "ruler {ruler:?} should fire"
            );
        }
    }

    #[test]
    fn fires_on_comment_dominant_lzi() {
        let src = "# c1\n# c2\n# c3\nfeature note\n";
        let f = scan_lzi_comment_noise(src);
        assert!(f.iter().any(|x| x.line == 0 && x.message.contains("mostly prose")));
    }

    #[test]
    fn clean_feature_does_not_fire() {
        // Representative of the (already-clean) pilot shape: signal-dense, no
        // dividers, comments do not dominate.
        let src = "feature note\n  resource Note { title: Text }\n  command create\n";
        assert!(scan_lzi_comment_noise(src).is_empty());
    }

    #[test]
    fn short_ruler_is_not_a_divider() {
        // Below DIVIDER_MIN_LEN — not flagged as a divider.
        let src = "# ---\nfeature note\n  command create\n";
        let f = scan_lzi_comment_noise(src);
        assert!(f.iter().all(|x| !x.message.contains("decorative divider")));
    }

    #[test]
    fn doctor_allow_suppresses() {
        let src =
            "# ============\nfeature note  # doctor:allow LZI-COMMENT-NOISE-001 — reason \"generated\"\n";
        assert!(scan_lzi_comment_noise(src).is_empty());
    }

    #[test]
    fn never_gates_is_caller_contract() {
        // This rule has no severity field of its own; like CONFIG-NOISE-001 it
        // is advisory-only. The dispatcher must exclude CODE from gate
        // aggregation. We assert the CODE constant is the stable identity the
        // dispatcher keys on.
        assert_eq!(CODE, "LZI-COMMENT-NOISE-001");
    }
}
