//! Cell A11 — LAZ-create-table-alter-emit.
//!
//! Fixture tests for `emit_alter_migration_file` (in
//! `lazuli_codegen_go::emitter::migration_ddl`). Each test exercises one
//! row of the cell's fixture table:
//!
//! 1. Adds (nullable, no default) — emits `ADD COLUMN IF NOT EXISTS`.
//! 2. Adds (NOT NULL + DEFAULT) — emits NOT NULL with the default.
//! 3. Adds (NOT NULL, no default) — degrades to nullable + TODO comment
//!    (irreversible NOT NULL on existing rows fails Postgres' check).
//! 4. Drops without `--allow-drops` — commented out + warning header.
//! 5. Drops with `--allow-drops` — emits `DROP COLUMN IF EXISTS`.
//! 6. Type change `TEXT → JSONB` — safe cast, emits `USING <col>::jsonb`.
//!
//! Cross-cell coordination: when A10 lands `crates/lazuli_codegen_go/src/
//! emitter/schema_diff.rs`, the typed shapes used here become aliases of
//! A10's exports (see the TEMP-STUB banner in migration_ddl.rs). The
//! public emit API does not change.

use lazuli_codegen_go::emitter::migration_ddl::{
    AlterDefault, AlterEmitOptions, ColumnAdd, ColumnDrop, SchemaDiff, TypeChange,
    emit_alter_migration_file,
};

fn diff_with(
    adds: Vec<ColumnAdd>,
    drops: Vec<ColumnDrop>,
    type_changes: Vec<TypeChange>,
) -> SchemaDiff {
    SchemaDiff {
        adds,
        drops,
        type_changes,
    }
}

fn emit(diff: SchemaDiff, options: AlterEmitOptions) -> (String, String) {
    let (up, down) = emit_alter_migration_file(2, "customer", "Customer", "crm", &diff, options)
        .expect("non-empty diff must emit");
    assert_eq!(up.path, "migrations/002_customer_customer_alter.sql");
    assert_eq!(down.path, "migrations/002_customer_customer_alter.down.sql");
    (up.contents, down.contents)
}

#[test]
fn empty_diff_returns_none() {
    let result = emit_alter_migration_file(
        2,
        "customer",
        "Customer",
        "crm",
        &SchemaDiff::default(),
        AlterEmitOptions::default(),
    );
    assert!(
        result.is_none(),
        "empty diff must NOT emit a migration file"
    );
}

#[test]
fn fixture_1_nullable_add_emits_add_column_if_not_exists() {
    let diff = diff_with(
        vec![ColumnAdd {
            table: "customer".to_owned(),
            column: "profile_photo".to_owned(),
            sql_type: "JSONB".to_owned(),
            not_null: false,
            default: None,
        }],
        vec![],
        vec![],
    );
    let (up, _down) = emit(diff, AlterEmitOptions::default());

    assert!(
        up.contains("ALTER TABLE \"customer\" ADD COLUMN IF NOT EXISTS profile_photo JSONB;"),
        "expected `ADD COLUMN IF NOT EXISTS profile_photo JSONB;` in:\n{up}"
    );
    assert!(
        !up.contains("NOT NULL"),
        "nullable add must not emit NOT NULL:\n{up}"
    );
    assert!(
        !up.contains("TODO"),
        "nullable add needs no TODO marker:\n{up}"
    );
}

#[test]
fn fixture_2_not_null_with_default_emits_default() {
    let diff = diff_with(
        vec![ColumnAdd {
            table: "customer".to_owned(),
            column: "tier".to_owned(),
            sql_type: "TEXT".to_owned(),
            not_null: true,
            default: Some(AlterDefault::String("standard".to_owned())),
        }],
        vec![],
        vec![],
    );
    let (up, _down) = emit(diff, AlterEmitOptions::default());

    assert!(
        up.contains(
            "ALTER TABLE \"customer\" ADD COLUMN IF NOT EXISTS tier TEXT NOT NULL DEFAULT 'standard';"
        ),
        "expected NOT NULL + DEFAULT line in:\n{up}"
    );
    assert!(
        !up.contains("TODO"),
        "NOT NULL + default needs no TODO:\n{up}"
    );
}

