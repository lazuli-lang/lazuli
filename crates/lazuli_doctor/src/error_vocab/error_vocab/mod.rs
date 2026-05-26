//! IR Error-Vocab — 7 typed analyzer cross-checks (Cell ANALYZE-1).
//!
//! Each rule is a self-contained `check_*` function in its own
//! `rule_<code>.rs` module file. The doctor pipeline
//! (`lazuli_cli::doctor::aggregators::error_vocab`) adapts findings to
//! `DoctorDiagnostic` and routes the per-rule severity. The LSP can
//! mount the same checks for live diagnostics in a later cell.
//!
//! ## Module layout
//!
//! - `catalogs.rs` — closed catalogs (`FRAMEWORK_ERROR_CODES`,
//!   `EXPOSE_4XX_FIELDS`, `EXPOSE_5XX_FIELDS`) and the shared
//!   `has_policy_denied_catchall` helper.
//! - `rule_policies_no_when_denied.rs` — ERR-VOCAB-001
//! - `rule_translation_key_unknown.rs` — ERR-VOCAB-002
//! - `rule_builtin_fallback.rs` — ERR-VOCAB-003
//! - `rule_code_unknown.rs` — ERR-VOCAB-CODE-UNKNOWN
//! - `rule_expose_unknown.rs` — ERR-VOCAB-EXPOSE-UNKNOWN
//! - `rule_when_denied_no_policy.rs` — ERR-VOCAB-WHEN-DENIED-NO-POLICY
//! - `rule_expose_5xx_message.rs` — ERR-VOCAB-EXPOSE-5XX-MESSAGE
//!
//! External callers continue to write
//! `lazuli_doctor::error_vocab::error_vocab::check_*` — every `pub` item
//! from the rule files is re-exported here so the surface stays stable.
//!
//! Closed catalogs (proposal §2.B / §2.C):
//! * Error codes (12): `policy_denied`, `validation_failed`,
//!   `tenant_mismatch`, `not_found`, `rate_limited`, `bad_request`,
//!   `method_not_allowed`, `integration_error`, `unique_violation`,
//!   `foreign_key_violation`, `not_null_violation`, `check_violation`.
//! * 4xx exposure fields: `message`, `code`, `data`, `message_key`.
//! * 5xx exposure fields: `code`, `data`. (`message` is rejected — see
//!   ERR-VOCAB-EXPOSE-5XX-MESSAGE).
//!
//! Reference: `docs/proposals/ir-error-messages-vocab.md` §6 §11
//! Cell ANALYZE-1.

mod catalogs;
mod rule_builtin_fallback;
mod rule_code_unknown;
mod rule_expose_5xx_message;
mod rule_expose_unknown;
mod rule_policies_no_when_denied;
mod rule_translation_key_unknown;
mod rule_when_denied_no_policy;

pub use catalogs::{EXPOSE_4XX_FIELDS, EXPOSE_5XX_FIELDS, FRAMEWORK_ERROR_CODES};
pub use rule_builtin_fallback::{BuiltinFallbackFinding, check_builtin_fallback};
pub use rule_code_unknown::{CodeUnknownFinding, check_code_unknown};
pub use rule_expose_5xx_message::{Expose5xxMessageFinding, check_expose_5xx_message};
pub use rule_expose_unknown::{ExposeUnknownFinding, check_expose_unknown};
pub use rule_policies_no_when_denied::{
    PoliciesNoWhenDeniedFinding, check_policies_no_when_denied,
};
pub use rule_translation_key_unknown::{KeyUnknownFinding, check_translation_key_unknown};
pub use rule_when_denied_no_policy::{
    WhenDeniedNoPolicyFinding, WhenDeniedSite, check_when_denied_no_policy,
};

// =============================================================================
// Tests — exercise each rule positive + negative against synthetic IR.
//
// Fixture-driven coverage lives in `crates/lazuli_cli/tests/doctor_error_vocab.rs`
// (the dispatcher feeds each fixture through `DoctorPackage::diagnostics()`
// and asserts the expected code fires exactly once). These unit tests
// keep the rule shape pinned without depending on the doctor scaffolding.
// =============================================================================

#[cfg(test)]
mod tests {
    include!("mod_tests.rs");
}
