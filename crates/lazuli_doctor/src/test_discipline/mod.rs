//! Test discipline lint rules (`TEST-*` + adjacent runtime/migration rules).
//!
//! Each rule is a sub-module exposing a `check` function returning `Vec<Finding>`.
//! The CLI dispatcher in `crates/lazuli_cli/src/doctor.rs` adapts each `Finding`
//! into a `DoctorDiagnostic` and emits with the rule's documented severity.
//!
//! See `docs/proposals/test-completeness-lints.md` (companion design) and
//! `docs/proposals/tdd-bdd-first-2026-05-23.md` Wave 1 for the canonical spec.
//!
//! Wave 1 v0.1 ships seven rules:
//!
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
//!   JSONB-typed field that codegen will lower to `SetIfNotNilSlice` (the
//!   pgx pathway that fails on JSONB targets). Catches LAZ-RUNTIME-SETSLICE-JSONB.

pub mod migration_dsl_unique_001;
pub mod runtime_update_builder_jsonb_001;
pub mod test_fixture_literal_001;
pub mod test_missing_authored_001;
pub mod test_predicate_uncovered_001;
pub mod test_restates_effect_001;
pub mod test_restates_policy_001;
