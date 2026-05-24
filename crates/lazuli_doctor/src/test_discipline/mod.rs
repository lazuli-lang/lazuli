//! Doctor test-discipline rules — pair every executable construct with
//! a test file so TDD/BDD is the default surface.
//!
//! Sibling to `correctness/` (concrete bug catchers) and `vocab/`
//! (style/vocabulary fitness). Test-discipline rules emit when an
//! executable surface (handler, view, command) has no corresponding
//! test scaffold.
//!
//! Severity grades smoothly: `info` in the prototype profile (so green
//! field projects do not see a wall of red), `warning` at strict,
//! `error` at production. The dispatcher in
//! `lazuli_cli::doctor::correctness_diagnostics` applies the profile
//! mapping; this module only emits plain findings.
//!
//! Reference: docs/proposals/tdd-bdd-first-2026-05-23.md §5.3.

pub mod test_handler_missing_001;
