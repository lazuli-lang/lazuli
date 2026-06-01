//! UNIQUE / FOREIGN KEY constraint emission.
//!
//! Two flavors of UNIQUE coexist:
//!
//! - **Inline `unique` on a field** — `inline_unique_constraint_sql`
//!   walks `resource.fields` and emits one `UNIQUE (<field>[, org_id])`
//!   per `field.unique == true`. Org-tenanted resources get the
//!   tenancy column appended automatically so `User.email` is unique
//!   per org, not globally.
//! - **`constraint unique { fields per }` block** —
//!   `unique_constraint_sql` reads `Constraint::Unique` and emits the
//!   same `UNIQUE (...)` shape but with the explicit `per` slug.
//!
//! `foreign_key_constraints` emits one `FOREIGN KEY (<col>) REFERENCES
//! <target_table> (id)` per `UserDefined` field whose target resource
//! resolves to a declared table in the module. The target table name
//! is the bare snake-case of `Resource.name`, matching what
//! `create_table.rs` emits and what the Go runtime references via
//! `lazuli.Resource[T].Name`.
//!
//! `composite_key_sql` renders the optional `composite_key` block as
//! either `PRIMARY KEY (...)` (`primary true`) or `UNIQUE (...)`
//! (`primary false`).

use lazuli_ir::{Constraint, Feature, Module, Resource, Tenancy, TypeRef};

use super::super::cross_feature::CrossFeatureIndex;
use super::index::partial_unique_index_name;
use super::sql_builder::{lower_snake, quote_ident, sql_ident};
use super::sql_column::SqlColumn;
use super::topo::foreign_key_owner;

/// Walk a resource's `unique ... error <CODE>` constraints and return one
/// `(constraint_name, domain_code)` pair per coded unique. The constraint
/// name is the SAME deterministic string the DDL emits (`<table>_<slug>_key`
/// for the unconditional table constraint, `<table>_<slug>_uidx` for the
/// partial `when` index), so the runtime `RegisterUniqueViolationCode` key is
/// guaranteed byte-identical to `pgErr.ConstraintName`. This shared helper is
/// the single source of truth for that name — codegen glue and DDL never
/// compute it independently, so they cannot drift.
pub(crate) fn unique_violation_codes(resource: &Resource) -> Vec<(String, String)> {
    let table_name = lower_snake(&resource.name);
    resource
        .constraints
        .iter()
        .filter_map(|constraint| {
            let Constraint::Unique(unique) = constraint else {
                return None;
            };
            let code = unique.error_code.as_ref()?;
            if unique.fields.is_empty() {
                return None;
            }
            let name = if unique.when.is_some() {
                partial_unique_index_name(&table_name, &unique.fields)
            } else {
                unique_constraint_name(&table_name, &unique.fields)
            };
            Some((name, code.clone()))
        })
        .collect()
}

pub(super) fn unique_constraint_sql(table_name: &str, constraint: &Constraint) -> Option<SqlColumn> {
    let Constraint::Unique(unique) = constraint else {
        return None;
    };
    // GAP-NEW-001 — conditional uniques (`when <predicate>`) can't be a
    // table-level `UNIQUE (...)` clause (Postgres only honors `WHERE` on
    // indexes). They're emitted separately as `CREATE UNIQUE INDEX ...
    // WHERE` via `index::partial_unique_index_sql`; skip them here so the
    // table body doesn't carry a duplicate (and invalid) clause.
    if unique.when.is_some() {
        return None;
    }

    // Per-constraint domain code (`unique ... error <CODE>`) requires a
    // DETERMINISTICALLY-named constraint so the runtime `ConstraintName`
    // match is reliable — an anonymous `UNIQUE (...)` would get a Postgres
    // auto-generated name codegen cannot predict. The name is the same
    // `<table>_<field_slug>_key` form Postgres itself uses, so this is a
    // no-op rename for existing anonymous constraints.
    let name = unique
        .error_code
        .as_ref()
        .map(|_| unique_constraint_name(table_name, &unique.fields));

    unique_fields_sql(
        unique.fields.iter().map(String::as_str),
        unique.per.as_deref(),
        name.as_deref(),
    )
}

/// Deterministic name for a table-level UNIQUE constraint:
/// `<table>_<field_slug>_key` (the same suffix Postgres auto-uses). Shared
/// by the DDL emitter and the runtime registration glue so the emitted
/// constraint name and the registered `ConstraintName -> domain code` key
/// are guaranteed identical — the load-bearing correctness property for the
/// 23505 remap.
pub(super) fn unique_constraint_name(table_name: &str, fields: &[String]) -> String {
    let field_slug = fields
        .iter()
        .map(|field| lower_snake(field))
        .collect::<Vec<_>>()
        .join("_");
    format!("{table_name}_{field_slug}_key")
}

