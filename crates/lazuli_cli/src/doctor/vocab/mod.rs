//! Vocabulary lint rules (`VOCAB-*`).
//!
//! Each rule is a sub-module exposing a `check` function. The orchestrator
//! wires each `check` into `DoctorPackage::diagnostics()` post-merge.

pub mod vocab_audit_001;
pub mod vocab_cap_missing_001;
pub mod vocab_derived_read_001;
pub mod vocab_event_payload_001;
pub mod vocab_union_001;
