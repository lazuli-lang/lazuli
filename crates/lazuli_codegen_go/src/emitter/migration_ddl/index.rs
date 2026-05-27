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

use lazuli_ir::{IndexConstraint, IndexMethod};

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

pub(super) fn index_method_sql(method: IndexMethod) -> &'static str {
    match method {
        IndexMethod::Btree => "BTREE",
        IndexMethod::Gin => "GIN",
        IndexMethod::Gist => "GIST",
    }
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
