//! `lazuli fix` subcommand handler.
//!
//! Thin wrapper around the `lazuli_fix` crate so that the CLI and LSP share
//! the same fix-action logic (Wave 2.3 of the TDD/BDD-first proposal).

use std::path::Path;

use anyhow::{Context, Result, bail};

pub fn run_fix(rule: &str, path: &Path, line: usize, column: usize, apply: bool) -> Result<()> {
    let request = lazuli_fix::FixRequest {
        rule: rule.to_string(),
        path: path.to_path_buf(),
        line,
        column,
        apply,
    };
    let result = lazuli_fix::execute(&request)
        .with_context(|| format!("failed to execute fix for {rule}"))?;

    match result.outcome {
        lazuli_fix::FixOutcome::Applied => {
            println!("applied {} to {}", rule, path.display());
            if !result.preview.is_empty() {
                println!("--- preview ---");
                println!("{}", result.preview);
            }
            Ok(())
        }
        lazuli_fix::FixOutcome::Preview => {
            println!("preview (re-run with --apply to write):");
            println!("{}", result.preview);
            if let Some(note) = result.note {
                println!("note: {note}");
            }
            // Non-zero exit so agents notice they need to confirm.
            bail!("preview only — pass --apply to commit the change");
        }
        lazuli_fix::FixOutcome::NoChange => {
            println!("no change needed for {}", rule);
            if let Some(note) = result.note {
                println!("note: {note}");
            }
            Ok(())
        }
        lazuli_fix::FixOutcome::Skipped => {
            if let Some(note) = result.note {
                bail!("{note}");
            }
            bail!("fix skipped — no action registered for {rule}");
        }
    }
}
