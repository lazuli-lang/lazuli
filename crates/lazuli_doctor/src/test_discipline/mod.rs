//! Test-discipline doctor rules (Wave 4 + §7.1 widening of the TDD/BDD-first proposal).
//!
//! These rules detect drift between authored test declarations and the IR
//! shapes those declarations are supposed to constrain. Three rules live
//! here today:
//!
//! - `TEST-VIEW-EXTENSIBILITY-001` (warning) — extensible views must
//!   author at least one `accepted by` / `rejected by` assertion.
//! - `TEST-VIEW-DRIFT-001` (error) — `accepted by <feature>` must resolve
//!   to a feature whose `extends @anchor.<X>` clause matches the host
//!   view's anchor.
//! - `TEST-COMMAND-ASSERTION-DRIFT-001` (error) — a command `tests` block
//!   with `denies when target.<field> = <value>` must be backed by either
//!   a resource-level invariant, a constraint, or a lifecycle state gate
//!   enforcing that filter. Without such backing, the assertion is
//!   documentation-only and runtime behavior may diverge (the
//!   `leave_host_reply` bug pattern from proposal §7.1).
//!
//! Per the TDD/BDD-first proposal Waves 4 + §7.1, dispatcher wiring lands
//! alongside the Wave 0.5 `RuleCategory` infrastructure; until then each
//! rule's `check` is exercised through its `#[cfg(test)] mod tests`.

pub mod test_command_assertion_drift_001;
pub mod test_view_drift_001;
pub mod test_view_extensibility_001;
