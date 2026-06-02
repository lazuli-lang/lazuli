//! Projection of an IR `Resource` into a `ResourceSchema` for diffing.
//!
//! Mirrors the type mapping used by `migration_ddl.rs`
//! (`pg_type_for` / `pg_type_for_builtin`). We deliberately do NOT
//! do FK resolution here — the cell A11 ALTER emitter never
//! re-targets a foreign key without an explicit migration plan, so
//! columns whose type is `TypeRef::UserDefined` that would
//! normally lower to `BIGINT` are emitted as their nominal IR
//! mapping. Callers that need the cross-feature view stay on
//! `migration_ddl::pg_type_for_field`.

use lazuli_ir::{BuiltinType, CapabilityRef, DefaultValue, Field, Resource, TypeRef};

use super::{Column, ResourceSchema};

/// Project an IR resource into a column-level `ResourceSchema`.
/// Mirrors the type mapping in `migration_ddl::pg_type_for` /
/// `pg_type_for_builtin` so the diff against a freshly emitted
/// migration is identity.
///
/// We capture:
/// - the implicit `id BIGSERIAL PRIMARY KEY` row identity,
/// - each IR `Field` lowered through the same mapping,
/// - the implicit `created_at` / `updated_at` columns when the
///   resource opts in to timestamps (the IR `Resource.timestamps`
///   axis is `Option<bool>`; `None` and `Some(true)` both opt-in
///   — feature-level defaults are resolved upstream),
/// - the `deleted_at TIMESTAMPTZ` column when `soft_delete`, plus a
///   `deleted_by BIGINT` actor column when `soft_delete by` (spec 0015).
///
/// We deliberately do NOT capture:
/// - The `org_id BIGINT NOT NULL` tenancy column. That requires
///   `Module + Feature` to resolve tenancy inheritance; A11's
///   first-cut ALTER emitter restricts itself to the resource-
///   intrinsic columns and A12's doctor lint warns when tenancy
///   would have to change.
/// - The `<money>_currency` paired columns (same reason — needs
///   feature context).
/// - FK column type narrowing to `BIGINT` (needs `CrossFeatureIndex`).
/// - Generated-as expressions for `derived from` fields (these
///   never participate in ALTER TABLE; A11 skips them).
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_go::emitter::schema_diff::current_schema_from_ir;
/// let schema = current_schema_from_ir(&resource);
/// assert!(schema.columns.iter().any(|c| c.name == "id"));
/// ```
pub fn current_schema_from_ir(resource: &Resource) -> ResourceSchema {
    let mut columns = Vec::new();

    columns.push(Column::new(
        lazuli_ir::synthesized_columns::IDENTITY,
        "BIGSERIAL",
        false,
    ));

    for field in &resource.fields {
        if field.derived_from.is_some() {
            // Generated columns are never altered by hand; A11
            // emits a `DROP COLUMN` + `ADD GENERATED COLUMN` plan
            // through a different path (out of A10 scope).
            continue;
        }
        columns.push(field_to_column(field));
    }

    // `timestamps` defaults to opted-in unless the resource set
    // `Some(false)`. We don't have the feature here, so mirror the
    // resource-local default; A12 will warn if the feature default
    // disagrees.
    let timestamps_enabled = resource.timestamps.unwrap_or(true);
    if timestamps_enabled {
        // Column NAMES come from the IR single-source-of-truth
        // (`synthesized_columns`) so the doctor's universal-column filter
        // and this schema-diff projection can never disagree on the
        // synthesized names. `[created_at, updated_at]` by construction.
        for name in lazuli_ir::synthesized_columns::TIMESTAMPS {
            columns.push(Column::new(*name, "TIMESTAMPTZ", false).with_default("NOW()"));
        }
    }

    // Resource-relative soft-delete column set (`[]` / `[deleted_at]` /
    // `[deleted_at, deleted_by]`) from the IR SoT. Spec 0015 — the
    // `soft_delete by` actor form adds `deleted_by`; mirroring the SoT
    // here keeps `@correctness.migration_out_of_sync` from flagging the
    // emitted column as "migration-only".
    for name in resource.synthesized_soft_delete_columns() {
        let sql_type = if *name == "deleted_by" {
            "BIGINT"
        } else {
            "TIMESTAMPTZ"
        };
        columns.push(Column::new(*name, sql_type, true));
    }

    ResourceSchema { columns }
}

