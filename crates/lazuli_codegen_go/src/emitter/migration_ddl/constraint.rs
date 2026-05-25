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
use super::sql_builder::{lower_snake, quote_ident, sql_ident};
use super::sql_column::SqlColumn;
use super::topo::foreign_key_owner;

pub(super) fn unique_constraint_sql(constraint: &Constraint) -> Option<SqlColumn> {
    let Constraint::Unique(unique) = constraint else {
        return None;
    };

    unique_fields_sql(
        unique.fields.iter().map(String::as_str),
        unique.per.as_deref(),
    )
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
        .filter_map(|field| unique_fields_sql(std::iter::once(field.name.as_str()), per))
        .collect()
}

pub(super) fn unique_fields_sql<'a>(
    fields: impl IntoIterator<Item = &'a str>,
    per: Option<&str>,
) -> Option<SqlColumn> {
    let mut fields: Vec<String> = fields.into_iter().map(sql_ident).collect();
    if fields.is_empty() {
        return None;
    }
    if let Some(per) = per {
        fields.push(sql_ident(&format!("{}_id", lower_snake(per))));
    }
    Some(SqlColumn::raw(&format!("UNIQUE ({})", fields.join(", "))))
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
