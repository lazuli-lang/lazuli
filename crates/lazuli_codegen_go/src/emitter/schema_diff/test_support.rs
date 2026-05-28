//! Shared test fixtures for `schema_diff` submodules.
//!
//! Test-only; not part of any public surface. Each child file
//! (`mod.rs`, `parse.rs`, `ir.rs`) imports the helpers it needs
//! through `super::test_support::*`. Kept tiny so the inline-test
//! discipline isn't undermined.

use super::Column;
use lazuli_ir::{Field, FieldConstraints, Resource, TypeRef};

pub(super) fn col(name: &str, sql_type: &str, nullable: bool) -> Column {
    Column::new(name, sql_type, nullable)
}

pub(super) fn empty_field(name: &str, type_ref: TypeRef, required: bool) -> Field {
    Field {
        name: name.to_owned(),
        type_ref,
        required,
        unique: false,
        slug: false,
        default: None,
        derived_from: None,
        computed_date: None,
        constraints: FieldConstraints::default(),
        full_text: false,
        previous_names: Vec::new(),
        pii: None,
        owner_axis: None,
        cross_feature_target: None,
        span_ref: None,
    }
}

pub(super) fn empty_resource(name: &str, fields: Vec<Field>) -> Resource {
    Resource {
        name: name.to_owned(),
        public_contract: None,
        tenancy: None,
        soft_delete: false,
        timestamps: Some(true),
        fields,
        constraints: Vec::new(),
        validate: None,
        validates: Vec::new(),
        retention: None,
        previous_names: Vec::new(),
        span_ref: None,
        lifecycle: None,
        invariants: Vec::new(),
        lock: None,
        composite_key: None,
        conventions: Vec::new(),
        lifecycle_routes: None,
        polymorphic_refs: Vec::new(),
        many_through: Vec::new(),
        append_only: false,
    }
}
