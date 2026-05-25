//! `lazuli debug` — turn a runtime error envelope (the wire shape
//! emitted by the Lazuli Go runtime on panic / typed-error fallout)
//! into a human or agent-readable triage bundle.
//!
//! Input is a JSON `ErrorEnvelopeInput` arriving on `stdin` or via the
//! `--error <path>` flag. The handler optionally overrides
//! `capsule` from the `--capsule` flag (useful when an envelope was
//! captured without an explicit capsule field), then calls
//! `debug::run_debug` which traces the offending command/query through
//! IR + manifest context and returns a `DebugBundle`.
//!
//! Output: `--format json` for agents (pretty `serde_json`),
//! `--format markdown` for humans (a `debug::format_markdown` render).
//! Any other value is rejected with a typed error rather than silently
//! defaulting — debug surfaces are agent-facing and silent fallbacks
//! poison the contract.
//!
//! Cross-refs:
//! - `crate::debug::ErrorEnvelopeInput` — the wire shape.
//! - `crate::debug::run_debug` — the actual triage.
//! - `crate::debug::format_markdown` — the markdown projection.

use std::fs;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::debug;

/// Handler for the `Commands::Debug` clap arm.
pub fn debug_command(
    project_root: &Path,
    error_path: Option<&Path>,
    capsule: Option<String>,
    format: &str,
) -> Result<()> {
    let input = match error_path {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("failed to read error envelope {}", path.display()))?,
        None => {
            let mut input = String::new();
            std::io::stdin()
                .read_to_string(&mut input)
                .context("failed to read error envelope from stdin")?;
            input
        }
    };
    let mut envelope: debug::ErrorEnvelopeInput =
        serde_json::from_str(&input).context("failed to parse error envelope JSON")?;
    if let Some(capsule) = capsule {
        envelope.capsule = capsule;
    }

    let bundle =
        debug::run_debug(project_root, envelope).map_err(|err| anyhow::anyhow!("{err}"))?;
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&bundle)?),
        "markdown" => print!("{}", debug::format_markdown(&bundle)),
        other => bail!("unsupported debug format `{other}`; expected json or markdown"),
    }

    Ok(())
}
