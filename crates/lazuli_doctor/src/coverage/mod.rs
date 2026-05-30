//! Wave 6 — Per-layer coverage report.
//!
//! See `docs/proposals/tdd-bdd-first-2026-05-23.md` §Wave 6 for the
//! design. Coverage is reported and gated **per layer**, never as a
//! single aggregated percentage that hides which paradigm is weak.
//! Aggregate is opt-in only with explicit method disclosure.
//!
//! Layer catalog (canonical):
//!
//! - [`spec_predicate`] — walks IR predicates; counts branches; counts
//!   coverage from `tests` blocks (pure-IR)
//! - [`spec_actor_matrix`] — walks `policy @policy.X` references vs
//!   `auth.roles` (pure-IR)
//! - [`spec_transition_state`] — walks transitions; computes
//!   `from <state>` coverage (pure-IR)
//! - [`view_extensibility`] — counts views with assertions present
//!   (pure-IR)
//! - [`view_e2e_pair`] — filesystem-checks
//!   `e2e/<feature>/<view>.spec.ts` (filesystem)
//! - [`handler_go`] — parses `go test -coverprofile` output if file
//!   present (external)
//!
//! Pure-IR layers compute coverage at parse-time with zero runtime
//! dependency, zero instrumentation, zero flakiness.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use lazuli_ir::Feature;

pub mod handler_go;
pub mod presets;
pub mod spec_actor_matrix;
pub mod spec_predicate;
pub mod spec_transition_state;
pub mod view_e2e_pair;
pub mod view_extensibility;

pub use presets::{CoveragePreset, preset_severity_overrides, preset_thresholds};

#[cfg(test)]
pub(crate) mod test_support;

include!("mod_p1.rs");
include!("mod_p2.rs");