fn field_to_column(field: &Field) -> Column {
    let sql_type = ir_pg_type(&field.type_ref);
    let nullable = !field.required;
    let default = field.default.as_ref().map(render_default);
    Column {
        name: field.name.clone(),
        sql_type,
        nullable,
        default,
    }
}

/// Subset of `migration_ddl::pg_type_for` that operates without a
/// `Module` / `Feature` / `CrossFeatureIndex`. The cell-scope
/// signature `current_schema_from_ir(resource)` is intentionally
/// narrower than the main codegen path; the trade-off is that FK
/// types stay at their nominal mapping (e.g. a `UserDefined`
/// resource ref renders as `TEXT`). A11's caller-side wiring will
/// upgrade this to the cross-feature view if doctor (A12) flags a
/// false-positive type change against an FK column.
fn ir_pg_type(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Builtin(builtin) => ir_pg_type_for_builtin(builtin),
        TypeRef::Many(inner) => format!("{}[]", ir_pg_type(inner)),
        TypeRef::Capability(cap) => ir_pg_type_for_capability(cap),
        TypeRef::UserDefined(_) | TypeRef::EnumRef(_) | TypeRef::Unresolved(_) => "TEXT".to_owned(),
    }
}

fn ir_pg_type_for_builtin(builtin: &BuiltinType) -> String {
    match builtin {
        BuiltinType::Id => "BIGINT".to_owned(),
        BuiltinType::Text => "TEXT".to_owned(),
        BuiltinType::Boolean => "BOOLEAN".to_owned(),
        BuiltinType::Integer => "BIGINT".to_owned(),
        BuiltinType::Decimal => "NUMERIC(20, 6)".to_owned(),
        BuiltinType::Date => "DATE".to_owned(),
        BuiltinType::DateTime => "TIMESTAMPTZ".to_owned(),
        BuiltinType::Json => "JSONB".to_owned(),
        BuiltinType::SemanticEmail
        | BuiltinType::SemanticPhone
        | BuiltinType::SemanticUrl
        | BuiltinType::SemanticUuid
        | BuiltinType::SemanticCurrency
        // W1 GAP-04 — HexColor is a text carrier.
        | BuiltinType::SemanticHexColor => "TEXT".to_owned(),
        // W1 GAP-05 — Percentage mirrors Decimal's NUMERIC precision.
        BuiltinType::SemanticPercentage => "NUMERIC(20, 6)".to_owned(),
        // Batch E — PositiveDecimal mirrors Decimal; NonNegativeInt mirrors Integer.
        BuiltinType::SemanticPositiveDecimal => "NUMERIC(20, 6)".to_owned(),
        BuiltinType::SemanticNonNegativeInt => "BIGINT".to_owned(),
        BuiltinType::SemanticMoney { .. } => "NUMERIC(20,4)".to_owned(),
        BuiltinType::SemanticGeoPoint => "geography(point, 4326)".to_owned(),
        BuiltinType::SemanticPluginType { carrier, .. } => ir_pg_type_for_builtin(carrier),
        BuiltinType::CapSecret => "TEXT".to_owned(),
        BuiltinType::CapFile => "JSONB".to_owned(),
    }
}

fn ir_pg_type_for_capability(capability: &CapabilityRef) -> String {
    match capability {
        CapabilityRef::Encrypted(_) | CapabilityRef::E2ee(_) => "BYTEA".to_owned(),
        CapabilityRef::File(_) => "JSONB".to_owned(),
        _ => "TEXT".to_owned(),
    }
}

