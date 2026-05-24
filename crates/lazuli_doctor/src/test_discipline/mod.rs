//! Test-discipline doctor rules — `TEST-*` family + `DOCTOR-*` meta-rules.
//!
//! Each rule is a sub-module exposing a `check` function returning `Vec<Finding>`.
//! The CLI dispatcher in `crates/lazuli_cli/src/doctor.rs` adapts each `Finding`
//! into a `DoctorDiagnostic` and emits with the rule's documented severity.
//!
//! See `docs/proposals/test-completeness-lints.md` (companion design) and
//! `docs/proposals/tdd-bdd-first-2026-05-23.md` Waves 0.5 + 1 + 3 + 4 + §7.1
//! for the canonical spec.
//!
//! Waves 0.5 + 1 + 3 + 4 ship twelve rules in this module:
//!
//! - `override_needs_reason_001` (warning) — `[doctor.test_discipline].severity_override`
//!   entry exists without a non-empty `reason` field. Wave 0.5 meta-rule.
//! - `test_missing_authored_001` (warning) — construct with authored predicate
//!   gate has no `tests` block.
//! - `test_predicate_uncovered_001` (info) — `tests` block carries one-side
//!   coverage only (allows-only or denies-only).
//! - `test_restates_effect_001` (warning) — `allows when` assertion restates a
//!   field the construct itself writes (timestamps/updates/creates).
//! - `test_restates_policy_001` (warning) — actor-only assertion shadows the
//!   generated permits/forbids matrix derived from `policy @policy.*`.
//! - `test_fixture_literal_001` (error) — predicate value matches a
//!   fixture-shape literal (CPF/email/phone/UUID), Jest-creep vector.
//! - `migration_dsl_unique_001` (error) — `.lzi` field declares `unique`
//!   but the resource's `*.sql` migration does NOT carry `UNIQUE (<col>)`.
//!   Catches LAZ-MIGRATION-DSL-UNIQUE (Hostpoint pass-2 bug §7.1).
//! - `runtime_update_builder_jsonb_001` (warning) — `UpdateEffect` writes a
//!   JSONB-typed field that codegen will lower to `SetIfNotNilSlice`.
//!   Catches LAZ-RUNTIME-SETSLICE-JSONB.
//! - `test_stub_001` (warning) — catches `@TODO authored:` markers from
//!   `lazuli generate command/view/rule/transition/handler`. Wave 3.
//! - `test_view_extensibility_001` (warning) — extensible views must author
//!   at least one `accepted by` / `rejected by` assertion. Wave 4.
//! - `test_view_drift_001` (error) — `accepted by <feature>` must resolve to
//!   a feature whose `extends @anchor.<X>` matches the view's anchor. Wave 4.
//! - `test_command_assertion_drift_001` (error) — command `tests` block with
//!   `denies when target.<field> = <value>` must be backed by an invariant,
//!   constraint, or lifecycle gate. Wave 4 + §7.1 widening (catches
//!   `leave_host_reply` bug pattern).

pub mod migration_dsl_unique_001;
pub mod override_needs_reason_001;
pub mod runtime_update_builder_jsonb_001;
pub mod test_command_assertion_drift_001;
pub mod test_fixture_literal_001;
pub mod test_missing_authored_001;
pub mod test_predicate_uncovered_001;
pub mod test_restates_effect_001;
pub mod test_restates_policy_001;
pub mod test_stub_001;
pub mod test_view_drift_001;
pub mod test_view_e2e_missing_001;
pub mod test_handler_missing_001;
pub mod test_view_extensibility_001;
