//! `lazuli examples bundle|validate` — operate on the project's
//! `examples/` directory as a single canonical IR snapshot bundle.
//!
//! `bundle` walks `examples/`, lifts each entry to IR, and emits a
//! single JSON artifact (path defaults to project root or honors
//! `--out <path>`). The bundle is the wire format the docs site, the
//! grade rubric, and external auditors consume — one file instead of
//! N directory traversals.
//!
//! `validate` runs the same lift pass and additionally checks the
//! existing bundle for IR-shape drift against the live examples. With
//! `--check-decay`, the handler also flags examples whose IR shape
//! looks structurally degraded (missing required slots, drifted
//! anchor coverage) — the catch for examples falling out of date with
//! the language proper.
//!
//! Cross-refs:
//! - `crate::examples_bundle::{run_examples_bundle,
//!   run_examples_validate}` — the actual walkers.

use std::env;
use std::path::Path;

use anyhow::{Context, Result};

use crate::examples_bundle;

/// Handler for `ExamplesCommand::Bundle`.
///
/// Resolves the project root from the current working directory and
/// delegates to [`examples_bundle::run_examples_bundle`], threading the
/// optional `--out` override.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::commands::examples::bundle_command;
///
/// // bundle_command(None)?;   // writes the canonical bundle file
/// ```
pub fn bundle_command(out: Option<&Path>) -> Result<()> {
    let project_root = env::current_dir().context("failed to determine current directory")?;
    examples_bundle::run_examples_bundle(&project_root, out).map_err(|err| anyhow::anyhow!("{err}"))
}

/// Handler for `ExamplesCommand::Validate`.
///
/// Re-lifts every example in `examples/` and compares against the
/// committed bundle. `check_decay` toggles the additional structural
/// degradation pass that flags examples drifting from current grammar.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::commands::examples::validate_command;
///
/// // validate_command(true)?;
/// ```
pub fn validate_command(check_decay: bool) -> Result<()> {
    let project_root = env::current_dir().context("failed to determine current directory")?;
    examples_bundle::run_examples_validate(&project_root, check_decay)
        .map_err(|err| anyhow::anyhow!("{err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_in_blank_dir_errors_or_ok_does_not_panic() {
        // Smoke: handler should not panic; in a workspace without
        // examples/ the lift may either succeed (empty) or surface a
        // typed error — both are acceptable.
        let _ = validate_command(false);
    }
}
