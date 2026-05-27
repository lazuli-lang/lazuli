//! Doctor rules for the `lifecycle` vocabulary (L0 #7).
//!
//! Each rule is a sub-module exposing a `check` function. Full dispatch
//! into `DoctorPackage::diagnostics()` is a separate follow-up cell (~+50
//! LOC adapter); each rule's inline `#[cfg(test)] mod tests` exercises
//! the logic until then.
//!
//! Per docs/proposals/lifecycle-vocab.md §5.

pub mod enum_duplicate;
pub mod field_double_declared;
pub mod initial_ambiguous;
pub mod invariant_catalog_mismatch;
pub mod invariant_param_unresolved;
pub mod no_initial_state;
pub mod no_jump_needs_linear;
pub mod policy_required;
pub mod state_duplicate;
pub mod terminal_has_outgoing;
pub mod timestamp_type;
pub mod transition_from_undeclared;
pub mod transition_to_undeclared;
pub mod unreachable_state;
