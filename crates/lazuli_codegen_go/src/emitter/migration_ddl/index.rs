//! `CREATE INDEX` / `DROP INDEX` emission for authored indexes.
//!
//! Three index flavors flow through this module:
//!
//! - **`@full_text` GIN tsvector** — `to_tsvector('english', col)` with
//!   suffix `_fts`. Reused by query lowering for `@@
//!   websearch_to_tsquery(...)` (Roadmap §1.5 CL.C.2).
//! - **Method-tagged b-tree / GIN / GIST** — explicit
//!   `index { method btree|gin|gist }` on a constraint block.
//!   Suffixes `_idx`, `_gin`, `_gist`.
//! - **Composite full-text** — multi-field `index { full_text true }`
//!   builds `to_tsvector('english', concat_ws(' ', a, b, ...))`.
//!
//! Index names are deterministic: `<table>_<field_slug>_<suffix>`
//! where `field_slug` is the snake-cased fields joined by `_`. This
//! ties the up-migration `CREATE INDEX` to the down-migration's
//! commented `DROP INDEX` hints rendered in `drop_table.rs`.

use std::fmt::Write;

use lazuli_ir::{
    CompareOp, EvalPredicate, Expr, IndexConstraint, IndexMethod, Predicate, Resource,
    UniqueConstraint,
};

use super::sql_builder::{lower_snake, quote_ident, sql_ident};

pub(super) fn authored_index_sql(table_name: &str, index: &IndexConstraint) -> Option<String> {
    if index.fields.is_empty() {
        return None;
    }
    let name = authored_index_name(table_name, index)?;
    if index.full_text {
        let expression = format!(
            "to_tsvector('english', concat_ws(' ', {}))",
            index
                .fields
                .iter()
                .map(|field| sql_ident(field))
                .collect::<Vec<_>>()
                .join(", ")
        );
        return Some(format!(
            "CREATE INDEX {} ON {} USING GIN ({});",
            sql_ident(&name),
            quote_ident(table_name),
            expression
        ));
    }
    let columns = index
        .fields
        .iter()
        .map(|field| sql_ident(field))
        .collect::<Vec<_>>()
        .join(", ");
    let using = index
        .method
        .map(|method| format!(" USING {}", index_method_sql(method)))
        .unwrap_or_default();
    Some(format!(
        "CREATE INDEX {} ON {}{} ({});",
        sql_ident(&name),
        quote_ident(table_name),
        using,
        columns
    ))
}

pub(super) fn authored_index_name(table_name: &str, index: &IndexConstraint) -> Option<String> {
    if index.fields.is_empty() {
        return None;
    }
    let field_slug = index
        .fields
        .iter()
        .map(|field| lower_snake(field))
        .collect::<Vec<_>>()
        .join("_");
    let suffix = if index.full_text {
        "fts"
    } else {
        match index.method {
            Some(IndexMethod::Gin) => "gin",
            Some(IndexMethod::Gist) => "gist",
            Some(IndexMethod::Btree) | None => "idx",
        }
    };
    Some(format!("{}_{}_{}", table_name, field_slug, suffix))
}

