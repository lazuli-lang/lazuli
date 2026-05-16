//! Doctor rules for the `encryption` vocabulary.
//!
//! Each rule is a sub-module exposing a `check` function. Full dispatch
//! into `DoctorPackage::diagnostics()` is a separate follow-up cell
//! (per the lifecycle/correctness pattern); each rule's inline
//! `#[cfg(test)] mod tests` exercises the logic until then.
//!
//! Per `docs/proposals/encryption-vocab.md` §Doctor diagnostics.

pub mod e2ee_event;
pub mod key_missing;
pub mod rotation;
pub mod source_env;
pub mod template_axis;
pub mod tenancy;

#[cfg(test)]
pub(crate) mod test_support;
