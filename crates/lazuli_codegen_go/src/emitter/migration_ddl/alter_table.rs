//! `ALTER TABLE` emission for schema diffs.
//!
//! LAZ-create-table-alter-emit (Cell A11). Today every initial
//! migration emitted under `create_table.rs` is `CREATE TABLE IF NOT
//! EXISTS`, which no-ops on existing databases. Pilots have to author
//! ALTER SQL by hand. The pieces in this module let the orchestrator
//! compute a `SchemaDiff` between the baseline (last emitted
//! migration on disk) and the current IR, then emit a follow-up
//! `<NNN+1>_<feature>_<resource>_alter.sql` + `.down.sql` pair.
//!
//! Three diff shapes flow through `SchemaDiff`:
//!
//! - `ColumnAdd` — `ALTER TABLE … ADD COLUMN IF NOT EXISTS`. NOT-NULL
//!   without DEFAULT auto-downgrades to nullable + TODO comment so the
//!   migration applies cleanly against existing rows.
//! - `ColumnDrop` — `ALTER TABLE … DROP COLUMN IF EXISTS`. Gated by
//!   `AlterEmitOptions.allow_drops`: without the flag, drops emit as
//!   commented lines under a warning header.
//! - `TypeChange` — `ALTER COLUMN … TYPE … USING …::…`. Safe casts
//!   (`is_safe_cast` closed catalog) emit live; unsafe casts emit
//!   commented with operator-review hints.
//!
//! TEMP-STUB notice: cell A10 owns `crates/lazuli_codegen_go/src/
//! emitter/schema_diff.rs` (the typed `SchemaDiff` + diff algorithm).
//! At the time cell A11 landed `schema_diff.rs` was NOT yet in tree,
//! so the typed shapes below are local stubs. When A10 merges, the
//! orchestrator should switch to importing `super::schema_diff::
//! SchemaDiff` (and friends) and either re-export the local stubs as
//! aliases or delete them outright. The public API of
//! `emit_alter_migration_file` does not need to change — its
//! argument is structural, not nominal.

use std::fmt::Write;

use super::sql_builder::{comment_value, lower_snake, quote_ident, sql_ident};
use crate::GeneratedFile;

/// Sentinel for an explicit per-column DEFAULT expression. Currently only
/// the analyzer-recognized `DefaultValue` variants surface here; the
/// SQL literal is rendered via `render_default_sql`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlterDefault {
    /// Quoted SQL string literal — `'hello'`.
    String(String),
    /// Integer literal — `42`.
    Integer(i64),
    /// Boolean literal — `TRUE` / `FALSE`.
    Boolean(bool),
    /// SQL NULL.
    Null,
    /// Raw SQL fragment (`NOW()`, `'{}'::jsonb`, …) when the caller has
    /// already shaped the literal for SQL. The emitter trusts the
    /// fragment verbatim.
    Raw(String),
}

fn render_default_sql(default: &AlterDefault) -> String {
    match default {
        AlterDefault::String(s) => format!("'{}'", s.replace('\'', "''")),
        AlterDefault::Integer(n) => n.to_string(),
        AlterDefault::Boolean(b) => {
            if *b {
                "TRUE".to_owned()
            } else {
                "FALSE".to_owned()
            }
        }
        AlterDefault::Null => "NULL".to_owned(),
        AlterDefault::Raw(raw) => raw.clone(),
    }
}

/// One row in `SchemaDiff.adds`. `not_null` mirrors the IR field's
/// requiredness; the emitter degrades NOT-NULL-without-default to a
/// nullable column + TODO comment because adding NOT NULL to an existing
/// table with rows fails Postgres' constraint check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnAdd {
    pub table: String,
    pub column: String,
    pub sql_type: String,
    pub not_null: bool,
    pub default: Option<AlterDefault>,
}

/// One row in `SchemaDiff.drops`. The emitter honors the `--allow-drops`
/// flag: without it, drops emit as commented-out lines under a warning
/// header. With it, the drop is emitted as `ALTER TABLE … DROP COLUMN
/// IF EXISTS`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDrop {
    pub table: String,
    pub column: String,
    /// Carried so the `.down.sql` companion can re-add the dropped
    /// column with its previous shape.
    pub previous_sql_type: String,
    pub previous_not_null: bool,
    pub previous_default: Option<AlterDefault>,
}