#[test]
fn fixture_3_not_null_without_default_degrades_and_marks_todo() {
    let diff = diff_with(
        vec![ColumnAdd {
            table: "customer".to_owned(),
            column: "needs_review".to_owned(),
            sql_type: "BOOLEAN".to_owned(),
            not_null: true,
            default: None,
        }],
        vec![],
        vec![],
    );
    let (up, _down) = emit(diff, AlterEmitOptions::default());

    // The column is emitted WITHOUT `NOT NULL` because adding it on a
    // populated table without a default would fail Postgres' constraint
    // check. A TODO comment tells the operator to backfill and then
    // SET NOT NULL manually.
    assert!(
        up.contains("ALTER TABLE \"customer\" ADD COLUMN IF NOT EXISTS needs_review BOOLEAN;"),
        "expected nullable shape (no NOT NULL) in:\n{up}"
    );
    assert!(
        !up.contains("needs_review BOOLEAN NOT NULL"),
        "must not emit literal `NOT NULL` on a column with no default:\n{up}"
    );
    assert!(
        up.contains("TODO: backfill"),
        "expected `TODO: backfill` marker in:\n{up}"
    );
    assert!(
        up.contains("SET NOT NULL"),
        "expected SET NOT NULL follow-up note in:\n{up}"
    );
}

#[test]
fn fixture_4_drops_without_flag_are_commented_and_warned() {
    let diff = diff_with(
        vec![],
        vec![ColumnDrop {
            table: "customer".to_owned(),
            column: "legacy_score".to_owned(),
            previous_sql_type: "BIGINT".to_owned(),
            previous_not_null: false,
            previous_default: None,
        }],
        vec![],
    );
    let (up, _down) = emit(diff, AlterEmitOptions { allow_drops: false });

    assert!(
        up.contains("-- WARNING:"),
        "expected WARNING header when drops are commented out:\n{up}"
    );
    assert!(
        up.contains("--allow-drops"),
        "expected hint about the --allow-drops flag in:\n{up}"
    );
    assert!(
        up.contains("-- ALTER TABLE \"customer\" DROP COLUMN IF EXISTS legacy_score;"),
        "expected drop to be COMMENTED OUT in:\n{up}"
    );
    // Live, non-commented DROP would be a regression — re-search for the
    // unprefixed form anchored to start-of-line.
    let live_drop = "\nALTER TABLE \"customer\" DROP COLUMN";
    assert!(
        !up.contains(live_drop),
        "drop must not be emitted as live SQL without --allow-drops:\n{up}"
    );
}

#[test]
fn fixture_5_drops_with_flag_emit_drop_column_if_exists() {
    let diff = diff_with(
        vec![],
        vec![ColumnDrop {
            table: "customer".to_owned(),
            column: "legacy_score".to_owned(),
            previous_sql_type: "BIGINT".to_owned(),
            previous_not_null: false,
            previous_default: None,
        }],
        vec![],
    );
    let (up, down) = emit(diff, AlterEmitOptions { allow_drops: true });

    assert!(
        up.contains("ALTER TABLE \"customer\" DROP COLUMN IF EXISTS legacy_score;"),
        "expected live DROP COLUMN in:\n{up}"
    );
    assert!(
        !up.contains("-- WARNING:"),
        "WARNING header must not appear when drops are live:\n{up}"
    );
    // Down should re-add the column with its previous shape.
    assert!(
        down.contains("ALTER TABLE \"customer\" ADD COLUMN IF NOT EXISTS legacy_score BIGINT;"),
        "down migration must re-add the dropped column:\n{down}"
    );
}

