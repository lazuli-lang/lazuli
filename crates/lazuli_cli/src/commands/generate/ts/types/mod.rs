//! TypeScript type lowering for command/query slots and resource fields.
//!
//! Carved out of `ts/mod.rs` as part of Wave R6-5 (Rails-style refactor)
//! and re-split in R9 into sibling sub-files:
//!
//! - [`slots`]      — `TsSlot` extractors for commands + queries
//!   (`command_sdk_slots`, `command_input_slots`, `query_args`).
//! - [`resources`]  — resource lookups + field formatters
//!   (`find_resource`, `resource_ts_name`, `resource_field_ts_name`,
//!   `resource_field_ts_type`, `is_resource_ref`, `ts_type_for_type_ref`).
//! - [`output`]     — `command_output_ts_type` +
//!   `pick_query_resource_ts` query-subject inference (+ `pluralize_snake`).
//! - [`pure_read`]  — `command_is_pure_read` IR side-effect gate.
//!
//! The `TsSlot` shape stays declared here so siblings can refer to it
//! via `super::TsSlot`.

mod output;
mod pure_read;
mod resources;
mod slots;

pub(crate) use output::{command_output_ts_type, pick_query_resource_ts};
pub(crate) use pure_read::command_is_pure_read;
pub(crate) use resources::{
    find_resource, is_resource_ref, resource_field_ts_name, resource_field_ts_type,
    ts_type_for_type_ref,
};
pub(crate) use slots::{command_input_slots, command_sdk_slots, query_args};

#[derive(Clone)]
pub(crate) struct TsSlot {
    pub(crate) name: String,
    pub(crate) type_ref: lazuli_ir::TypeRef,
    pub(crate) required: bool,
    pub(crate) constraints: lazuli_ir::FieldConstraints,
}
