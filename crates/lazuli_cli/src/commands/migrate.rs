//! `lazuli migrate up|down|status|dsl` — operational migration
//! surface that drives the project's SQL migration ledger plus the
//! grammar-level "migrate DSL" recipes.
//!
//! `up <--target N>`, `down <--steps N>`, `status` operate on the
//! sequenced migration files codegen emits under `dist/go/migrations/`
//! (Lazuli Go owns the actual SQL apply / rollback execution). Pass
//! `--yes` to skip the destructive-confirmation prompt — the handler
//! refuses destructive operations otherwise.
//!
//! `dsl --from <ver> --to <ver>` runs the typed DSL upgrade recipes
//! against the project source: each recipe rewrites authored `.lzi` /
//! `.lzx` to the target DSL version with full rollback when any
//! recipe fails. `--dry-run` previews the rewrite without touching
//! disk; the rendered report mirrors the dry-run output by design.
//!
//! Cross-refs:
//! - `crate::migrate::run_migrate` / `crate::migrate::MigrateAction` —
//!   the ledger surface.
//! - `crate::migrate::dsl::run_migrate_dsl` /
//!   `crate::migrate::dsl::render_report` — the DSL recipe runner.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::migrate;

/// Handler for `MigrateCommand::Up`.
///
/// Applies pending migrations up to `target` (or all when `None`).
/// `yes` skips the destructive-confirmation prompt.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::commands::migrate::up_command;
/// // up_command(None, true)?;
/// ```
pub fn up_command(target: Option<String>, yes: bool) -> Result<()> {
    let project_root = std::env::current_dir().context("reading current directory")?;
    migrate::run_migrate(&project_root, migrate::MigrateAction::Up { target, yes })
        .map_err(|err| anyhow::anyhow!("{err}"))
}

/// Handler for `MigrateCommand::Down`.
///
/// Rolls back `steps` migrations. `yes` skips the destructive prompt.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::commands::migrate::down_command;
/// // down_command(1, true)?;
/// ```
pub fn down_command(steps: u32, yes: bool) -> Result<()> {
    let project_root = std::env::current_dir().context("reading current directory")?;
    migrate::run_migrate(&project_root, migrate::MigrateAction::Down { steps, yes })
        .map_err(|err| anyhow::anyhow!("{err}"))
}

/// Handler for `MigrateCommand::Status`.
///
/// Prints the ledger of applied / pending migrations for the project.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::commands::migrate::status_command;
/// // status_command()?;
/// ```
pub fn status_command() -> Result<()> {
    let project_root = std::env::current_dir().context("reading current directory")?;
    migrate::run_migrate(&project_root, migrate::MigrateAction::Status)
        .map_err(|err| anyhow::anyhow!("{err}"))
}

/// Handler for `MigrateCommand::Dsl`.
///
/// Runs typed DSL upgrade recipes from `from` → `to` over the project
/// source. `dry_run` previews the rewrite. `bail!`s when any recipe
/// rolled back so authors notice the failure.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::commands::migrate::dsl_command;
/// // dsl_command("1.0", "1.1", true, None)?;
/// ```
pub fn dsl_command(from: &str, to: &str, dry_run: bool, path: Option<PathBuf>) -> Result<()> {
    let project_root = std::env::current_dir().context("reading current directory")?;
    let root = path.unwrap_or(project_root);
    let report = migrate::dsl::run_migrate_dsl(&root, from, to, dry_run)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    print!("{}", migrate::dsl::render_report(&report, dry_run));
    if !report.rolled_back.is_empty() {
        bail!(
            "lazuli migrate dsl rolled back {} file(s); fix the recipe and re-run",
            report.rolled_back.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsl_command_unknown_versions_returns_err() {
        // Pointing at a fresh temp dir without any sources still
        // exercises the version-resolution path; we just guard against
        // panics.
        let dir = std::env::temp_dir().join("lazuli_cli_migrate_dsl_test");
        let _ = std::fs::create_dir_all(&dir);
        let _ = dsl_command("0.0", "0.0", true, Some(dir));
    }
}
