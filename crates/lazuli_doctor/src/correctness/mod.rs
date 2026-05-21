//! Doctor correctness rules — dangling references, type shadows, etc.
//!
//! Distinct from `vocab/` (Rule Zero / vocabulary fitness). Correctness
//! rules surface concrete bugs (typo, shape mismatch) rather than style
//! drift. Diagnostic severity is typically `error` in both strict and
//! production profiles.
//!
//! Full dispatch into `DoctorPackage::diagnostics()` is a separate cell;
//! each rule's `#[cfg(test)] mod tests` exercises the logic until then.

pub mod channel_payload_unresolved_001;
pub mod command_input_shadows_field_001;
pub mod composite_key_contract_001;
pub mod event_group_variant_type_001;
pub mod event_outbox_001;
pub mod full_text_type_001;
pub mod hook_target_001;
pub mod missing_policy_on_query_001;
pub mod mutation_without_readback;
pub mod record_column_storage;
pub mod resource_lock_contract_001;
pub mod route_id_effect_consistency;
pub mod schema_migration_present;
pub mod webhook_emit_predicate_field_001;