/// One row in `SchemaDiff.type_changes`. Safe casts (a closed catalog
/// inside `is_safe_cast`) emit `ALTER COLUMN … TYPE … USING …::…`. Unsafe
/// casts emit a commented-out line under a warning header so authors
/// review them before pushing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeChange {
    pub table: String,
    pub column: String,
    pub previous_sql_type: String,
    pub new_sql_type: String,
}

/// TEMP-STUB shape — see module banner. A10's `schema_diff.rs` will own
/// the typed shape; the orchestrator wires it through to
/// `emit_alter_migration_file`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaDiff {
    pub adds: Vec<ColumnAdd>,
    pub drops: Vec<ColumnDrop>,
    pub type_changes: Vec<TypeChange>,
}

impl SchemaDiff {
    /// `true` when no adds, drops, or type changes were recorded.
    ///
    /// The orchestrator skips writing an ALTER migration file when the
    /// diff is empty so noise stays out of the output listing.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// assert!(SchemaDiff::default().is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.adds.is_empty() && self.drops.is_empty() && self.type_changes.is_empty()
    }
}

/// Closed catalog of safe type casts. Mirrors the Postgres-side rules in
/// the proposal: widening integer casts and adding structure on top of
/// TEXT are safe; everything else needs operator review.
fn is_safe_cast(from: &str, to: &str) -> bool {
    matches!(
        (
            from.to_ascii_uppercase().as_str(),
            to.to_ascii_uppercase().as_str()
        ),
        ("TEXT", "JSONB")
            | ("TEXT", "JSON")
            | ("INTEGER", "BIGINT")
            | ("INT", "BIGINT")
            | ("INT4", "BIGINT")
            | ("INT4", "INT8")
            | ("SMALLINT", "INTEGER")
            | ("SMALLINT", "BIGINT")
            | ("VARCHAR", "TEXT")
            | ("CHAR", "TEXT")
    )
}

/// Render the body of an `ALTER TABLE … ADD COLUMN IF NOT EXISTS …` line
/// from a `ColumnAdd`. When `add.not_null` is set but the author did not
/// supply a default, NOT NULL is omitted and a TODO comment is appended
/// to the line — adding NOT NULL to an existing table without a default
/// fails on any pre-existing row.
fn render_alter_add(add: &ColumnAdd) -> String {
    let mut sql = format!(
        "ALTER TABLE {} ADD COLUMN IF NOT EXISTS {} {}",
        quote_ident(&add.table),
        sql_ident(&add.column),
        add.sql_type,
    );

    let has_default = add.default.is_some();
    let nullable_downgrade = add.not_null && !has_default;

    if add.not_null && has_default {
        sql.push_str(" NOT NULL");
    }
    if let Some(default) = &add.default {
        sql.push_str(" DEFAULT ");
        sql.push_str(&render_default_sql(default));
    }
    sql.push(';');
    if nullable_downgrade {
        sql.push_str(&format!(
            " -- TODO: backfill {}.{}, then `ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;`",
            add.table,
            add.column,
            quote_ident(&add.table),
            sql_ident(&add.column),
        ));
    }
    sql
}

/// Inverse of `render_alter_add` — `DROP COLUMN IF EXISTS` is the safe
/// rollback for an additive migration regardless of NOT NULL / DEFAULT.
fn render_alter_add_down(add: &ColumnAdd) -> String {
    format!(
        "ALTER TABLE {} DROP COLUMN IF EXISTS {};",
        quote_ident(&add.table),
        sql_ident(&add.column),
    )
}

fn render_alter_drop(drop: &ColumnDrop) -> String {
    format!(
        "ALTER TABLE {} DROP COLUMN IF EXISTS {};",
        quote_ident(&drop.table),
        sql_ident(&drop.column),
    )
}

/// Inverse of `render_alter_drop` — re-create the column with its
/// previous shape. Same NOT-NULL-without-default downgrade rule applies
/// to keep the rollback idempotent on existing rows.
fn render_alter_drop_down(drop: &ColumnDrop) -> String {
    let recreated = ColumnAdd {
        table: drop.table.clone(),
        column: drop.column.clone(),
        sql_type: drop.previous_sql_type.clone(),
        not_null: drop.previous_not_null,
        default: drop.previous_default.clone(),
    };
    render_alter_add(&recreated)
}

fn render_alter_type_change(change: &TypeChange) -> String {
    format!(
        "ALTER TABLE {} ALTER COLUMN {} TYPE {} USING {}::{};",
        quote_ident(&change.table),
        sql_ident(&change.column),
        change.new_sql_type,
        sql_ident(&change.column),
        change.new_sql_type.to_ascii_lowercase(),
    )
}

