//! Test-discipline doctor rules — `TEST-*` family + `DOCTOR-*`
//! meta-rules that govern how `[doctor.test_discipline]` overrides are
//! authored.
//!
//! Wave 0.5 establishes only the meta-rule
//! `DOCTOR-OVERRIDE-NEEDS-REASON-001`. The five `TEST-*` content rules
//! (`TEST-MISSING-AUTHORED-001`, `TEST-PREDICATE-UNCOVERED-001`,
//! `TEST-RESTATES-EFFECT-001`, `TEST-RESTATES-POLICY-001`,
//! `TEST-FIXTURE-LITERAL-001`) land in Wave 1.
//!
//! See `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 0.5.

pub mod override_needs_reason_001;
