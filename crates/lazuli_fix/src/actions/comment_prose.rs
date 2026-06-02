//! `LZI-COMMENT-PROSE-001` fix action — the MECHANICAL half of comment-discipline
//! (spec 0029).
//!
//! POINT-FIX (per the spec-0028/0029 grader): `lazuli fix` operates on ONE
//! finding at a time (`FixRequest { rule, path, line, column }`), so this action
//! handles the SINGLE `#` comment line at `request.line`. To clean a whole
//! pilot, the resolution harness invokes the per-site fix once per
//! `LZI-COMMENT-PROSE-001` finding.
//!
//! ## Mechanical vs manual (the honest split)
//!
//! This action ONLY touches cases where the right move is unambiguous and
//! lossless:
//!
//! - **delete** an empty / whitespace-only `#` line (no content);
//! - **delete** a pure decorative divider / box-draw banner (`# ====`,
//!   `# ──────`, `# ***`, …) — visual noise with no semantic content;
//! - **migrate** a legacy `# doctor:allow X [— reason "y"]` to the
//!   `@doctor.allow(X, reason: "y")` node (delegates to spec 0028's
//!   [`super::doctor_allow_comment`] migration — same rewrite, reused verbatim).
//!
//! For PROSE that carries meaning (a sentence, a section label, a gap note), the
//! correct home is a SEMANTIC relocation — rationale → `<feature>.ctx.md`, intent
//! → a `purpose`/`doc` field — which a codemod cannot safely guess. Those are
//! reported as [`crate::FixOutcome::Skipped`] with a `note` telling the caller it
//! is MANUAL, so the resolution fleet (an LLM with judgement) relocates them. The
//! action never guesses a semantic move.
//!
//! Idempotent: a line already deleted/migrated, or any non-comment line, is left
//! untouched.

use std::fs;

use anyhow::{Context, Result};

use lazuli_syntax::doctor_allow::recognize_legacy_comment_line;

use crate::actions::{FixAction, doctor_allow_comment::DoctorAllowCommentToNode};
use crate::{FixOutcome, FixRequest, FixResult};

/// Fix action for `LZI-COMMENT-PROSE-001` — deletes mechanical `#` comments
/// (empty / divider) and migrates legacy `# doctor:allow` waivers; reports
/// semantic prose as manual.
pub struct CommentProseFix;

const RULE_CODE: &str = "LZI-COMMENT-PROSE-001";

/// Decorative-ruler / box-draw glyphs that, when a comment body is made ENTIRELY
/// of them (plus whitespace), make the line a pure visual divider/banner with no
/// semantic content — safe to delete.
const DIVIDER_GLYPHS: &[char] = &[
    '-', '=', '*', '#', '/', '_', '~', '+', '.', '<', '>',
    // box-draw set (U+2500..) the grader flagged on `.lzx` banners.
    '\u{2500}', '\u{2501}', '\u{2502}', '\u{2503}', '\u{2550}', '\u{2551}', '\u{2554}', '\u{2557}',
    '\u{255a}', '\u{255d}', '\u{2560}', '\u{2563}', '\u{2566}', '\u{2569}', '\u{256c}', '\u{2574}',
    '\u{2576}', '\u{2578}', '\u{257a}', '\u{25a0}', '\u{25cf}',
];

