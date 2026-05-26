//! Schema diff algorithm for incremental migration emission.
//!
//! Today every `CREATE TABLE IF NOT EXISTS` migration we emit is a
//! no-op against a database that already has the table — adding a
//! column to a resource and re-running `lazuli generate` rebuilds
//! the same `CREATE TABLE` block with the new column wedged into the
//! middle, but PostgreSQL skips the create entirely (the table
//! exists), so the new column never lands. Pilots have had to
//! `ALTER TABLE ... ADD COLUMN` by hand. This cell (A10) delivers
//! the schema-diff primitive; cell A11 consumes it to emit ALTER
//! SQL and cell A12 adds the doctor lint that prevents silent
//! divergence.
//!
//! Scope of this module:
//! - `Column` / `TypeChange` / `ResourceSchema` / `SchemaDiff` types.
//! - `diff(baseline, current)` — pure comparison.
//! - `parse_baseline_from_migration(path)` — recovers the last
//!   emitted column shape from a generated `.sql` file. The parser
//!   is hand-rolled because (a) we control the emitter's output
//!   format, so a full SQL parser is overkill, and (b) `lazuli`
//!   already has zero SQL-parser dependency and we want to keep it
//!   that way. Lives in `parse.rs`.
//! - `current_schema_from_ir(resource)` — projects an IR resource
//!   into a `ResourceSchema`, mirroring the type mapping used by
//!   `migration_ddl.rs` (`pg_type_for` / `pg_type_for_builtin`).
//!   We deliberately do NOT do FK resolution here — the cell A11
//!   ALTER emitter never re-targets a foreign key without an
//!   explicit migration plan, so columns whose type is
//!   `TypeRef::UserDefined` that would normally lower to `BIGINT`
//!   are emitted as their nominal IR mapping. Callers that need
//!   the cross-feature view stay on `migration_ddl::pg_type_for_field`.
//!   Lives in `ir.rs`.
//!
//! Out of scope for this cell:
//! - Emitting the ALTER SQL (cell A11, separate file `migration.rs`).
//! - Doctor lint that warns when ALTER coverage is missing (A12).
//! - Index diffs (separate primitive, future cell).
//! - Constraint diffs (the parser intentionally skips them; A11's
//!   ALTER emission also stays inside the column dimension for v1).

mod ir;
mod parse;

#[cfg(test)]
mod test_support;

pub use ir::current_schema_from_ir;
pub use parse::{parse_baseline_from_migration, parse_baseline_from_str, ParseError};

/// A single column extracted either from a generated migration or
/// from an IR resource. The shape is intentionally the lowest
/// common denominator across the two sources: name, SQL type, NOT
/// NULL bit, optional `DEFAULT` literal. Generated-as expressions
/// and trailing comments are dropped on the parser side — the
/// diff is shape-only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
    pub default: Option<String>,
}

impl Column {
    pub fn new(name: impl Into<String>, sql_type: impl Into<String>, nullable: bool) -> Self {
        Self {
            name: name.into(),
            sql_type: sql_type.into(),
            nullable,
            default: None,
        }
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }
}

/// A column whose SQL type changed between the baseline migration
/// and the current IR. Nullability flips and default flips are
/// captured separately by A11 as part of the ALTER plan; this
/// struct is the dimension that matters for `ALTER COLUMN TYPE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeChange {
    pub column: String,
    pub old_type: String,
    pub new_type: String,
}

/// Column-level shape of a resource at one point in time. Order is
/// preserved (matches the order columns appear in the migration or
/// in the IR) so callers that want stable error messages can index
/// by position. The diff itself is order-insensitive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceSchema {
    pub columns: Vec<Column>,
}

impl ResourceSchema {
    pub fn new(columns: Vec<Column>) -> Self {
        Self { columns }
    }

    fn find(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }
}

/// The output of `diff(baseline, current)`. `adds` are columns the
/// IR has that the baseline migration does not; `drops` are the
/// reverse; `type_changes` are columns present in both but whose
/// SQL type does not match. Nullability and default flips are
/// reserved for A11 (they require `ALTER COLUMN SET NOT NULL` /
/// `ALTER COLUMN SET DEFAULT` clauses that A11 handles in the
/// emitter, not here).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaDiff {
    pub adds: Vec<Column>,
    pub drops: Vec<Column>,
    pub type_changes: Vec<TypeChange>,
}

