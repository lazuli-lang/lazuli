//! `lazuli upgrade` — apply typed upgrade recipes that rewrite the
//! project's source tree between two Lazuli versions.
//!
//! Distinct from `lazuli migrate dsl`: `migrate dsl` rewrites DSL
//! grammar between language versions (e.g. `route id: ID` → `id: ID`);
//! `upgrade` rewrites every other project artifact a Lazuli version
//! bump touches (Lazurite manifest pins, generated-file conventions,
//! `dist/` layout changes, etc.). The two are sibling surfaces sharing
//! the recipe pattern.
//!
//! `--from <ver> --to <ver>` selects the recipe set;
//! `--target <subdir>` scopes the rewrite to a project sub-tree;
//! `--dry-run` previews without writing. The handler exits non-zero if
//! any recipe fails so CI can gate version bumps.
//!
//! Cross-refs:
//! - `crate::upgrade::run_upgrade` — the recipe loader + applier.
//! - `commands/migrate.rs::dsl_command` — the sibling DSL-grammar
//!   recipe surface.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::upgrade;

/// Handler for the `Commands::Upgrade` clap arm.
///
/// Resolves the project root from the cwd, runs every recipe between
/// `from` and `to` scoped to `target` (with `dry_run` preview), prints
/// applied/failed lines, and `bail!`s when any recipe failed so CI can
/// gate version bumps.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::commands::upgrade::upgrade_command;
///
/// // upgrade_command("1.0", "1.1", Path::new("."), true)?;
/// ```
pub fn upgrade_command(from: &str, to: &str, target: &Path, dry_run: bool) -> Result<()> {
    let project_root = std::env::current_dir().context("reading current directory")?;
    let report = upgrade::run_upgrade(&project_root, from, to, target, dry_run)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    for recipe in &report.applied {
        println!("applied {}", recipe.display());
    }
    for (recipe, error) in &report.failed {
        println!("failed {}: {}", recipe.display(), error);
    }
    if !report.failed.is_empty() {
        bail!("lazuli upgrade failed");
    }
    println!("lazuli upgrade applied {} recipe(s)", report.applied.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::upgrade_command;

    // Smoke: upgrade_command needs project state we don't synthesize
    // here; we just guard the public symbol.
    #[test]
    fn upgrade_command_is_callable() {
        let _ = upgrade_command;
    }
}