#[test]
fn fixture_6_text_to_jsonb_type_change_uses_safe_cast() {
    let diff = diff_with(
        vec![],
        vec![],
        vec![TypeChange {
            table: "customer".to_owned(),
            column: "metadata".to_owned(),
            previous_sql_type: "TEXT".to_owned(),
            new_sql_type: "JSONB".to_owned(),
        }],
    );
    let (up, down) = emit(diff, AlterEmitOptions::default());

    assert!(
        up.contains(
            "ALTER TABLE \"customer\" ALTER COLUMN metadata TYPE JSONB USING metadata::jsonb;"
        ),
        "expected safe TEXT->JSONB cast with USING clause in:\n{up}"
    );
    assert!(
        !up.contains("-- WARNING:"),
        "TEXT->JSONB is in the safe catalog; no WARNING expected:\n{up}"
    );
    // Inverse cast should appear in the down migration.
    assert!(
        down.contains(
            "ALTER TABLE \"customer\" ALTER COLUMN metadata TYPE TEXT USING metadata::text;"
        ),
        "down migration must emit the inverse JSONB->TEXT cast:\n{down}"
    );
}

#[test]
fn unsafe_type_change_is_commented_out_with_warning() {
    // Decimal -> Integer is destructive (truncation). The emitter must
    // refuse to apply it automatically and surface a WARNING + commented
    // line so the operator decides what to do.
    let diff = diff_with(
        vec![],
        vec![],
        vec![TypeChange {
            table: "customer".to_owned(),
            column: "credits".to_owned(),
            previous_sql_type: "NUMERIC(20,4)".to_owned(),
            new_sql_type: "BIGINT".to_owned(),
        }],
    );
    let (up, _down) = emit(diff, AlterEmitOptions::default());

    assert!(up.contains("-- WARNING:"), "expected WARNING header:\n{up}");
    assert!(
        up.contains("-- ALTER TABLE \"customer\" ALTER COLUMN credits TYPE BIGINT"),
        "expected commented-out unsafe cast in:\n{up}"
    );
}

#[test]
fn down_migration_inverse_for_pure_add() {
    let diff = diff_with(
        vec![ColumnAdd {
            table: "customer".to_owned(),
            column: "profile_photo".to_owned(),
            sql_type: "JSONB".to_owned(),
            not_null: false,
            default: None,
        }],
        vec![],
        vec![],
    );
    let (_up, down) = emit(diff, AlterEmitOptions::default());

    // Inverse of an ADD COLUMN is DROP COLUMN IF EXISTS — safe regardless
    // of NOT NULL because we degrade NOT NULL on the up side anyway.
    assert!(
        down.contains("ALTER TABLE \"customer\" DROP COLUMN IF EXISTS profile_photo;"),
        "down for an ADD must DROP IF EXISTS:\n{down}"
    );
}

#[test]
fn deterministic_ordering_when_diff_mixes_kinds() {
    // The orchestrator's diff may produce sets in any order. The emitter
    // must sort by column name so byte-identical output across runs is
    // preserved (downstream golden tests rely on this).
    let diff = diff_with(
        vec![
            ColumnAdd {
                table: "customer".to_owned(),
                column: "zebra".to_owned(),
                sql_type: "TEXT".to_owned(),
                not_null: false,
                default: None,
            },
            ColumnAdd {
                table: "customer".to_owned(),
                column: "alpha".to_owned(),
                sql_type: "BIGINT".to_owned(),
                not_null: false,
                default: None,
            },
        ],
        vec![],
        vec![],
    );
    let (up_a, _) = emit(diff.clone(), AlterEmitOptions::default());
    let (up_b, _) = emit(diff, AlterEmitOptions::default());
    assert_eq!(up_a, up_b, "emit must be deterministic across runs");
    // `alpha` must precede `zebra` regardless of input order.
    let alpha_pos = up_a.find("alpha").expect("alpha column missing");
    let zebra_pos = up_a.find("zebra").expect("zebra column missing");
    assert!(
        alpha_pos < zebra_pos,
        "columns must be sorted alphabetically in the emit"
    );
}
