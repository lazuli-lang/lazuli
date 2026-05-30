//! Tier-3 job aggregator (JOB-TIMEOUT-001, JOB-FANOUT-001/002).
//!
//! Lifted from the parent `tier3` god-file in the rails-style split.
//! Only the per-job rule emits live here; the parent `mod.rs` still
//! owns dispatch, accumulator setup, and the cross-feature
//! WEBHOOK-EVENT-001 sweep.

use crate::doctor::{DoctorDiagnostic, DoctorSeverity, Tier3FeatureFacts};

pub(super) fn tier3_job_diagnostics(
    feature: &Tier3FeatureFacts,
    job: &lazuli_ir::Job,
    diagnostics: &mut Vec<DoctorDiagnostic>,
) {
    let line = feature
        .job_lines
        .get(&job.name)
        .copied()
        .unwrap_or(feature.feature_line);

    // JOB-TIMEOUT-001: job declares external calls but no timeout. The
    // `INT-CALL-002` text-pattern check on `ExternalCallFact` covers
    // the same ground today; this rule fires from the IR lift so the
    // diagnostic survives `parse_command` arriving in Tier 4 and the
    // text-pattern fact disappearing.
    if !job.external_calls.is_empty() && job.timeout.is_none() {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "JOB-TIMEOUT-001".to_owned(),
            message: format!(
                "job `{}` declares external `calls` but no `timeout \"...\"` — external operations require an explicit timeout.",
                job.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // JOB-FANOUT-001: fanout.axis must match the feature's tenancy axis.
    if let (Some(fanout), Some(axis)) = (&job.fanout, &feature.tenancy_axis)
        && &fanout.axis != axis
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "JOB-FANOUT-001".to_owned(),
            message: format!(
                "job `{}` declares `fanout tenants {}` but feature `{}` uses tenancy axis `{}`.",
                job.name, fanout.axis, feature.feature, axis
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }

    // JOB-FANOUT-002: scheduled job declares fanout but no idempotency
    // key — without a key, fanout re-fire can double-execute on tenants.
    if matches!(job.trigger, lazuli_ir::JobTrigger::Schedule { .. })
        && job.fanout.is_some()
        && job.idempotency.is_none()
    {
        diagnostics.push(DoctorDiagnostic {
            path: feature.path.clone(),
            line,
            column: 1,
            severity: DoctorSeverity::Warning,
            code: "JOB-FANOUT-002".to_owned(),
            message: format!(
                "scheduled job `{}` declares `fanout` but no `idempotency by ...` — re-fires may double-execute per tenant.",
                job.name
            ),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}
