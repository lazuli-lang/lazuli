//! Lifecycle-vocabulary aggregator.
//!
//! Wires the ten `lazuli_doctor::lifecycle::*` rules into the canonical
//! `DoctorDiagnostic` envelope. Each rule ships with passing unit tests
//! but was previously never reached by the dispatcher — the audit at
//! `docs/proposals/lifecycle-vocab-architect-audit-2026-05-27.md` §"Cell
//! A" flagged the gap; this aggregator closes it.
//!
//! ## Scope
//!
//! Fifteen resource-lifecycle structural checks (see
//! `docs/proposals/lifecycle-vocab.md` §5 and
//! `.specs/changes/0017-state-enum-transition` for the closed-state-set rule):
//!
//! - `LIFECYCLE-ENUM-DUPLICATE`
//! - `LIFECYCLE-FIELD-DOUBLE-DECLARED`
//! - `LIFECYCLE-INITIAL-AMBIGUOUS`
//! - `LIFECYCLE-INVARIANT-CATALOG-MISMATCH`
//! - `LIFECYCLE-INVARIANT-PARAM-UNRESOLVED`
//! - `LIFECYCLE-NO-INITIAL-STATE`
//! - `LIFECYCLE-NO-JUMP-NEEDS-LINEAR`
//! - `LIFECYCLE-POLICY-REQUIRED`
//! - `LIFECYCLE-STATE-DUPLICATE`
//! - `LIFECYCLE-STATE-SET-UNDECLARED-001`
//! - `LIFECYCLE-TERMINAL-HAS-OUTGOING-TRANSITION`
//! - `LIFECYCLE-TIMESTAMP-TYPE`
//! - `LIFECYCLE-TRANSITION-FROM-UNDECLARED`
//! - `LIFECYCLE-TRANSITION-TO-UNDECLARED`
//! - `LIFECYCLE-UNREACHABLE-STATE`
//!
//! Every rule shares the uniform signature
//! `check(feature: &Feature, path: &Path) -> Vec<Finding>` and exposes
//! `Finding::CODE` + `Finding::message()`. The aggregator walks the
//! Tier 3 fact bundle, synthesizes a per-feature view via the
//! correctness aggregator's `make_synthetic_feature_for_correctness`
//! adapter (lifecycle/resources slices are populated there), and emits
//! one `DoctorDiagnostic` per finding under `RuleCategory::Lifecycle`.
//!
//! ## Severity policy
//!
//! Every rule defers to `doctor_severity_for` with
//! `RuleCategory::Lifecycle`. The category falls into the default
//! (non-test-discipline) branch, so the legacy global mapping applies:
//! `Production` → Error, `Strict` → Warning. The `Prototype` profile
//! short-circuits to an empty vec — lifecycle structural rules
//! reference an authored lifecycle block, which is itself a v0.2
//! vocabulary primitive most prototypes skip. Promoting to Strict /
//! Production opts the family in.

use std::path::Path;

use lazuli_doctor::allow_comment::file_contains_doctor_allow;
use lazuli_doctor::{RuleCategory, lifecycle};
use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use super::correctness::make_synthetic_feature_for_correctness;
use crate::doctor::diagnostic::DoctorSeverityOverride;
use crate::doctor::{DoctorDiagnostic, Tier3FeatureFacts, doctor_severity_for};

// Aggregate every `LIFECYCLE-*` finding across the package's Tier 3 facts into
// the canonical `DoctorDiagnostic` envelope (`diagnostics`, in lifecycle_p1.rs).
// Returns an empty vec when `security_profile == Prototype` — the family is
// opt-in at strict/production.
include!("lifecycle_p1.rs");
include!("lifecycle_p2.rs");
