//! Vocabulary lint rules (`VOCAB-*`).
//!
//! Each rule is a sub-module exposing a `check` function. Full dispatch
//! into `DoctorPackage::diagnostics()` is a separate cell (~+500 LOC of
//! IR loading + Finding → DoctorDiagnostic adapter). Until that ships,
//! each rule's inline `#[cfg(test)] mod tests` exercises the logic.
//!
//! v0.1 (4 rules): audit, derived_read, event_payload, union.
//! v0.2 catalog (3 rules): cap_missing, grammar_form, union_002.
//! v0.3 catalog (4 rules): event_orphan, event_producer, audit_002,
//! json_typed.
//! v0.4 catalog (2 rules): handler_heavy, tests_missing.
//! v0.5 catalog (1 rule): money_multi_currency.
//! v0.6 shared helpers: universal_columns (used by shadow-record /
//! resource-wide-cluster lints landing in subsequent cells).

pub mod conventions;
pub mod owner_axis;
pub mod rate_limit;
pub mod money_arithmetic_001;
pub mod money_compare_001;
pub mod universal_columns;
pub mod vocab_audit_001;
pub mod vocab_audit_002;
pub mod vocab_cap_missing_001;
pub mod vocab_derived_read_001;
pub mod vocab_event_orphan_001;
pub mod vocab_event_payload_001;
pub mod vocab_event_producer_001;
pub mod vocab_grammar_form_001;
pub mod vocab_handler_heavy_001;
pub mod vocab_json_typed_001;
pub mod vocab_money_multi_currency_001;
pub mod vocab_resource_wide_cluster_001;
pub mod vocab_shadow_record_001;
pub mod vocab_tests_missing_001;
pub mod vocab_union_001;
pub mod vocab_union_002;
