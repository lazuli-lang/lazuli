//! Doctor cross-feature auth diagnostics — `auth_*_001` rule family.
//!
//! Per `docs/proposals/auth-lowering-scope.md` Route A §"Closed-cycle
//! criterion": four IR-driven cross-feature checks live here, one file
//! per diagnostic, matching the rule-module convention used by
//! `doctor/correctness/` and `doctor/vocab/`.
//!
//! Each rule's `check()` reads only typed IR (`Feature.auth`,
//! `Feature.resources`, `Feature.extensions`) and produces zero-IO
//! `Finding` values that mirror the diagnostic emitted by
//! `doctor::auth_diagnostics`. The dispatch into
//! `DoctorPackage::diagnostics()` continues to live in `doctor.rs`
//! against `AuthFacts` (line-anchored facts); these modules host the
//! pure-IR shape so doctor/auth coverage gains a unit-testable surface
//! without touching the integration path.
//!
//! v0.1 (4 rules):
//!   - `auth_password_algorithm_hash_mismatch_001`
//!   - `auth_sessions_resource_unknown_001`
//!   - `auth_identity_field_unknown_001`
//!   - `auth_oauth_adapter_unbound_001`

pub mod auth_identity_field_unknown_001;
pub mod auth_oauth_adapter_unbound_001;
pub mod auth_password_algorithm_hash_mismatch_001;
pub mod auth_sessions_resource_unknown_001;
