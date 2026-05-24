//! T5 — Fail-fast coordinator for `lazuli test`.
//!
//! When `--fail-fast` is set, the orchestrator stops executing
//! remaining layers as soon as one fails. This module owns the
//! decision logic so the orchestrator can stay declarative:
//!
//! ```ignore
//! let mut coord = FailFastCoordinator::new(opts.fail_fast);
//! for layer in plan {
//!     if coord.should_skip() { /* emit skip event */ continue; }
//!     let result = run_layer(layer)?;
//!     coord.observe(&result);
//!     /* append to accumulator */
//! }
//! ```

use crate::cmd_test_types::{LayerResult, LayerVerdict};

#[derive(Debug, Default)]
pub struct FailFastCoordinator {
    enabled: bool,
    tripped: bool,
}

impl FailFastCoordinator {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            tripped: false,
        }
    }

    /// Returns true when the orchestrator should skip the next layer.
    pub fn should_skip(&self) -> bool {
        self.enabled && self.tripped
    }

    /// Record one layer result; flips the coordinator to `tripped`
    /// when the layer failed.
    pub fn observe(&mut self, layer: &LayerResult) {
        if layer.result == LayerVerdict::Fail {
            self.tripped = true;
        }
    }

    pub fn is_tripped(&self) -> bool {
        self.tripped
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd_test_types::{Layer, LayerResult, LayerVerdict};

    fn make(result: LayerVerdict) -> LayerResult {
        LayerResult {
            layer: Layer::Spec,
            runner: "lazuli-doctor".into(),
            result,
            tests_run: 0,
            tests_passed: 0,
            tests_failed: 0,
            issues: 0,
            exit_code: None,
            command: None,
            duration_ms: 0,
            failures: vec![],
            runner_native_only: None,
            skip_reason: None,
        }
    }

    #[test]
    fn disabled_never_skips() {
        let mut c = FailFastCoordinator::new(false);
        c.observe(&make(LayerVerdict::Fail));
        assert!(!c.should_skip());
        assert!(c.is_tripped()); // tripped but not enforcing
    }

    #[test]
    fn enabled_skips_after_fail() {
        let mut c = FailFastCoordinator::new(true);
        assert!(!c.should_skip());
        c.observe(&make(LayerVerdict::Pass));
        assert!(!c.should_skip());
        c.observe(&make(LayerVerdict::Fail));
        assert!(c.should_skip());
    }

    #[test]
    fn skipped_layers_dont_trip_fail_fast() {
        let mut c = FailFastCoordinator::new(true);
        c.observe(&make(LayerVerdict::Skip));
        assert!(!c.should_skip());
    }
}