impl FixAction for CommentProseFix {
    fn rule_code(&self) -> &'static str {
        RULE_CODE
    }

    fn execute(&self, request: &FixRequest) -> Result<FixResult> {
        if !request.path.exists() {
            return Ok(FixResult {
                outcome: FixOutcome::Skipped,
                preview: String::new(),
                note: Some(format!("file does not exist: {}", request.path.display())),
            });
        }

        // Read the target line first so we can decide which mechanical branch
        // (if any) applies before touching disk.
        let source = fs::read_to_string(&request.path)
            .with_context(|| format!("failed to read {}", request.path.display()))?;
        let lines: Vec<&str> = source.lines().collect();
        let target_idx = request.line.saturating_sub(1);
        let Some(&old_line) = lines.get(target_idx) else {
            return Ok(FixResult {
                outcome: FixOutcome::Skipped,
                preview: String::new(),
                note: Some(format!(
                    "line {} is out of range ({} lines)",
                    request.line,
                    lines.len()
                )),
            });
        };

        // Branch 1 — legacy `# doctor:allow X` → migrate to the node (reuse the
        // spec-0028 action verbatim; same point-fix contract).
        if recognize_legacy_comment_line(old_line).is_some() {
            return DoctorAllowCommentToNode.execute(request);
        }

        // Locate the `#` (full-line or inline) so we can classify the body.
        let Some((hash_byte, body)) = comment_body(old_line) else {
            return Ok(FixResult {
                outcome: FixOutcome::NoChange,
                preview: String::new(),
                note: Some(format!(
                    "line {} is not a `#` comment (already cleaned?)",
                    request.line
                )),
            });
        };

        let is_inline = !old_line[..hash_byte].trim().is_empty();
        let mechanical = is_empty_body(body) || is_divider_body(body);

        if !mechanical {
            // Semantic prose — do NOT guess a relocation. Report as manual.
            return Ok(FixResult {
                outcome: FixOutcome::Skipped,
                preview: String::new(),
                note: Some(format!(
                    "line {} is prose (`#{body}`) — MANUAL relocation required: move \
                     rationale to a `<feature>.ctx.md` context file, a construct's intent to \
                     its `purpose`/`doc`/`description` field, or a waiver to \
                     `@doctor.allow(CODE, reason: \"...\")`. The codemod will not guess a \
                     semantic move.",
                    request.line
                )),
            });
        }

        // Mechanical delete. For a full-line comment, drop the whole line. For an
        // inline comment, strip the trailing ` # ...` and keep the code prefix
        // (trimming the now-trailing whitespace).
        let (new_lines, preview) = if is_inline {
            let kept = old_line[..hash_byte].trim_end().to_string();
            let mut v: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            v[target_idx] = kept.clone();
            let p = format!(
                "--- {path}:{line}\n-{old}\n+{new}\n",
                path = request.path.display(),
                line = request.line,
                old = old_line,
                new = kept,
            );
            (v, p)
        } else {
            let mut v: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
            v.remove(target_idx);
            let p = format!(
                "--- {path}:{line}\n-{old}\n(line deleted)\n",
                path = request.path.display(),
                line = request.line,
                old = old_line,
            );
            (v, p)
        };

        if !request.apply {
            return Ok(FixResult {
                outcome: FixOutcome::Preview,
                preview,
                note: Some("pass --apply to write the change to disk".into()),
            });
        }

        let mut output = new_lines.join("\n");
        if source.ends_with('\n') {
            output.push('\n');
        }
        fs::write(&request.path, output)
            .with_context(|| format!("failed to write {}", request.path.display()))?;
        Ok(FixResult {
            outcome: FixOutcome::Applied,
            preview,
            note: None,
        })
    }
}

/// Find the `#` that starts a line-comment on `line` and return `(byte_offset,
/// body)` where `body` is everything after the `#`. Ignores a `#` inside a
/// string literal. `None` when the line has no comment.
fn comment_body(line: &str) -> Option<(usize, &str)> {
    let mut in_string = false;
    for (byte_idx, ch) in line.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '#' if !in_string => {
                return Some((byte_idx, &line[byte_idx + 1..]));
            }
            _ => {}
        }
    }
    None
}

/// True when a comment body is empty or whitespace-only.
fn is_empty_body(body: &str) -> bool {
    body.trim().is_empty()
}

