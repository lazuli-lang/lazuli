//! `[migrations]`, `[seeds]`, and `[dev]` block schema — three small
//! sections covering database migration strategy, seed-data layout,
//! and developer-only plugin path overrides.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Migrations {
    #[serde(default = "default_migrations_generated")]
    pub generated: String,
    #[serde(default = "default_migrations_manual")]
    pub manual: String,
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

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationStrategy {
    #[default]
    Auto,
    Manual,
    CheckOnly,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Seeds {
    #[serde(default = "default_seeds_dir")]
    pub dir: String,
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DevOverrides {
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