fn render_alter_type_change_down(change: &TypeChange) -> String {
    // Swap previous_sql_type / new_sql_type for the rollback. The
    // safety of the forward cast is the caller's concern; emitter
    // honesty: if the forward cast was safe enough to apply
    // automatically, the rollback prints the inverse using the same
    // `USING <col>::<type>` shape so operators can read it and
    // override if their data has drifted.
    let inverse = TypeChange {
        table: change.table.clone(),
        column: change.column.clone(),
        previous_sql_type: change.new_sql_type.clone(),
        new_sql_type: change.previous_sql_type.clone(),
    };
    render_alter_type_change(&inverse)
}

/// Knobs the orchestrator passes through from the CLI. `allow_drops`
/// gates whether `diff.drops` emit as live SQL or commented lines + a
/// warning header.
///
/// ## Examples
///
/// ```ignore
/// let opts = AlterEmitOptions { allow_drops: false };
/// assert!(!opts.allow_drops);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct AlterEmitOptions {
    /// `true` when the CLI was invoked with the explicit
    /// `--allow-drops` flag.
    pub allow_drops: bool,
}

/// Emit a `<NNN+1>_<feature>_<resource>_alter.sql` + `.down.sql` pair
/// from a `SchemaDiff`. Returns `None` when `diff.is_empty()` — no file
/// is written if there is nothing to ALTER.
///
/// `sequence` is the index `NNN+1` (1-based) that the orchestrator
/// derives from `ls migrations/`. `feature` and `resource` slug into
/// the filename the same way `emit_resource_migration` slugs the
/// initial migration.
///
/// ## Examples
///
/// ```ignore
/// let pair = emit_alter_migration_file(2, "billing", "Customer", "billing.lzi", &diff, opts);
/// // None when the diff is empty.
/// ```
pub fn emit_alter_migration_file(
    sequence: usize,
    feature: &str,
    resource: &str,
    source_label: &str,
    diff: &SchemaDiff,
    options: AlterEmitOptions,
) -> Option<(GeneratedFile, GeneratedFile)> {
    if diff.is_empty() {
        return None;
    }

    let resource_slug = lower_snake(resource);
    let up_path = format!(
        "migrations/{:03}_{}_{}_alter.sql",
        sequence, feature, resource_slug
    );
    let down_path = format!(
        "migrations/{:03}_{}_{}_alter.down.sql",
        sequence, feature, resource_slug
    );

    let up_contents = render_alter_up(feature, resource, source_label, diff, options);
    let down_contents = render_alter_down(feature, resource, source_label, diff, options);

    Some((
        GeneratedFile {
            path: up_path,
            contents: up_contents,
        },
        GeneratedFile {
            path: down_path,
            contents: down_contents,
        },
    ))
}

