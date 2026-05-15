//! Doctor rules for the `poller` vocabulary (L0 #8).
//!
//! Each rule is a sub-module exposing a `check` function. Full dispatch
//! into `DoctorPackage::diagnostics()` is a separate follow-up cell; each
//! rule's inline `#[cfg(test)] mod tests` exercises the logic until then.
//!
//! Per docs/proposals/poller-vocab.md §5.

pub mod cursor_missing_001;
pub mod dual_scheduler_001;
pub mod handler_orphan_001;
pub mod max_retries_unbounded_001;
pub mod no_terminal_001;
pub mod terminal_field_enum_001;
