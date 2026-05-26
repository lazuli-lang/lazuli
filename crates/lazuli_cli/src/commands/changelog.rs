//! `lazuli changelog --from <a.json> --to <b.json>` — OpenAPI bucket
//! cycle changelog generator.
//!
//! Reads two `lazuli inspect --format=json` payloads (the canonical
//! IR-of-record across revisions), diffs them via
//! `lazuli_changelog::diff`, and renders a Markdown report covering
//! added / removed / deprecated / breaking / non-breaking operations.
//! Writes to `--output <path>` when present; otherwise prints to
//! stdout.
//!
//! The diff is intentionally JSON-only: the changelog crate works on
//! `lazuli_ir::Module` shapes, so callers have to first run
//! `lazuli inspect --format=json` against each revision (usually
//! `--from <baseline-rev>` and `--to <head-rev>` checkouts).
//!
//! Cross-refs:
//! - `lazuli_changelog::diff` / `lazuli_changelog::render_markdown` —
//!   the actual diff engine.
//! - `commands/inspect.rs` (TBD) — the producer of the inputs.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Handler for the `Commands::Changelog` clap arm.
///
/// Reads two `inspect --format=json` payloads, runs them through
/// `lazuli_changelog::diff` + `render_markdown`, and either writes the
/// result to `output` (creating its parent directory) or prints to
/// stdout when `output` is `None`.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::changelog::changelog_command;
///
/// // changelog_command(Path::new("v1.json"), Path::new("v2.json"), None)?;
/// ```
pub fn changelog_command(from: &Path, to: &Path, output: Option<&Path>) -> Result<()> {
    let old_text =
        fs::read_to_string(from).with_context(|| format!("reading {}", from.display()))?;
    let new_text = fs::read_to_string(to).with_context(|| format!("reading {}", to.display()))?;
    let old_module: lazuli_ir::Module = serde_json::from_str(&old_text)
        .with_context(|| format!("parsing {} as IR JSON", from.display()))?;
    let new_module: lazuli_ir::Module = serde_json::from_str(&new_text)
        .with_context(|| format!("parsing {} as IR JSON", to.display()))?;
    let report = lazuli_changelog::diff(&old_module, &new_module);
    let md = lazuli_changelog::render_markdown(&report);
    match output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("creating output directory {}", parent.display())
                    })?;
                }
            }
            fs::write(path, &md)
                .with_context(|| format!("writing changelog to {}", path.display()))?;
            println!("wrote {}", path.display());
        }
        None => print!("{}", md),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_from_file_errors() {
        let result = changelog_command(
            Path::new("__lazuli_nonexistent_from.json"),
            Path::new("__lazuli_nonexistent_to.json"),
            None,
        );
        assert!(result.is_err(), "expected read failure for missing input");
    }
}
