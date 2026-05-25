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
pub fn bundle_command(out: Option<&Path>) -> Result<()> {
    let project_root = env::current_dir().context("failed to determine current directory")?;
    examples_bundle::run_examples_bundle(&project_root, out).map_err(|err| anyhow::anyhow!("{err}"))
}

/// Handler for `ExamplesCommand::Validate`.
pub fn validate_command(check_decay: bool) -> Result<()> {
    let project_root = env::current_dir().context("failed to determine current directory")?;
    examples_bundle::run_examples_validate(&project_root, check_decay)
        .map_err(|err| anyhow::anyhow!("{err}"))
}
