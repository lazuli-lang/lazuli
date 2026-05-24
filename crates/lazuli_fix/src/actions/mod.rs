//! `FixAction` trait and the built-in action catalogue.

use anyhow::Result;

use crate::{FixRequest, FixResult};

pub mod insert_tests_block;
pub mod scaffold_errors_block;

/// One fix action — a thin object that knows how to mutate a file in
/// response to a doctor finding.
pub trait FixAction {
    /// Rule code this action services (e.g. `TEST-MISSING-AUTHORED-001`).
    fn rule_code(&self) -> &'static str;

    /// Run the action. Implementations honor `request.apply` (write to
    /// disk vs. preview only) and return a `FixResult` in either case.
    fn execute(&self, request: &FixRequest) -> Result<FixResult>;
}
