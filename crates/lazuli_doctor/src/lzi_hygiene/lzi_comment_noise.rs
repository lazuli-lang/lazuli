//! `lzi_comment_noise` — LZI-COMMENT-NOISE-001: comment-noise lint for `.lzi`/`.lzx`.
//!
//! Severity: Advisory (preventive; does not gate CI).
//!
//! Generalizes `CONFIG-NOISE-001` (TOML) onto the `.lzi`/`.lzx` design surface.
//! Flags two kinds of low-signal noise:
//!
//! 1. **Decorative dividers** — comment lines that are mostly a run of one
//!    repeated punctuation char (e.g. `# ======`, `# ------`, `# ######`).
//! 2. **High comment-to-semantic ratio** — when comment lines vastly outnumber
//!    the semantic (non-comment, non-blank) lines, the file is mostly prose.
//!
//! Trigger cue: fires when a `.lzi`/`.lzx` file's comment-to-semantic ratio
//! exceeds the threshold or a decorative divider is present.
//!
//! ## Preventive, not remedial
//!
//! The pilots' `.lzi` files are *already clean* — this rule ships before the
//! problem exists, to keep the design surface signal-dense as files grow. The
//! fixtures in this module are SYNTHETIC; they do not reflect any real pilot file.
//!
//! ## Suppression
//!
//! Honors `# doctor:allow LZI-COMMENT-NOISE-001` on the offending line (divider)
//! or anywhere in the file (ratio).
//!
//! ## Notes
//! See `docs/lazuli_way/comment-hygiene.md` for the teach cell. `.lzi`/`.lzx`
//! comments use the `#` line-comment convention (same as `# doctor:allow`).

use crate::allow_comment::AllowSet;

/// The diagnostic code this rule emits.
pub const CODE: &str = "LZI-COMMENT-NOISE-001";

/// Default threshold: comment lines per semantic line above which the file is
/// flagged as comment-heavy.
const DEFAULT_RATIO_THRESHOLD: f64 = 2.0;

/// A single comment-noise finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoiseFinding {
    /// 1-based line number (0 = file-level / ratio finding).
    pub line: usize,
    /// Human-readable message.
    pub message: String,
}

/// Scan `.lzi`/`.lzx` `source` for comment-noise findings.
pub fn scan_lzi_comment_noise(source: &str) -> Vec<NoiseFinding> {
    scan_with_threshold(source, DEFAULT_RATIO_THRESHOLD)
}

/// Scan with an explicit ratio threshold.
pub fn scan_with_threshold(source: &str, ratio_threshold: f64) -> Vec<NoiseFinding> {
    let allow = AllowSet::from_source(source);
    let mut findings = Vec::new();

    let mut comment_lines = 0usize;
    let mut semantic_lines = 0usize;

    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_comment = trimmed.starts_with('#');
        if is_comment {
            // Evaluate the divider on the text BEFORE any embedded
            // `# doctor:allow` directive, so a same-line suppression does not
            // itself defeat divider detection.
            let divider_part = match trimmed.find("# doctor:allow") {
                // Skip the leading `#` so we don't treat the *second* `#` (the
                // directive marker) as the start of the directive on a bare
                // comment line.
                Some(0) => trimmed,
                Some(pos) => trimmed[..pos].trim(),
                None => trimmed,
            };
            if is_decorative_divider(divider_part) && !allow.is_allowed(line_no, CODE) {
                findings.push(NoiseFinding {
                    line: line_no,
                    message: format!(
                        "decorative divider comment on line {line_no} (low signal)"
                    ),
                });
            }
            comment_lines += 1;
        } else {
            semantic_lines += 1;
        }
    }

    if semantic_lines > 0 {
        let ratio = comment_lines as f64 / semantic_lines as f64;
        if ratio > ratio_threshold && !allow.is_allowed_anywhere(CODE) {
            findings.push(NoiseFinding {
                line: 0,
                message: format!(
                    "comment-to-semantic ratio {ratio:.1} exceeds {ratio_threshold:.1} \
                     (file is mostly prose)"
                ),
            });
        }
    }

    findings
}

/// True when a comment line is a decorative divider (mostly one repeated char).
fn is_decorative_divider(trimmed: &str) -> bool {
    let body = trimmed.trim_start_matches('#').trim();
    if body.len() < 4 {
        return false;
    }
    let mut counts = std::collections::HashMap::new();
    for ch in body.chars().filter(|c| !c.is_whitespace()) {
        *counts.entry(ch).or_insert(0usize) += 1;
    }
    let total: usize = counts.values().sum();
    let max = counts.values().copied().max().unwrap_or(0);
    // Divider if >=80% of non-space chars are the same punctuation char.
    total >= 4 && (max as f64 / total as f64) >= 0.8
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: every fixture below is SYNTHETIC. The pilots' real `.lzi` files are
    // already clean; this rule is preventive, so we must construct noise by hand.

    #[test]
    fn fires_on_decorative_divider() {
        let src = "# =================\nscreen Home {}\n";
        let f = scan_lzi_comment_noise(src);
        assert!(f.iter().any(|x| x.message.contains("decorative divider")));
    }

    #[test]
    fn fires_on_high_comment_ratio() {
        let src = "# c1\n# c2\n# c3\nscreen Home {}\n";
        let f = scan_lzi_comment_noise(src);
        assert!(f.iter().any(|x| x.line == 0 && x.message.contains("ratio")));
    }

    #[test]
    fn clean_lzi_is_silent() {
        // Representative of the (already-clean) pilot shape: signal-dense, no dividers.
        let src = "screen Home {\n  title \"Welcome\"\n}\n";
        let f = scan_lzi_comment_noise(src);
        assert!(f.is_empty());
    }

    #[test]
    fn divider_respects_allow_on_line() {
        let src = "# ========== # doctor:allow LZI-COMMENT-NOISE-001\nscreen Home {}\n";
        let f = scan_lzi_comment_noise(src);
        assert!(f.is_empty());
    }

    #[test]
    fn ratio_respects_allow_anywhere() {
        let src = "# c1\n# c2\n# c3\nscreen Home {}  # doctor:allow LZI-COMMENT-NOISE-001\n";
        let f = scan_lzi_comment_noise(src);
        assert!(f.is_empty());
    }
}
