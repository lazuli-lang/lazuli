//! Test-discipline lint rules (`TEST-*`).
//!
//! These checks live separately from `vocab/*` because they assert
//! behavioural-coverage discipline (file-pair existence, authored test
//! presence, scaffold-stub fading), not vocabulary fitness. The split is
//! mechanical today — code-prefix `TEST-*` — but Wave 0.5 of the
//! [TDD/BDD-first proposal](../../../../docs/proposals/tdd-bdd-first-2026-05-23.md)
//! plans to back this with a `RuleCategory::TestDiscipline` enum so the
//! doctor severity mapper can treat the category as a unit.
//!
//! v0 catalog (Wave 3.5): test_view_e2e_missing_001 — pairs every
//! `view` declared in `.lzx` with a Playwright spec at
//! `e2e/<experience>/<view>.spec.ts`. File-pair check only; doctor does
//! NOT invoke Playwright (preserves the wire-thin invariant).

pub mod test_view_e2e_missing_001;
