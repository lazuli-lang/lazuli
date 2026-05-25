//! `lazuli seed` — populate a project's local database from typed
//! `seed` blocks declared in `.lzi` source.
//!
//! Each `seed <name> { ... }` block is a deterministic, idempotent
//! insert plan: re-running the command does not duplicate rows
//! unless `--force` is passed (which truncates the target tables
//! first). `--only <pattern>` restricts the run to seed names
//! matching the pattern, useful for incremental authoring loops.
//!
//! The handler delegates to `crate::seed::run_seed`, which owns the
//! actual SQL execution and idempotency contract. This module is the
//! thin clap wrapper.
//!
//! Cross-refs:
//! - `crate::seed::run_seed` — the actual seeder.

use anyhow::{Context, Result};

use crate::seed;

/// Handler for the `Commands::Seed` clap arm.
pub fn seed_command(only: Option<&str>, force: bool) -> Result<()> {
    let project_root =
        std::env::current_dir().context("failed to determine current directory")?;
    seed::run_seed(&project_root, only, force).map_err(|err| anyhow::anyhow!("{err}"))
}