/// True when a comment body is made ENTIRELY of decorative-ruler / box-draw
/// glyphs (plus whitespace) and has at least one such glyph — a pure visual
/// divider/banner with no words. A body containing ANY letter or digit is NOT a
/// divider (it carries content → manual).
fn is_divider_body(body: &str) -> bool {
    let mut saw_glyph = false;
    for ch in body.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if DIVIDER_GLYPHS.contains(&ch) {
            saw_glyph = true;
        } else {
            return false;
        }
    }
    saw_glyph
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn write_temp(body: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("feature.lzi");
        std::fs::File::create(&path)
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        (dir, path)
    }

    fn req(path: &PathBuf, line: usize, apply: bool) -> FixRequest {
        FixRequest {
            rule: RULE_CODE.into(),
            path: path.clone(),
            line,
            column: 1,
            apply,
        }
    }

    #[test]
    fn rule_code_is_canonical() {
        assert_eq!(CommentProseFix.rule_code(), "LZI-COMMENT-PROSE-001");
    }

    #[test]
    fn deletes_empty_hash_line() {
        let (_d, path) = write_temp("feature billing\n  # \n  command create\n");
        let res = CommentProseFix.execute(&req(&path, 2, true)).unwrap();
        assert_eq!(res.outcome, FixOutcome::Applied);
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "feature billing\n  command create\n");
    }

    #[test]
    fn deletes_ascii_divider() {
        let (_d, path) = write_temp("# ================\nfeature billing\n");
        let res = CommentProseFix.execute(&req(&path, 1, true)).unwrap();
        assert_eq!(res.outcome, FixOutcome::Applied);
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "feature billing\n");
    }

    #[test]
    fn deletes_box_draw_banner_with_only_glyphs() {
        let (_d, path) = write_temp("# ──────────\nexperience customer\n");
        let res = CommentProseFix.execute(&req(&path, 1, true)).unwrap();
        assert_eq!(res.outcome, FixOutcome::Applied);
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "experience customer\n");
    }

    #[test]
    fn keeps_real_prose_as_manual() {
        let (_d, path) = write_temp("# This resource stores the billing address.\nfeature x\n");
        let res = CommentProseFix.execute(&req(&path, 1, true)).unwrap();
        assert_eq!(res.outcome, FixOutcome::Skipped);
        assert!(res.note.unwrap().contains("MANUAL"));
        // File untouched.
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.starts_with("# This resource stores"));
    }

    #[test]
    fn keeps_section_label_as_manual() {
        // A box banner WITH words (`# ── Public ──`) carries content → manual,
        // not a mechanical delete.
        let (_d, path) = write_temp("# ── Public (unauthenticated) ──\nexperience x\n");
        let res = CommentProseFix.execute(&req(&path, 1, true)).unwrap();
        assert_eq!(res.outcome, FixOutcome::Skipped);
        assert!(res.note.unwrap().contains("MANUAL"));
    }

    #[test]
    fn migrates_legacy_doctor_allow() {
        let (_d, path) =
            write_temp("# doctor:allow LZI-FILE-SIZE-001 — reason \"gen\"\nfeature billing\n");
        let res = CommentProseFix.execute(&req(&path, 1, true)).unwrap();
        assert_eq!(res.outcome, FixOutcome::Applied);
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("@doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")"),
            "got: {after}"
        );
        assert!(!after.contains("# doctor:allow"));
    }

    #[test]
    fn strips_inline_comment_keeps_code() {
        let (_d, path) = write_temp("feature billing  # ====\n");
        let res = CommentProseFix.execute(&req(&path, 1, true)).unwrap();
        assert_eq!(res.outcome, FixOutcome::Applied);
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "feature billing\n");
    }

    #[test]
    fn idempotent_on_clean_line() {
        let (_d, path) = write_temp("feature billing\n");
        let res = CommentProseFix.execute(&req(&path, 1, true)).unwrap();
        assert_eq!(res.outcome, FixOutcome::NoChange);
        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, "feature billing\n");
    }

    #[test]
    fn preview_does_not_write() {
        let (_d, path) = write_temp("# ====\nfeature x\n");
        let res = CommentProseFix.execute(&req(&path, 1, false)).unwrap();
        assert_eq!(res.outcome, FixOutcome::Preview);
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("# ===="), "preview must not write");
    }

    #[test]
    fn divider_with_letter_is_not_mechanical() {
        assert!(!is_divider_body(" ==a=="));
        assert!(is_divider_body(" ======"));
        assert!(is_divider_body(" ── ──"));
        assert!(!is_divider_body(" hello"));
        assert!(!is_divider_body("")); // empty is is_empty_body's job, not divider
    }
}