/// GAP-NEW-001 — emit a PARTIAL unique index for a `unique (...) when
/// <predicate>` constraint. Postgres supports the `WHERE` qualifier only
/// on indexes (not table `UNIQUE` constraints), so the conditional form
/// lowers to `CREATE UNIQUE INDEX ... WHERE <predicate>`.
///
/// Returns `None` for the unconditional form (`when == None`) — that case
/// stays a table-level `UNIQUE (...)` clause emitted by `constraint.rs`.
/// Predicate shapes the renderer can't lower (`Unparsed`, non-comparison)
/// degrade gracefully: the index is still emitted over the columns and the
/// unsupported predicate is dropped into a trailing `-- WHERE` comment so
/// `go build` / `psql` never see invalid SQL.
pub(super) fn partial_unique_index_sql(
    table_name: &str,
    unique: &UniqueConstraint,
) -> Option<String> {
    let when = unique.when.as_ref()?;
    if unique.fields.is_empty() {
        return None;
    }
    let field_slug = unique
        .fields
        .iter()
        .map(|field| lower_snake(field))
        .collect::<Vec<_>>()
        .join("_");
    let name = format!("{}_{}_uidx", table_name, field_slug);
    let columns = unique
        .fields
        .iter()
        .map(|field| sql_ident(field))
        .collect::<Vec<_>>()
        .join(", ");
    match eval_predicate_sql(when) {
        Some(predicate_sql) => Some(format!(
            "CREATE UNIQUE INDEX {} ON {} ({}) WHERE {};",
            sql_ident(&name),
            quote_ident(table_name),
            columns,
            predicate_sql,
        )),
        None => Some(format!(
            "CREATE UNIQUE INDEX {} ON {} ({}); -- WHERE clause unsupported (predicate not lowerable)",
            sql_ident(&name),
            quote_ident(table_name),
            columns,
        )),
    }
}

/// Render an [`EvalPredicate`] into a SQL boolean expression for a
/// partial-index `WHERE` clause. Only the closed `Comparison` shape over
/// a column path and a literal is lowered today (`is_default = true`,
/// `status != 'archived'`); richer shapes (`And`/`Or`/`Has`/`Contains`/
/// `Unparsed`) return `None` so the caller degrades to a non-partial
/// index plus a comment. Kept wire-thin: no expression engine, just a
/// direct projection of the closed catalog.
///
/// GAP-NEW-002 — a `nil` operand can't render as a SQL value (`= NULL`
/// never matches in three-valued logic), so a comparison against `nil`
/// lowers to the `IS NULL` / `IS NOT NULL` predicate form instead. This
/// makes soft-delete-aware uniqueness (`unique name when deleted_at ==
/// nil`) emit a partial `WHERE deleted_at IS NULL` index rather than
/// degrading to a full unique index that would block name reuse after a
/// soft-delete.
fn eval_predicate_sql(pred: &EvalPredicate) -> Option<String> {
    let EvalPredicate::Closed(Predicate::Comparison { left, op, right }) = pred else {
        return None;
    };
    // `<col> == nil` / `<col> != nil` (or the mirrored `nil == <col>`)
    // → `<col> IS NULL` / `<col> IS NOT NULL`.
    if let Some(null_sql) = nil_comparison_sql(left, *op, right) {
        return Some(null_sql);
    }
    let lhs = expr_sql(left)?;
    let rhs = expr_sql(right)?;
    Some(format!("{} {} {}", lhs, compare_op_sql(*op), rhs))
}

/// Lower a comparison whose other operand is `nil` into the SQL `IS NULL`
/// / `IS NOT NULL` form. Only equality (`==` → `IS NULL`) and inequality
/// (`!=` → `IS NOT NULL`) are meaningful against null; ordering ops
/// (`<`, `>=`, ...) against `nil` are not lowerable and return `None`
/// (caller degrades to a non-partial index). Handles `nil` on either
/// side so `deleted_at == nil` and `nil == deleted_at` both work.
fn nil_comparison_sql(left: &Expr, op: CompareOp, right: &Expr) -> Option<String> {
    let column = match (left, right) {
        (Expr::Nil, other) | (other, Expr::Nil) => expr_sql(other)?,
        _ => return None,
    };
    let predicate = match op {
        CompareOp::Eq => "IS NULL",
        CompareOp::Ne => "IS NOT NULL",
        _ => return None,
    };
    Some(format!("{} {}", column, predicate))
}

fn compare_op_sql(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Eq => "=",
        CompareOp::Ne => "!=",
        CompareOp::Lt => "<",
        CompareOp::Le => "<=",
        CompareOp::Gt => ">",
        CompareOp::Ge => ">=",
    }
}