impl SchemaDiff {
    pub fn is_empty(&self) -> bool {
        self.adds.is_empty() && self.drops.is_empty() && self.type_changes.is_empty()
    }
}

/// Pure diff. Treat both inputs as unordered column sets keyed by
/// name. SQL type comparison is case-sensitive after normalization
/// — both the parser and `current_schema_from_ir` emit uppercase
/// type names matching `migration_ddl::pg_type_for_builtin`, so a
/// type mismatch is always a semantic shift (e.g. `TEXT` ↔ `JSONB`)
/// rather than a `text` vs `TEXT` formatting drift.
pub fn diff(baseline: &ResourceSchema, current: &ResourceSchema) -> SchemaDiff {
    let mut out = SchemaDiff::default();

    for current_col in &current.columns {
        match baseline.find(&current_col.name) {
            None => out.adds.push(current_col.clone()),
            Some(baseline_col) => {
                if baseline_col.sql_type != current_col.sql_type {
                    out.type_changes.push(TypeChange {
                        column: current_col.name.clone(),
                        old_type: baseline_col.sql_type.clone(),
                        new_type: current_col.sql_type.clone(),
                    });
                }
            }
        }
    }

    for baseline_col in &baseline.columns {
        if current.find(&baseline_col.name).is_none() {
            out.drops.push(baseline_col.clone());
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::col;

    #[test]
    fn diff_detects_added_column() {
        let baseline = ResourceSchema::new(vec![col("a", "TEXT", true), col("b", "BIGINT", false)]);
        let current = ResourceSchema::new(vec![
            col("a", "TEXT", true),
            col("b", "BIGINT", false),
            col("c", "JSONB", true),
        ]);
        let out = diff(&baseline, &current);
        assert_eq!(out.adds, vec![col("c", "JSONB", true)]);
        assert!(out.drops.is_empty());
        assert!(out.type_changes.is_empty());
    }

    #[test]
    fn diff_detects_dropped_column() {
        let baseline = ResourceSchema::new(vec![
            col("a", "TEXT", true),
            col("b", "BIGINT", false),
            col("c", "JSONB", true),
        ]);
        let current = ResourceSchema::new(vec![col("a", "TEXT", true), col("b", "BIGINT", false)]);
        let out = diff(&baseline, &current);
        assert_eq!(out.drops, vec![col("c", "JSONB", true)]);
        assert!(out.adds.is_empty());
        assert!(out.type_changes.is_empty());
    }

    #[test]
    fn diff_detects_type_change() {
        let baseline = ResourceSchema::new(vec![col("address", "TEXT", true)]);
        let current = ResourceSchema::new(vec![col("address", "JSONB", true)]);
        let out = diff(&baseline, &current);
        assert_eq!(
            out.type_changes,
            vec![TypeChange {
                column: "address".to_owned(),
                old_type: "TEXT".to_owned(),
                new_type: "JSONB".to_owned(),
            }]
        );
        assert!(out.adds.is_empty());
        assert!(out.drops.is_empty());
    }

    #[test]
    fn diff_no_change_is_empty() {
        let baseline = ResourceSchema::new(vec![col("a", "TEXT", false), col("b", "BIGINT", true)]);
        let current = baseline.clone();
        let out = diff(&baseline, &current);
        assert!(out.is_empty(), "expected empty diff, got {out:?}");
    }

    #[test]
    fn diff_combines_add_drop_type_change() {
        let baseline = ResourceSchema::new(vec![
            col("a", "TEXT", true),
            col("b", "BIGINT", false),
            col("address", "TEXT", true),
        ]);
        let current = ResourceSchema::new(vec![
            col("a", "TEXT", true),
            col("address", "JSONB", true),
            col("profile_photo", "JSONB", true),
        ]);
        let out = diff(&baseline, &current);
        assert_eq!(out.adds, vec![col("profile_photo", "JSONB", true)]);
        assert_eq!(out.drops, vec![col("b", "BIGINT", false)]);
        assert_eq!(
            out.type_changes,
            vec![TypeChange {
                column: "address".to_owned(),
                old_type: "TEXT".to_owned(),
                new_type: "JSONB".to_owned(),
            }]
        );
    }
}