pub(super) fn inline_unique_constraint_sql(
    resource: &Resource,
    tenancy: &Tenancy,
) -> Vec<SqlColumn> {
    let per = if matches!(tenancy, Tenancy::Org) {
        Some("Org")
    } else {
        None
    };
    resource
        .fields
        .iter()
        .filter(|field| field.unique)
        .filter_map(|field| unique_fields_sql(std::iter::once(field.name.as_str()), per, None))
        .collect()
}

pub(super) fn unique_fields_sql<'a>(
    fields: impl IntoIterator<Item = &'a str>,
    per: Option<&str>,
    constraint_name: Option<&str>,
) -> Option<SqlColumn> {
    let mut fields: Vec<String> = fields.into_iter().map(sql_ident).collect();
    if fields.is_empty() {
        return None;
    }
    if let Some(per) = per {
        fields.push(sql_ident(&format!("{}_id", lower_snake(per))));
    }
    // A named constraint (`CONSTRAINT <name> UNIQUE (...)`) is emitted only
    // when a per-constraint domain code pins it; otherwise the anonymous
    // form is kept for byte-stable back-compat.
    let prefix = match constraint_name {
        Some(name) => format!("CONSTRAINT {} ", sql_ident(name)),
        None => String::new(),
    };
    Some(SqlColumn::raw(&format!(
        "{prefix}UNIQUE ({})",
        fields.join(", ")
    )))
}

/// Roadmap §1.5 (CL.C.2) — render the `composite_key` block as either a
/// `PRIMARY KEY (<fields>)` clause (when `primary true`) or a
/// `UNIQUE (<fields>)` constraint (when `primary false`). The
/// `Resource.lock` + `Resource.composite_key` axes are independent;
/// this helper only knows about composite key emission.
pub(super) fn composite_key_sql(resource: &Resource) -> Option<SqlColumn> {
    let ck = resource.composite_key.as_ref()?;
    if ck.fields.is_empty() {
        return None;
    }
    let cols: Vec<String> = ck.fields.iter().map(|f| sql_ident(f)).collect();
    let clause = if ck.primary {
        format!("PRIMARY KEY ({})", cols.join(", "))
    } else {
        format!("UNIQUE ({})", cols.join(", "))
    };
    Some(SqlColumn::raw(&clause))
}

/// GAP-13 — render the columns + CHECK for every `polymorphic_ref` on a
/// resource. Each declaration contributes:
///   - `<type_field> TEXT NOT NULL CHECK (<type_field> IN ('A','B',...))`
///     — the discriminator, constrained to the closed target-name set.
///   - `<id_field> BIGINT NOT NULL` — the row id of whichever target the
///     discriminator names.
/// No `FOREIGN KEY` is emitted: a polymorphic referent can point at any
/// of N tables, which a single SQL FK cannot express. The composite
/// `(type_field, id_field)` index is emitted separately by
/// `index::polymorphic_ref_indexes`.
pub(super) fn polymorphic_ref_columns(resource: &Resource) -> Vec<SqlColumn> {
    let mut columns = Vec::new();
    for pref in &resource.polymorphic_refs {
        let type_col = sql_ident(&pref.type_field);
        let allowed = pref
            .targets
            .iter()
            .map(|t| format!("'{}'", t.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");
        columns.push(SqlColumn::raw(&format!(
            "{type_col} TEXT NOT NULL CHECK ({type_col} IN ({allowed}))"
        )));
        columns.push(SqlColumn::raw(&format!(
            "{} BIGINT NOT NULL",
            sql_ident(&pref.id_field)
        )));
    }
    columns
}

pub(super) fn foreign_key_constraints<'a>(
    module: &'a Module,
    feature: &'a Feature,
    resource: &'a Resource,
    cross_index: &CrossFeatureIndex<'a>,
) -> Vec<SqlColumn> {
    resource
        .fields
        .iter()
        .filter_map(|field| {
            let TypeRef::UserDefined(qname) = &field.type_ref else {
                return None;
            };
            // `foreign_key_owner` returns `Some` only when the target
            // resource is declared in a different feature of the same
            // module — used here purely to confirm the FK target's
            // migration will exist before we point at it.
            let _owner = foreign_key_owner(module, feature, qname, cross_index)?;
            // FK target = the bare table name emitted by
            // `emit_resource_migration` (line 88) and stored on
            // `lazuli.Resource[T].Name` (consumed by the Go runtime in
            // `handle.go` `INSERT INTO eff.Resource.Name`). The
            // previous `<feature>_<resource>` form drifted from those
            // call sites and produced FKs pointing to non-existent
            // tables — migrations would refuse to apply.
            let target_table = lower_snake(&qname.name);
            Some(SqlColumn::raw(&format!(
                "FOREIGN KEY ({}) REFERENCES {} (id)",
                sql_ident(&field.name),
                quote_ident(&target_table)
            )))
        })
        .collect()
}
