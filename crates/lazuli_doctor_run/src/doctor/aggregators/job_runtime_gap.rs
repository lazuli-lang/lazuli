//! Job runtime-gap aggregator — `JOB-*` rules over the lifted IR
//! `Feature.jobs` of every Tier 3 feature.
//!
//! Mirrors the IR-driven shape of [`super::correctness`] (it reuses the
//! same [`super::correctness::make_synthetic_feature_for_correctness`]
//! synthetic-feature builder, whose `jobs:` slot is populated from the
//! fact bundle) but resolves severity through the
//! `[doctor.error_handling].preset` like
//! [`super::error_handling_handlers`] — `JOB-*` is part of the
//! error-handling 4th dimension.
//!
//! ## Rules dispatched
//!
//! - [`JOB-DECLARATIVE-BODY-UNSUPPORTED-001`] — a `job` declared with a
//!   declarative body the Lazuli runtime cannot execute. Codegen lowers
//!   the body to a no-op (`jobs.JobContract` has no body slot), so the
//!   job registers but silently does nothing. Default `Warning`; under
//!   `tdd-iron-hand` → `Error`. Per `CLAUDE.md` inviolable rule 7 a gap
//!   the runtime can't honor must surface in tooling, not compile green.
//!
//! [`JOB-DECLARATIVE-BODY-UNSUPPORTED-001`]: lazuli_doctor::error_handling::job_declarative_body_unsupported_001

use lazuli_doctor::error_handling::job_declarative_body_unsupported_001;
use lazuli_doctor::error_handling::preset::ErrorHandlingPreset;

use super::correctness::make_synthetic_feature_for_correctness;
use crate::doctor::helpers::resolve_error_handling_severity;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

/// Aggregate every `JOB-*` runtime-gap finding across the package's
/// Tier 3 facts into the canonical [`DoctorDiagnostic`] envelope.
///
/// `preset` is the active `[doctor.error_handling]` preset (resolved by
/// the caller off the severity `ResolvedDoctorConfig`). Pass `None` to
/// fall back to the per-rule default (`Warning`).
///
/// v2 — the preset arrives pre-resolved from the caller's severity
/// config (CLI: disk; LSP: unsaved `Lazurite.toml` buffer) rather than
/// being re-read from an on-disk manifest here, so in-editor severity
/// tracks unsaved `[doctor.error_handling] preset` edits.
pub(crate) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    preset: Option<ErrorHandlingPreset>,
) -> Vec<DoctorDiagnostic> {
    let mut diagnostics = Vec::new();

    for fact in facts {
        let feature = make_synthetic_feature_for_correctness(fact);

        // JOB-DECLARATIVE-BODY-UNSUPPORTED-001 — default Warning; the
        // `[doctor.error_handling]` iron-hand preset escalates to Error.
        for finding in job_declarative_body_unsupported_001::check(&feature, &fact.path) {
            let severity = resolve_error_handling_severity(
                DoctorSeverity::Warning,
                job_declarative_body_unsupported_001::Finding::CODE,
                preset,
            );
            diagnostics.push(DoctorDiagnostic {
                message: finding.message(),
                path: finding.path,
                line: fact.feature_line,
                column: 1,
                severity,
                code: job_declarative_body_unsupported_001::Finding::CODE.to_owned(),
                category: None,
                feature_name: Some(fact.feature.clone()),
                construct: None,
                fix: None,
                group: None,
            });
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_facts_yield_no_diagnostics() {
        // Smoke test: the aggregator runs clean with no features.
        // Behavioral fires/silent coverage lives in the rule module
        // (`lazuli_doctor::error_handling::job_declarative_body_unsupported_001`),
        // which exercises `check` against a real `Feature`. The
        // synthetic-feature builder this aggregator reuses is itself
        // unit-tested under `super::correctness`.
        assert!(diagnostics(&[], None).is_empty());
    }

    #[test]
    fn severity_warns_by_default_errors_under_iron_hand() {
        // The "warn under strict, error under iron-hand" contract this
        // aggregator depends on, exercised through the same resolver the
        // dispatch path uses. With no preset the per-rule default
        // (`Warning`) stands at strict; `tdd-iron-hand` escalates to
        // `Error`.
        let code = job_declarative_body_unsupported_001::Finding::CODE;
        assert_eq!(
            resolve_error_handling_severity(DoctorSeverity::Warning, code, None),
            DoctorSeverity::Warning,
        );
        assert_eq!(
            resolve_error_handling_severity(
                DoctorSeverity::Warning,
                code,
                Some(ErrorHandlingPreset::TddIronHand),
            ),
            DoctorSeverity::Error,
        );
    }
}
