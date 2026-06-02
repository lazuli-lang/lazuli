//! DDL migration emission for resource tables.
//!
//! `emit_migrations` is the entry point used by the top-level Go
//! codegen orchestrator. It walks every feature's resources, sorts
//! them so FK targets come before their references (Kahn's algorithm,
//! see `topo`), and emits one `<NNN>_<feature>_<resource>.sql` up
//! migration plus a paired `.down.sql` companion. A shared
//! `audit_log.down.sql` rollback closes the run — the matching
//! `audit_log.sql` up migration is emitted by the top-level module
//! emitter and is intentionally not the responsibility of this
//! walker.
//!
//! The sub-tree is split Rails-style for cold-readability:
//!
//! - [`sql_builder`] — identifier quoting, snake-casing, reserved
//!   words, and dialect-agnostic helpers used by every sibling.
//! - [`sql_column`] — `SqlColumn` rendering + `pg_type_for*` IR-to-
//!   Postgres type lowering, plus the `encryption_marker_for` `--`
//!   comment helper.
//! - [`constraint`] — UNIQUE (inline + block), FOREIGN KEY, composite
//!   key clauses.
//! - [`index`] — authored indexes (`@full_text`, method-tagged
//!   b-tree/GIN/GIST) + session-rotation companion indexes.
//! - [`topo`] — FK-aware topological sort + `foreign_key_owner`
//!   resolution.
//! - [`create_table`] — `CREATE TABLE` body + column gather +
//!   session-rotation column auto-injection.
//! - [`drop_table`] — `DROP TABLE` body + commented `DROP INDEX`
//!   hints + the shared audit-log rollback.
//! - [`alter_table`] — `ALTER TABLE` emission from `SchemaDiff`
//!   (cell A11). The TEMP-STUB shapes carried here will migrate to
//!   `crates/lazuli_codegen_go/src/emitter/schema_diff.rs` (cell
//!   A10) once that lands.
//!
//! Cross-feature FK resolution piggybacks on `super::cross_feature::
//! CrossFeatureIndex`, which the top-level module emitter builds
//! once per `Module`. See `crates/lazuli_codegen_go/src/emitter/
//! cross_feature.rs` for the indexing contract.

mod alter_table;
mod constraint;
mod create_table;
mod drop_table;
mod index;
mod sql_builder;
mod sql_column;
mod topo;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod auth_session_tests;
#[cfg(test)]
mod column_type_tests;
#[cfg(test)]
mod feature_walk_tests;
#[cfg(test)]
mod foreign_key_tests;

use lazuli_ir::{Feature, Module, Resource};

#[allow(unused_imports)]
use lazuli_ir::{BuiltinType, Constraint, TypeRef};

use super::cross_feature::CrossFeatureIndex;
use crate::GeneratedFile;

pub use alter_table::{
    AlterDefault, AlterEmitOptions, ColumnAdd, ColumnDrop, SchemaDiff, TypeChange,
    emit_alter_migration_file,
};
pub(crate) use constraint::unique_violation_codes;
// RESTRICT-WHERE-DIALECT-001 — the referential-guard emitter lowers a
// `restrict ... where <predicate>` through the SAME predicate lowering the
// partial-unique index uses, so `== nil` → `IS NULL` etc. is shared, not
// duplicated.
pub(crate) use index::restrict_where_sql;

// Internal-only re-exports — tests live in this file and use
// `super::*` to reach the prod helpers. New code should reach into
// the sub-modules directly rather than via this aggregator.
use create_table::emit_resource_migration;
use drop_table::{emit_audit_log_down_migration, emit_resource_down_migration};
use sql_builder::lower_snake;
use topo::topo_sort_resources;

#[allow(unused_imports)]
use create_table::resource_columns;
#[allow(unused_imports)]
use sql_builder::{comment_value, quote_ident, sql_ident};
#[allow(unused_imports)]
use sql_column::{PgType, pg_type_for, pg_type_for_capability, pg_type_for_field};
#[allow(unused_imports)]
use topo::foreign_key_owner;

/// Emit SQL migrations in deterministic, cross-feature lexical order.
///
/// The returned paths are relative to the generated Go output root:
/// `migrations/<NNN>_<feature>_<resource>.sql` plus companion
/// `migrations/<NNN>_<feature>_<resource>.down.sql` rollback files.
/// The shared audit table rollback is emitted here because the matching
/// `migrations/audit_log.sql` up migration is always emitted by the
/// top-level module emitter.
///
/// ## Examples
///
/// ```ignore
/// let files = emit_migrations(&module, "billing.lzi");
/// assert!(files.iter().any(|f| f.path.contains("migrations/")));
/// ```
pub fn emit_migrations(module: &Module, source_label: &str) -> Vec<GeneratedFile> {
    let cross_index = CrossFeatureIndex::build(module);
    let raw_resources: Vec<(&Feature, &Resource)> = module
        .features
        .iter()
        .flat_map(|feature| {
            feature
                .resources
                .iter()
                .map(move |resource| (feature, resource))
        })
        .collect();

    // WAR-RUNTIME-MIGRATION-03 — order resources so every FK target's
    // CREATE TABLE runs BEFORE the referencing FOREIGN KEY constraint.
    // Lexical (feature, resource) is the tiebreaker for resources with
    // no dependency between them, so output stays stable when the
    // dependency graph doesn't pin a relative order.
    let resources = topo_sort_resources(module, &raw_resources, &cross_index);

    let mut files = Vec::with_capacity(resources.len() * 2 + 1);

    for (idx, (feature, resource)) in resources.iter().copied().enumerate() {
        let resource_slug = lower_snake(&resource.name);
        files.push(GeneratedFile {
            path: format!(
                "migrations/{:03}_{}_{}.sql",
                idx + 1,
                feature.name,
                resource_slug
            ),
            contents: emit_resource_migration(
                module,
                feature,
                resource,
                source_label,
                &cross_index,
            ),
        });
    }

    for (idx, (feature, resource)) in resources.iter().copied().enumerate() {
        let resource_slug = lower_snake(&resource.name);
        files.push(GeneratedFile {
            path: format!(
                "migrations/{:03}_{}_{}.down.sql",
                idx + 1,
                feature.name,
                resource_slug
            ),
            contents: emit_resource_down_migration(feature, resource),
        });
    }

    files.push(emit_audit_log_down_migration());
    files
}
