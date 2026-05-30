//! TEST-FAILURE-ONLY-COVERAGE-001 — a `_test.go` file declares one
//! or more `func Test*` entrypoints, but no single test exercises a
//! success path.
//!
//! Catches the failure-only-test theater shape: every Test* body
//! contains only error-side assertions (`require.Error`,
//! `assert.Error`, `assert.Contains(t, err.Error(), ...)`) or
//! `t.Skip`s outright, and zero bodies invoke a positive assertion
//! (`require.NoError`, `assert.Equal(t, expected, result)`,
//! `assert.NotEmpty(...)`, etc.). The handler's success contract is
//! unverified — a regression that changes the success shape or
//! returns a slightly-different error will still pass the file
//! green.
//!
//! Sibling to [`crate::test_discipline::test_handler_missing_001`]
//! (file exists?) and the planned `TEST-PINS-STUB-VOCAB-001`
//! (file pins stub vocab?). Three rules, overlapping but distinct:
//! missing → exists-but-pins-stub → exists-but-only-covers-failure.
//!
//! Default severity: `warning` (per spec — some genuinely-negative
//! suites are legitimate, e.g. validation matrices). Opt-in
//! `error` under tdd-iron-hand. File-level opt-out via
//! `# doctor:allow TEST-FAILURE-ONLY-COVERAGE-001 — reason "..."`.
//!
//! Reference: docs/proposals/doctor-test-failure-only-coverage.md.

use std::path::PathBuf;

use crate::allow_comment::source_contains_doctor_allow;
use crate::error_handling::walker::GoHandlerSourceFile;

// ── output ────────────────────────────────────────────────────────────────────

include!("test_failure_only_coverage_001_p1.rs");
include!("test_failure_only_coverage_001_p2.rs");
