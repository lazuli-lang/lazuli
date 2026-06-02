//! `DOCTOR-ALLOW-LEGACY-COMMENT-001` fix action — rewrite a legacy
//! `# doctor:allow <CODE> [— reason "..."]` comment into the first-class
//! `@doctor.allow(<CODE>, reason: "...")` node (spec 0028).
//!
//! POINT-FIX (per the spec-0028 grader): `lazuli fix` operates on ONE finding
//! at a time (`FixRequest { rule, path, line, column }`), so this action
//! rewrites the SINGLE legacy comment line at `request.line`. To migrate a whole
//! pilot, the harness invokes the per-site fix once per
//! `DOCTOR-ALLOW-LEGACY-COMMENT-001` finding. A bulk driver is out of scope for
//! v1 (the CLI has no whole-tree `--all` mode).
//!
//! Idempotent: a line already in node form, or any non-legacy-comment line, is
//! left untouched ([`crate::FixOutcome::NoChange`]).

use std::fs;

use anyhow::{Context, Result};

use lazuli_syntax::doctor_allow::{is_node_line, recognize_legacy_comment_line};

use crate::actions::FixAction;
use crate::{FixOutcome, FixRequest, FixResult};

/// Fix action that rewrites one legacy `# doctor:allow` comment to the
/// `@doctor.allow(...)` node form, preserving the line's leading indentation.
pub struct DoctorAllowCommentToNode;

const RULE_CODE: &str = "DOCTOR-ALLOW-LEGACY-COMMENT-001";

impl FixAction for DoctorAllowCommentToNode {
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

        let Some((code, reason)) = recognize_legacy_comment_line(old_line) else {
            return Ok(FixResult {
                outcome: FixOutcome::NoChange,
                preview: String::new(),
                note: Some(format!(
                    "line {} is not a legacy `# doctor:allow` comment (already migrated?)",
                    request.line
                )),
            });
        };

        // Indent the node to its ENCLOSING construct's scope, NOT the legacy
        // comment's own column. Spec 0028 Bug 3: a `# doctor:allow` comment is
        // indentation-insensitive trivia, so authors routinely wrote it at
        // col 0 even INSIDE a `feature`. Emitting the node at that same col 0
        // dedents out of the feature block and silently orphans every following
        // construct. The waiver attaches to the next real (non-trivia,
        // non-waiver) construct line, so we adopt THAT line's indentation when
        // it is deeper than the comment's own — otherwise keep the comment's
        // indent (a genuinely file-level / trailing waiver stays where it was).
        let own_indent_len = old_line.chars().take_while(|c| *c == ' ').count();
        let enclosing_indent_len = next_construct_indent(&lines, target_idx + 1);
        let indent_len = match enclosing_indent_len {
            Some(next) if next > own_indent_len => next,
            _ => own_indent_len,
        };
        let indent: String = " ".repeat(indent_len);
        let new_line = match &reason {
            Some(r) => format!("{indent}@doctor.allow({code}, reason: \"{r}\")"),
            None => format!("{indent}@doctor.allow({code})"),
        };

        let preview = format!(
            "--- {path}:{line}\n-{old}\n+{new}\n",
            path = request.path.display(),
            line = request.line,
            old = old_line,
            new = new_line,
        );

        if !request.apply {
            return Ok(FixResult {
                outcome: FixOutcome::Preview,
                preview,
                note: Some("pass --apply to write the change to disk".into()),
            });
        }

