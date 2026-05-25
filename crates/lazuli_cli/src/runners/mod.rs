//! Per-layer test runners invoked by `cmd_test`.
//!
//! Each runner exposes a `run(...)` function that returns a
//! [`crate::cmd_test_types::LayerResult`] (and may pre-flight a sub-runner
//! before shelling out). All runners are wire-thin: they invoke native
//! tooling and parse output into the unified schema; they do not
//! re-implement test execution.

pub mod go_test;
pub mod handler_coverage;
pub mod playwright;
pub mod spec;
pub mod ts_test;
