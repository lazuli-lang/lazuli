//! `[migrations]`, `[seeds]`, and `[dev]` block schema — three small
//! sections covering database migration strategy, seed-data layout,
//! and developer-only plugin path overrides.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `[migrations]` block — paths for generated and manual SQL
/// migrations, plus the apply strategy.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Migrations {
    /// Directory codegen owns (overwritten on every `lazuli generate go`).
    #[serde(default = "default_migrations_generated")]
    pub generated: String,
    /// Directory authors own (never overwritten by codegen).
    #[serde(default = "default_migrations_manual")]
    pub manual: String,
    /// How `lazuli migrate up` applies the migrations.
    #[serde(default)]
    pub strategy: MigrationStrategy,
}

/// Frente 1 — canonical defaults for `[migrations]`. Applied
/// transparently when the block is absent.
impl Default for Migrations {
    fn default() -> Self {
        Self {
            generated: default_migrations_generated(),
            manual: default_migrations_manual(),
            strategy: MigrationStrategy::default(),
        }
    }
}

/// Closed catalog of migration-apply strategies.
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationStrategy {
    /// Apply pending migrations automatically on `lazuli migrate up`.
    #[default]
    Auto,
    /// Surface pending migrations but never apply.
    Manual,
    /// `lazuli migrate status` only — refuse `up`/`down`.
    CheckOnly,
}

/// `[seeds]` block — where seed data lives and whether to run it
/// automatically.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Seeds {
    /// Directory containing the project's seed blocks.
    #[serde(default = "default_seeds_dir")]
    pub dir: String,
    /// Whether `lazuli dev` should run `lazuli seed` after every
    /// regeneration.
    #[serde(default)]
    pub auto: bool,
}

/// Frente 1 — canonical defaults for `[seeds]`. Applied transparently
/// when the block is absent.
impl Default for Seeds {
    fn default() -> Self {
        Self {
            dir: default_seeds_dir(),
            auto: false,
        }
    }
}

/// `[dev]` block — developer-only overrides honored by the `lazuli
/// dev` watcher and friends.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DevOverrides {
    /// Map of plugin name → local checkout path. Lets a contributor
    /// run against an unpublished plugin without re-installing.
    #[serde(default)]
    pub plugin_paths: BTreeMap<String, String>,
}

pub(super) fn default_migrations_generated() -> String {
    "dist/go/migrations".to_string()
}

pub(super) fn default_migrations_manual() -> String {
    "migrations".to_string()
}

pub(super) fn default_seeds_dir() -> String {
    "seeds".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_default_paths() {
        let m = Migrations::default();
        assert_eq!(m.generated, "dist/go/migrations");
        assert_eq!(m.manual, "migrations");
    }

    #[test]
    fn seeds_default_dir_is_seeds() {
        assert_eq!(Seeds::default().dir, "seeds");
    }
}