fn render_default(default: &DefaultValue) -> String {
    match default {
        DefaultValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        DefaultValue::Integer(i) => i.to_string(),
        DefaultValue::Boolean(b) => if *b { "true" } else { "false" }.to_owned(),
        DefaultValue::EnumLiteral(lit) => format!("'{}'", lit.variant),
        DefaultValue::Nil => "NULL".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::{diff, parse_baseline_from_str};
    use super::*;
    use crate::emitter::schema_diff::test_support::{empty_field, empty_resource};

    #[test]
    fn current_schema_emits_implicit_id_and_timestamps() {
        let resource = empty_resource(
            "Host",
            vec![empty_field(
                "name",
                TypeRef::Builtin(BuiltinType::Text),
                true,
            )],
        );
        let schema = current_schema_from_ir(&resource);
        let names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "name", "created_at", "updated_at"]);
    }

    #[test]
    fn soft_delete_by_includes_deleted_by_in_schema() {
        // Spec 0015 — the schema-diff expected-column set must include
        // `deleted_by` for `soft_delete by`, mirroring the create_table
        // DDL emitter; otherwise `@correctness.migration_out_of_sync`
        // false-flags the emitted column as "migration-only".
        let mut resource = empty_resource(
            "Host",
            vec![empty_field(
                "name",
                TypeRef::Builtin(BuiltinType::Text),
                true,
            )],
        );
        resource.soft_delete = true;
        resource.soft_delete_actor = true;
        let schema = current_schema_from_ir(&resource);
        let names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"deleted_at"), "{names:?}");
        assert!(names.contains(&"deleted_by"), "{names:?}");

        // Bare soft_delete: only deleted_at, no deleted_by.
        resource.soft_delete_actor = false;
        let bare = current_schema_from_ir(&resource);
        let bare_names: Vec<&str> = bare.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(bare_names.contains(&"deleted_at"), "{bare_names:?}");
        assert!(!bare_names.contains(&"deleted_by"), "{bare_names:?}");
    }

    #[test]
    fn current_schema_round_trip_against_parser() {
        // The diff between a baseline parsed from a generated
        // migration AND the IR projection of the same resource
        // shape must be empty. This is the smoke test that says:
        // re-running `lazuli generate` on an unchanged spec emits
        // no ALTER plan.
        let resource = empty_resource(
            "Host",
            vec![
                empty_field("name", TypeRef::Builtin(BuiltinType::Text), true),
                empty_field("address", TypeRef::Builtin(BuiltinType::Text), false),
            ],
        );

        let sql = r#"
CREATE TABLE IF NOT EXISTS "host" (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
"#;
        let baseline = parse_baseline_from_str(sql).expect("parse ok");
        let current = current_schema_from_ir(&resource);

        // The parser captures `id BIGSERIAL` (no PRIMARY KEY in
        // the type string, that's a structural suffix), and the
        // IR projection emits `BIGSERIAL` — same string.
        let out = diff(&baseline, &current);
        assert!(
            out.is_empty(),
            "expected empty round-trip diff, got {out:?}"
        );
    }

    #[test]
    fn current_schema_emits_profile_photo_jsonb_for_jsonb_field() {
        // Concrete the canonical pilot regression case: a `profile_photo: Json`
        // field MUST project to a `JSONB` column so the A11
        // ALTER emitter doesn't loop adding/dropping the same column.
        let mut resource = empty_resource(
            "Host",
            vec![empty_field(
                "profile_photo",
                TypeRef::Builtin(BuiltinType::Json),
                false,
            )],
        );
        resource.timestamps = Some(false);
        let schema = current_schema_from_ir(&resource);
        let pp = schema
            .columns
            .iter()
            .find(|c| c.name == "profile_photo")
            .expect("profile_photo column present");
        assert_eq!(pp.sql_type, "JSONB");
        assert!(pp.nullable);
    }
}
