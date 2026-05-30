//! Analyzer unit tests.
//!
//! Extracted from `lib.rs` by R4-E. Each test module is kept as a sibling
//! file under `tests/` so raw-string indentation inside test fixtures stays
//! load-bearing-correct. The original top-level `mod tests` from lib.rs is
//! preserved here as `mod core`.
//!
//! R9-E follow-up — each module-body file keeps its original 4-space outer
//! indent verbatim because the raw-string fixtures inside are structural
//! and must not be dedented.

mod conventions_crud_synth_tests;
mod conventions_me_synth_tests;
mod conventions_owner_scope_synth_tests;
mod conventions_unknown_diagnostic_tests;
mod core;
mod field_constraint_lowering_tests;
mod query_compose_lowering_tests;
mod surface_lowering_tests;