fn render_alter_up(
    feature: &str,
    resource: &str,
    source_label: &str,
    diff: &SchemaDiff,
    options: AlterEmitOptions,
) -> String {
    let mut sql = String::new();
    let _ = writeln!(sql, "-- Code generated by lazuli; DO NOT EDIT.");
    let _ = writeln!(sql, "-- source: {}", comment_value(source_label));
    let _ = writeln!(
        sql,
        "-- alter for {}.{}",
        comment_value(feature),
        comment_value(resource),
    )
    .unwrap();

    // Optional warning header — surfaces when either drops are
    // commented out (no `--allow-drops`) or type changes are unsafe.
    let drops_commented = !options.allow_drops && !diff.drops.is_empty();
    let unsafe_type_changes: Vec<&TypeChange> = diff
        .type_changes
        .iter()
        .filter(|tc| !is_safe_cast(&tc.previous_sql_type, &tc.new_sql_type))
        .collect();
    let unsafe_present = !unsafe_type_changes.is_empty();
    if drops_commented || unsafe_present {
        writeln!(sql);
        let _ = writeln!(sql, "-- WARNING:");
        if drops_commented {
            let _ = writeln!(
                sql,
                "--   DROP COLUMN statements are COMMENTED OUT. Re-run `lazuli generate go --allow-drops`",
            )
            .unwrap();
            writeln!(sql, "--   to emit them as live SQL.");
        }
        if unsafe_present {
            let _ = writeln!(
                sql,
                "--   Some ALTER COLUMN TYPE casts are NOT in the safe catalog and are commented out.",
            )
            .unwrap();
            writeln!(
                sql,
                "--   Review the cast manually, write a backfill if needed, then uncomment.",
            )
            .unwrap();
        }
    }
    writeln!(sql);

    if !diff.adds.is_empty() {
        let _ = writeln!(sql, "-- ADDS");
        let mut adds = diff.adds.clone();
        adds.sort_by(|a, b| a.column.cmp(&b.column));
        for add in &adds {
            let _ = writeln!(sql, "{}", render_alter_add(add));
        }
    }

    if !diff.drops.is_empty() {
        if !diff.adds.is_empty() {
            let _ = writeln!(sql);
        }
        let _ = writeln!(sql, "-- DROPS");
        let mut drops = diff.drops.clone();
        drops.sort_by(|a, b| a.column.cmp(&b.column));
        for drop in &drops {
            if options.allow_drops {
                let _ = writeln!(sql, "{}", render_alter_drop(drop));
            } else {
                let _ = writeln!(sql, "-- {}", render_alter_drop(drop));
            }
        }
    }

    if !diff.type_changes.is_empty() {
        if !diff.adds.is_empty() || !diff.drops.is_empty() {
            let _ = writeln!(sql);
        }
        let _ = writeln!(sql, "-- TYPE CHANGES");
        let mut changes = diff.type_changes.clone();
        changes.sort_by(|a, b| a.column.cmp(&b.column));
        for change in &changes {
            if is_safe_cast(&change.previous_sql_type, &change.new_sql_type) {
                let _ = writeln!(sql, "{}", render_alter_type_change(change));
            } else {
                let _ = writeln!(sql, "-- {}", render_alter_type_change(change));
            }
        }
    }

    sql
}

fn render_alter_down(
    feature: &str,
    resource: &str,
    source_label: &str,
    diff: &SchemaDiff,
    options: AlterEmitOptions,
) -> String {
    let mut sql = String::new();
    let _ = writeln!(sql, "-- Code generated by lazuli; DO NOT EDIT.");
    let _ = writeln!(sql, "-- source: {}", comment_value(source_label));
    let _ = writeln!(
        sql,
        "-- rollback for {}.{} alter",
        comment_value(feature),
        comment_value(resource),
    )
    .unwrap();
    writeln!(sql);

    if !diff.type_changes.is_empty() {
        let _ = writeln!(sql, "-- TYPE CHANGES (inverse)");
        let mut changes = diff.type_changes.clone();
        changes.sort_by(|a, b| a.column.cmp(&b.column));
        for change in &changes {
            if is_safe_cast(&change.previous_sql_type, &change.new_sql_type) {
                let _ = writeln!(sql, "{}", render_alter_type_change_down(change));
            } else {
                let _ = writeln!(sql, "-- {}", render_alter_type_change_down(change));
            }
        }
    }

    if !diff.drops.is_empty() {
        if !diff.type_changes.is_empty() {
            let _ = writeln!(sql);
        }
        let _ = writeln!(sql, "-- DROPS (inverse: re-add)");
        let mut drops = diff.drops.clone();
        drops.sort_by(|a, b| a.column.cmp(&b.column));
        for drop in &drops {
            // Mirror the forward gating: when the forward DROP was
            // commented out (no `--allow-drops`), the inverse re-add
            // is also commented so the rollback parses cleanly when
            // applied against a database where the forward never
            // executed.
            if options.allow_drops {
                let _ = writeln!(sql, "{}", render_alter_drop_down(drop));
            } else {
                let _ = writeln!(sql, "-- {}", render_alter_drop_down(drop));
            }
        }
    }

    if !diff.adds.is_empty() {
        if !diff.type_changes.is_empty() || !diff.drops.is_empty() {
            let _ = writeln!(sql);
        }
        let _ = writeln!(sql, "-- ADDS (inverse: drop)");
        let mut adds = diff.adds.clone();
        adds.sort_by(|a, b| a.column.cmp(&b.column));
        for add in &adds {
            let _ = writeln!(sql, "{}", render_alter_add_down(add));
        }
    }

    sql
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_diff_is_empty() {
        assert!(SchemaDiff::default().is_empty());
    }

    #[test]
    fn safe_cast_text_to_jsonb_is_allowed() {
        assert!(is_safe_cast("TEXT", "JSONB"));
        assert!(!is_safe_cast("JSONB", "TEXT"));
    }
}