        // Rewrite ONLY the target line, preserving the file's trailing newline
        // shape (write the same line count back, joined by `\n`).
        let mut rewritten: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        rewritten[target_idx] = new_line;
        let mut output = rewritten.join("\n");
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

/// Leading-space count of the next non-trivia, non-waiver line at or after
/// `from` (0-based index into `lines`). `None` when none follows. The waiver
/// being migrated attaches to this construct, so the migrated node should match
/// its indentation (see Bug 3 in [`DoctorAllowCommentToNode::execute`]).
fn next_construct_indent(lines: &[&str], from: usize) -> Option<usize> {
    lines.iter().skip(from).find_map(|line| {
        let trimmed = line.trim_start();
        // Skip blank lines, comments (legacy trivia), and stacked waiver nodes.
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || is_node_line(trimmed)
            || recognize_legacy_comment_line(line).is_some()
        {
            None
        } else {
            Some(line.chars().take_while(|c| *c == ' ').count())
        }
    })
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

    #[test]
    fn rule_code_is_canonical() {
        assert_eq!(
            DoctorAllowCommentToNode.rule_code(),
            "DOCTOR-ALLOW-LEGACY-COMMENT-001"
        );
    }

    #[test]
    fn rewrites_comment_to_node() {
        let (_dir, path) = write_temp(
            "# doctor:allow LZI-FILE-SIZE-001 — reason \"generated table\"\nfeature billing\n",
        );
        let req = FixRequest {
            rule: RULE_CODE.into(),
            path: path.clone(),
            line: 1,
            column: 1,
            apply: true,
        };
        let res = DoctorAllowCommentToNode.execute(&req).unwrap();
        assert_eq!(res.outcome, FixOutcome::Applied);
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("@doctor.allow(LZI-FILE-SIZE-001, reason: \"generated table\")"),
            "got: {after}"
        );
        assert!(!after.contains("# doctor:allow"));
        assert!(after.contains("feature billing"));
    }

    #[test]
    fn preserves_indentation() {
        let (_dir, path) = write_temp("feature x\n  # doctor:allow Y-2\n  command create\n");
        let req = FixRequest {
            rule: RULE_CODE.into(),
            path: path.clone(),
            line: 2,
            column: 1,
            apply: true,
        };
        DoctorAllowCommentToNode.execute(&req).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("  @doctor.allow(Y-2)"), "got: {after}");
    }

    #[test]
    fn col0_comment_inside_feature_indents_to_enclosing_construct() {
        // Spec 0028 Bug 3b: a legacy comment written at col 0 INSIDE a feature
        // (legacy comments are indentation-insensitive) must NOT migrate to a
        // col-0 node — that would dedent out of the feature. The node adopts the
        // following construct's indentation instead.
        let (_dir, path) = write_temp(
            "feature billing\n# doctor:allow LZI-FILE-SIZE-001 — reason \"gen\"\n  command create\n",
        );
        let req = FixRequest {
            rule: RULE_CODE.into(),
            path: path.clone(),
            line: 2,
            column: 1,
            apply: true,
        };
        DoctorAllowCommentToNode.execute(&req).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("  @doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")"),
            "node must be indented to the enclosing construct (2 spaces), got: {after}"
        );
        // And it must NOT land at col 0.
        assert!(
            !after.contains("\n@doctor.allow("),
            "node must not be at col 0 inside the feature, got: {after}"
        );
    }

    #[test]
    fn col0_file_level_comment_stays_col0() {
        // A genuinely file-level waiver (next construct is also col 0) keeps its
        // col-0 placement — we only deepen, never dedent.
        let (_dir, path) =
            write_temp("# doctor:allow LZI-FILE-SIZE-001 — reason \"gen\"\nfeature billing\n");
        let req = FixRequest {
            rule: RULE_CODE.into(),
            path: path.clone(),
            line: 1,
            column: 1,
            apply: true,
        };
        DoctorAllowCommentToNode.execute(&req).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.starts_with("@doctor.allow(LZI-FILE-SIZE-001, reason: \"gen\")"),
            "file-level waiver stays at col 0, got: {after}"
        );
    }

    #[test]
    fn idempotent_on_node_form() {
        let (_dir, path) = write_temp("@doctor.allow(X-1, reason: \"ok\")\nfeature x\n");
        let req = FixRequest {
            rule: RULE_CODE.into(),
            path: path.clone(),
            line: 1,
            column: 1,
            apply: true,
        };
        let res = DoctorAllowCommentToNode.execute(&req).unwrap();
        assert_eq!(res.outcome, FixOutcome::NoChange);
        // File unchanged.
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.starts_with("@doctor.allow(X-1, reason: \"ok\")"));
    }

    #[test]
    fn preview_does_not_write() {
        let (_dir, path) = write_temp("# doctor:allow Z-9\n");
        let req = FixRequest {
            rule: RULE_CODE.into(),
            path: path.clone(),
            line: 1,
            column: 1,
            apply: false,
        };
        let res = DoctorAllowCommentToNode.execute(&req).unwrap();
        assert_eq!(res.outcome, FixOutcome::Preview);
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("# doctor:allow Z-9"), "preview must not write");
    }
}
