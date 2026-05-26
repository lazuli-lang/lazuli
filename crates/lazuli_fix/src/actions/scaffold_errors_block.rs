//! `ERROR-VOCAB-MISSING-001` fix action — append a stub `errors` block
//! to a feature.
//!
//! Proof-of-concept extraction. The LSP currently builds this block via
//! `build_scaffold_errors_action` in `lazuli_lsp::lib`. Once the LSP
//! refactor lands (follow-up cell), that helper becomes a thin wrapper
//! over this action.

use std::fs;

use anyhow::{Context, Result};

use crate::actions::FixAction;
use crate::{FixOutcome, FixRequest, FixResult};

/// Fix action that appends a stub `errors` block to a feature
/// missing its error vocabulary.
///
/// Idempotent — re-running on a feature that already declares an
/// `errors` block (anywhere in source) returns
/// [`FixOutcome::NoChange`]. The stub seeds three placeholder codes
/// (`not_found`, `invalid_request`, `forbidden`) plus a `@TODO`
/// reminding the author to land the canonical twelve.
pub struct ScaffoldErrorsBlock;

const RULE_CODE: &str = "ERROR-VOCAB-MISSING-001";

impl FixAction for ScaffoldErrorsBlock {
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
        let anchor_idx = request
            .line
            .saturating_sub(1)
            .min(lines.len().saturating_sub(1));
        let feature_line = lines.get(anchor_idx).copied().unwrap_or("");
        let feature_name = feature_line
            .trim_start()
            .strip_prefix("feature ")
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or("unknown");

        if has_errors_block(&source) {
            return Ok(FixResult {
                outcome: FixOutcome::NoChange,
                preview: String::new(),
                note: Some(format!(
                    "feature `{feature_name}` already has an errors block",
                )),
            });
        }

        let stub = format!(
            "  errors\n    # @TODO authored: choose the 12 stable codes for `{feature_name}`\n    not_found\n    invalid_request\n    forbidden\n",
        );

        let preview = format!(
            "--- {path}:{line}\n+++ scaffold errors block in feature `{feature_name}`\n{stub}",
            path = request.path.display(),
            line = request.line,
        );

        if !request.apply {
            return Ok(FixResult {
                outcome: FixOutcome::Preview,
                preview,
                note: Some("pass --apply to write the change to disk".into()),
            });
        }

        // Append the stub after the feature header line.
        let mut output = String::with_capacity(source.len() + stub.len());
        for (idx, line) in lines.iter().enumerate() {
            output.push_str(line);
            output.push('\n');
            if idx == anchor_idx {
                output.push_str(&stub);
            }
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

fn has_errors_block(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed == "errors" || trimmed.starts_with("errors ")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_code_is_canonical() {
        assert_eq!(ScaffoldErrorsBlock.rule_code(), "ERROR-VOCAB-MISSING-001");
    }
}
