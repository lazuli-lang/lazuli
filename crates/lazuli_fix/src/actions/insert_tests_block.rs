//! `TEST-MISSING-AUTHORED-001` fix action — append a stub `tests` block
//! to the construct on the requested anchor line.
//!
//! This action is the canonical proof-of-concept for the Wave 2.3 crate
//! extraction: the same logic powers the LSP `Insert tests block`
//! quick-fix once the LSP `code_action` handler is refactored to call
//! `lazuli_fix::execute`.

use std::fs;

use anyhow::{Context, Result};

use crate::actions::FixAction;
use crate::{FixOutcome, FixRequest, FixResult};

/// Fix action that appends a stub `tests` sub-block beneath the
/// flagged construct.
///
/// Idempotent — re-running on a construct that already has a `tests`
/// child returns [`FixOutcome::NoChange`]. Indentation follows the
/// construct's leading-space depth + 2 (matches the DSL formatter).
pub struct InsertTestsBlock;

const RULE_CODE: &str = "TEST-MISSING-AUTHORED-001";

impl FixAction for InsertTestsBlock {
    fn rule_code(&self) -> &'static str {
        RULE_CODE
    }

    fn execute(&self, request: &FixRequest) -> Result<FixResult> {
        if !request.path.exists() {
            return Ok(FixResult {
                outcome: FixOutcome::Skipped,
                preview: String::new(),
                note: Some(format!(
                    "file does not exist: {}",
                    request.path.display()
                )),
            });
        }
        let source = fs::read_to_string(&request.path)
            .with_context(|| format!("failed to read {}", request.path.display()))?;
        let lines: Vec<&str> = source.lines().collect();

        // The doctor finding anchors at the construct header (e.g.
        // `command create`). The fix inserts a `tests` sub-block
        // immediately after the construct body. Find the end of the
        // construct: the next line whose indent is `<= construct indent`.
        let anchor_idx = request.line.saturating_sub(1).min(lines.len().saturating_sub(1));
        let anchor_line = lines.get(anchor_idx).copied().unwrap_or("");
        let construct_indent = leading_spaces(anchor_line);
        let child_indent = construct_indent + 2;
        let indent_str = " ".repeat(child_indent);

        // Idempotency: if the construct already has a `tests` sub-block,
        // skip.
        let mut insertion_idx = lines.len();
        for (offset, line) in lines.iter().enumerate().skip(anchor_idx + 1) {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let indent = leading_spaces(line);
            if indent <= construct_indent {
                insertion_idx = offset;
                break;
            }
            if indent == child_indent
                && (trimmed == "tests" || trimmed.starts_with("tests "))
            {
                return Ok(FixResult {
                    outcome: FixOutcome::NoChange,
                    preview: String::new(),
                    note: Some(
                        "construct already has a tests block; nothing to do".into(),
                    ),
                });
            }
        }

        let stub = format!(
            "{indent}tests\n{indent}  # @TODO authored: cover policy + requires-predicate\n{indent}  allows as @role.editor\n{indent}  denies as @role.viewer\n",
            indent = indent_str
        );

        let preview = format!(
            "--- {path}:{line}\n+++ inserted tests block\n{stub}",
            path = request.path.display(),
            line = request.line,
            stub = stub
        );

        if !request.apply {
            return Ok(FixResult {
                outcome: FixOutcome::Preview,
                preview,
                note: Some("pass --apply to write the change to disk".into()),
            });
        }

        // Apply: rebuild source with the stub spliced in at `insertion_idx`.
        let mut output = String::with_capacity(source.len() + stub.len());
        for (idx, line) in lines.iter().enumerate() {
            if idx == insertion_idx {
                output.push_str(&stub);
            }
            output.push_str(line);
            output.push('\n');
        }
        if insertion_idx >= lines.len() {
            output.push_str(&stub);
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

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ').count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_code_is_canonical() {
        assert_eq!(InsertTestsBlock.rule_code(), "TEST-MISSING-AUTHORED-001");
    }

    #[test]
    fn skips_missing_file() {
        let action = InsertTestsBlock;
        let request = FixRequest {
            rule: RULE_CODE.to_string(),
            path: std::path::PathBuf::from("/this/does/not/exist.lzi"),
            line: 1,
            column: 1,
            apply: false,
        };
        let result = action.execute(&request).expect("should not error");
        assert!(matches!(result.outcome, FixOutcome::Skipped));
    }
}