/// Project a predicate [`Expr`] leaf into SQL. A single-segment path is a
/// column reference (quoted via `sql_ident`); string/int/bool literals
/// render as SQL literals. Multi-segment paths, enum constants, nil, and
/// `@fn(...)` calls aren't valid in a static partial-index predicate, so
/// they return `None` (caller drops to a non-partial index).
fn expr_sql(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path) if path.segments.len() == 1 => Some(sql_ident(&path.segments[0])),
        Expr::String(s) => Some(format!("'{}'", s.replace('\'', "''"))),
        Expr::Integer(n) => Some(n.to_string()),
        Expr::Boolean(b) => Some(if *b { "true".into() } else { "false".into() }),
        _ => None,
    }
}

pub(super) fn index_method_sql(method: IndexMethod) -> &'static str {
    match method {
        IndexMethod::Btree => "BTREE",
        IndexMethod::Gin => "GIN",
        IndexMethod::Gist => "GIST",
    }
}

/// GAP-12 — emit a btree index per `target @feature.<feature>.<Resource>`
/// cross-feature FK field. The reference is *logical*: codegen does NOT
/// emit a hard `FOREIGN KEY` because the target table is owned by another
/// feature's migration set (the topo-sorted FK ordering in `topo.rs` only
/// guarantees ordering for same-module same-set FKs). A btree index keeps
/// the join column fast and documents the logical reference via a comment;
/// referential integrity across the boundary is the runtime's concern (and
/// becomes a hard FK only once the target migration set is co-ordered).
pub(super) fn cross_feature_target_indexes(table_name: &str, resource: &Resource) -> Vec<String> {
    resource
        .fields
        .iter()
        .filter_map(|field| {
            let target = field.cross_feature_target.as_ref()?;
            let name = format!("{}_{}_fkidx", table_name, lower_snake(&field.name));
            Some(format!(
                "CREATE INDEX {} ON {} ({}); -- logical FK -> {}.{} (cross-feature; no hard constraint)",
                sql_ident(&name),
                quote_ident(table_name),
                sql_ident(&field.name),
                target.feature,
                target.resource,
            ))
        })
        .collect()
}

/// GAP-13 — emit a composite btree index on `(type_field, id_field)` per
/// `polymorphic_ref` declaration so discriminated-FK lookups (`WHERE
/// entity_type = $1 AND entity_id = $2`) stay fast. No hard FK is emitted
/// (the referent is one of N tables); the discriminator CHECK lives on the
/// `type_field` column (see `constraint::polymorphic_ref_columns`).
pub(super) fn polymorphic_ref_indexes(table_name: &str, resource: &Resource) -> Vec<String> {
    resource
        .polymorphic_refs
        .iter()
        .map(|pref| {
            let name = format!(
                "{}_{}_{}_pidx",
                table_name,
                lower_snake(&pref.type_field),
                lower_snake(&pref.id_field),
            );
            format!(
                "CREATE INDEX {} ON {} ({}, {});",
                sql_ident(&name),
                quote_ident(table_name),
                sql_ident(&pref.type_field),
                sql_ident(&pref.id_field),
            )
        })
        .collect()
}

/// Emit the two `CREATE INDEX` lines that session-rotation tables
/// need. Sessions get an index on `parent_session_id` for the
/// rotation-chain walk and one on `refresh_token_hash` for the
/// "find the session this refresh token belongs to" lookup.
pub(super) fn emit_session_rotation_indexes(sql: &mut String, table: &str) {
    let _ = writeln!(sql);
    let _ = writeln!(
        sql,
        "CREATE INDEX ON {} (parent_session_id) WHERE parent_session_id IS NOT NULL;",
        quote_ident(table)
    );
    let _ = writeln!(sql);
    let _ = writeln!(
        sql,
        "CREATE INDEX ON {} (refresh_token_hash) WHERE refresh_token_hash != '';",
        quote_ident(table)
    );
}
