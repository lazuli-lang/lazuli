//! `lazuli migrate dsl <FROM> <TO>` — apply recipe-driven DSL version
//! migrations.
//!
//! Recipes live as Markdown files under
//! `migrations/recipes/<from>-to-<to>/<NN>-<slug>.md`. The Markdown body
//! is human prose; the machine-readable contract is a YAML-ish
//! frontmatter block delimited by `---` lines. Frontmatter keys:
//!
//! - `name` (required): a unique recipe identifier used in logs and
//!   the rollback trace.
//! - `applies_to` (required): file-extension filter (`.lzi` or
//!   `.lzx`).
//! - `match` (required, block scalar `|`): a line-anchored marker
//!   pattern. Literal text matches itself; whitespace runs match
//!   themselves. `${name}` captures a non-whitespace token; the
//!   typed forms `${name:ws}` and `${name:rest}` capture a whitespace
//!   run or the rest of the line respectively.
//! - `replace` (required, block scalar `|`): the replacement
//!   template. Markers `${name}` refer back to slots captured by
//!   `match`. Unknown markers are an authoring error.
//! - `description` (optional): free text shown in the dry-run diff.
//!
//! Rationale for marker syntax (not regex): the CLI crate forbids
//! `regex` as a dependency. Markers cover the recipe shapes Lazuli's
//! own deprecation history requires (rename/move/strip-keyword) and
//! keep recipe authors honest about which slots carry meaning.
//!
//! ## Lifecycle
//!
//! 1. Load every recipe under `migrations/recipes/<from>-to-<to>/`
//!    sorted by filename (lexical order: `00-...`, `01-...`).
//! 2. Walk the project root for `.lzi`/`.lzx` files (skipping
//!    `dist/`, `target/`, `.git/`, `.lazuli/`, `node_modules/`).
//! 3. For each file, apply each recipe sequentially. Each recipe
//!    rewrites lines where the pattern matches.
//! 4. After all recipes apply, re-parse the rewritten file with
//!    `lazuli_syntax::parse_feature_skeletons` (`.lzi`) or
//!    `parse_lzx_document` (`.lzx`). On parse failure: revert the
//!    file's bytes to the pre-transform snapshot and surface the
//!    parse error in the report.
//! 5. On `--dry-run`, no file is written; the diff is printed
//!    instead.

use std::error::Error;
use std::path::{Path, PathBuf};

mod apply;
mod recipe;

use apply::{process_file, walk_lazuli_sources};
use recipe::load_recipe_dir;

#[cfg(test)]
use apply::{apply_recipe, match_line};
#[cfg(test)]
use recipe::{AppliesTo, PatternToken, parse_recipe};
#[cfg(test)]
use std::fs;

/// Outcome of a `lazuli migrate dsl` run.
#[derive(Debug, Default)]
pub struct DslReport {
    /// Files whose contents changed and were written to disk.
    pub changed: Vec<PathBuf>,
    /// Files that matched at least one recipe but were rolled back
    /// because the post-transform source failed to parse.
    pub rolled_back: Vec<(PathBuf, String)>,
    /// Files that would change in `--dry-run` mode (no disk write).
    pub dry_run_changes: Vec<DslDiff>,
    /// Recipes loaded for this transition. Useful for surfacing the
    /// no-op case (no recipes ⇒ tell the user where the directory
    /// should be).
    pub recipes_applied: Vec<String>,
}

/// One per-file diff entry surfaced in `--dry-run` mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DslDiff {
    pub file: PathBuf,
    pub before: String,
    pub after: String,
}

/// Run the DSL migration tool.
///
/// `from` / `to` are version tags like `v0.11` and `v0.12`. The tool
/// looks up `migrations/recipes/<from>-to-<to>/` and exits non-zero
/// when that directory is missing.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_cli::migrate::dsl::run_migrate_dsl;
///
/// // let report = run_migrate_dsl(Path::new("."), "v0.11", "v0.12", true)?;
/// ```
pub fn run_migrate_dsl(
    project_root: &Path,
    from: &str,
    to: &str,
    dry_run: bool,
) -> Result<DslReport, Box<dyn Error>> {
    let recipe_dir = project_root
        .join("migrations")
        .join("recipes")
        .join(format!("{from}-to-{to}"));
    if !recipe_dir.exists() {
        return Err(format!(
            "no DSL recipes for {from} → {to} at {}",
            recipe_dir.display()
        )
        .into());
    }

    let recipes = load_recipe_dir(&recipe_dir)?;
    if recipes.is_empty() {
        return Err(format!(
            "DSL recipe directory {} contains no .md recipes",
            recipe_dir.display()
        )
        .into());
    }

    let mut report = DslReport {
        recipes_applied: recipes.iter().map(|r| r.name.clone()).collect(),
        ..DslReport::default()
    };

    for file in walk_lazuli_sources(project_root)? {
        process_file(&file, &recipes, dry_run, &mut report)?;
    }

    Ok(report)
}

/// Public-facing summary report renderer. Used by the CLI driver.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_cli::migrate::dsl::{render_report, DslReport};
///
/// let text = render_report(&DslReport::default(), true);
/// assert!(text.contains("recipe"));
/// ```
pub fn render_report(report: &DslReport, dry_run: bool) -> String {
    let mut out = String::new();
    use std::fmt::Write;
    let _ = writeln!(
        out,
        "loaded {} recipe(s): {}",
        report.recipes_applied.len(),
        report.recipes_applied.join(", ")
    );
    if dry_run {
        if report.dry_run_changes.is_empty() {
            let _ = writeln!(out, "dry-run: no files would change");
        }
        for diff in &report.dry_run_changes {
            let _ = writeln!(out, "would change: {}", diff.file.display());
            for (a, b) in diff
                .before
                .lines()
                .zip(diff.after.lines())
                .filter(|(a, b)| a != b)
            {
                let _ = writeln!(out, "  - {a}");
                let _ = writeln!(out, "  + {b}");
            }
        }
    } else {
        if report.changed.is_empty() {
            let _ = writeln!(out, "no files changed");
        }
        for path in &report.changed {
            let _ = writeln!(out, "changed: {}", path.display());
        }
    }
    for (path, err) in &report.rolled_back {
        let _ = writeln!(out, "rolled back {}: {}", path.display(), err);
    }
    out
}

#[cfg(test)]
mod tests {
    include!("dsl_tests.rs");
}
