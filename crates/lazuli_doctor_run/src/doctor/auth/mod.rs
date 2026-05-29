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
//!
//! v0.2 (+1 rule, IR-driven session-query security invariant):
//!   - `session_query_temporal_validity_001` — a `query.list` over the
//!     session resource bound by `auth sessions resource <X>` must carry
//!     a temporal lower bound (`expires_at > ctx.now` / `>=`) so it
//!     cannot return expired sessions. Name-agnostic (does not gate on
//!     the literal `active_sessions`); the resource is resolved from the
//!     binding and queries are attached with the codegen name scorer.
//!     Unlike the v0.1 four, this rule IS wired into the live dispatcher
//!     (`dispatch.rs`) via the re-parsed typed `Feature` IR, so it
//!     actually fires at `lazuli doctor` time and blocks under the
//!     strict profile.

pub mod auth_identity_field_unknown_001;
pub mod auth_oauth_adapter_unbound_001;
pub mod auth_password_algorithm_hash_mismatch_001;
pub mod auth_sessions_resource_unknown_001;
pub mod session_query_temporal_validity_001;

// Session-cookie transport family (`SESSION-COOKIE-*`) — the five doctor
// rules over `auth.sessions.cookie`. See
// `docs/proposals/cookie-sessions-child.md` §Doctor. Like
// `session_query_temporal_validity_001`, these are wired into the live
// dispatcher (`package_methods.rs` + `dispatch.rs`) via the re-parsed
// typed `Feature` IR (the fact-only `AuthFacts` slice the aggregator uses
// does not carry the cookie sub-block).
pub mod session_cookie_host_prefix_violation_001;
pub mod session_cookie_insecure_in_prod_001;
pub mod session_cookie_missing_001;
pub mod session_cookie_profile_conflict_001;
pub mod session_cookie_samesite_none_insecure_001;

// Shared `#[cfg(test)]` fixtures for the five rules above.
#[cfg(test)]
mod session_cookie_test_support;
