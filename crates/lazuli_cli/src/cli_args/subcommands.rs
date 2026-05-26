//! Multi-action subcommand enums — `lazuli translate`, `lazuli
//! examples`, `lazuli migrate`, `lazuli design`.
//!
//! Lifted out of the `cli_args` god-file in the rails-style R9 split.
//! No behavior change: `lazuli --help` output is unchanged.

use std::path::PathBuf;

#[derive(Debug, clap::Subcommand)]
pub(crate) enum TranslateCommand {
    /// Extract translatable keys to catalog stub files.
    Extract {
        /// Path to a `.lzi` file or a directory containing one.
        input: PathBuf,
        /// Output directory for per-locale catalog files (default `./i18n`).
        #[arg(long, default_value = "./i18n")]
        out: PathBuf,
        /// Only extract one locale's catalog. Defaults to every
        /// `app.locale.supported` tag.
        #[arg(long)]
        locale: Option<String>,
        /// Fail the CLI if any referenced `@translation.<key>` does
        /// not resolve, or if any declared key is missing a variant
        /// for a supported locale. CI gate.
        #[arg(long)]
        check: bool,
    },
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum ExamplesCommand {
    /// Emit deterministic JSONL bundle of curated examples for AI file-load.
    Bundle {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Validate that every curated example still compiles and matches frozen IR.
    Validate {
        /// Also check provenance.last_validated freshness (warn if > 90 days).
        #[arg(long)]
        check_decay: bool,
    },
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum MigrateCommand {
    /// Apply pending migrations.
    Up {
        /// Apply migrations up to and including this version.
        #[arg(long)]
        target: Option<String>,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Roll back applied migrations.
    Down {
        /// Number of migrations to roll back.
        #[arg(long, default_value_t = 1)]
        steps: u32,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Show current version and pending migrations.
    Status,
    /// Apply DSL recipes that rewrite `.lzi`/`.lzx` source between
    /// two Lazuli versions. Recipes live under
    /// `migrations/recipes/<from>-to-<to>/`.
    Dsl {
        /// Source version tag (e.g. `v0.11`).
        #[arg(long)]
        from: String,
        /// Target version tag (e.g. `v0.12`).
        #[arg(long)]
        to: String,
        /// Print the diff per file without writing.
        #[arg(long)]
        dry_run: bool,
        /// Project root containing `.lzi`/`.lzx` files. Defaults to
        /// the current directory.
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

#[derive(Debug, clap::Subcommand)]
pub(crate) enum DesignCommand {
    /// Import an external design-token catalog into `design.lzi`.
    Import {
        #[arg(long)]
        from: PathBuf,
        #[arg(long, value_enum, default_value_t = super::DesignImportFormat::Figma)]
        format: super::DesignImportFormat,
        #[arg(long)]
        overwrite: bool,
    },
    /// Export `design.lzi` into an external design-token catalog.
    Export {
        #[arg(long, value_enum)]
        target: super::DesignExportTarget,
        #[arg(long)]
        out: PathBuf,
    },
    /// Diff `design.lzi` against an external design-token catalog.
    Diff {
        #[arg(long)]
        against: PathBuf,
    },
}
