//! Doctor correctness rules — dangling references, type shadows, etc.
//!
//! Distinct from `vocab/` (Rule Zero / vocabulary fitness). Correctness
//! rules surface concrete bugs (typo, shape mismatch) rather than style
//! drift. Diagnostic severity is typically `error` in both strict and
//! production profiles.
//!
//! Full dispatch into `DoctorPackage::diagnostics()` is a separate cell;
//! each rule's `#[cfg(test)] mod tests` exercises the logic until then.

pub mod command_input_shadows_field_001;
pub mod hook_target_001;
