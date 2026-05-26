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
///
/// Resolves the project root from the cwd and delegates to
/// [`seed::run_seed`] with the optional `--only` pattern and `--force`
/// flag.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::commands::seed::seed_command;
///
/// // seed_command(Some("orders_*"), false)?;
/// ```
pub fn seed_command(only: Option<&str>, force: bool) -> Result<()> {
    let project_root = std::env::current_dir().context("failed to determine current directory")?;
    seed::run_seed(&project_root, only, force).map_err(|err| anyhow::anyhow!("{err}"))
}

#[cfg(test)]
mod tests {
    use super::seed_command;

    #[test]
    fn seed_command_is_callable() {
        // Smoke: ensure the public symbol stays addressable. We do not
        // run it because it requires a live database connection.
        let _ = seed_command;
    }
}
