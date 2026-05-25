//! Agent aggregator — emits every `agent_*` family diagnostic plus
//! the registry tool-effect cross-check.
//!
//! ## Sub-modules
//!
//! - `tool` — `agent_tool_diagnostics` (policy / write_unguarded /
//!   pii_unsafetied) + `resolve_tool` + `policy_atoms_more_restrictive`.
//! - `discriminator` — `agent_discriminator_diagnostics` (target_invalid
//!   / field_invalid) + `check_record_discriminator`.
//! - `eval` — `agent_eval_diagnostics` + `operand_resolves_numeric`.
//! - `expose` — `agent_expose_diagnostics` (Cut A.7 cross-feature path
//!   conflict + audience reachability).
//! - `run_trace` — `agent_run_trace_diagnostics` (Cut A.8 built-in
//!   `agent_run` trace event) + payload-field drift + reserved-name
//!   + `trigger_trace_unknown`.
//!
//! `registry_tool_effect_diagnostics` lives in this file because it
//! sits next to the agent tool resolver and is a thin one-pass walker
//! over `RegistryToolDefect` (the doctor `package::load` collects these
//! per registry).
//!
//! Extracted from `doctor/mod.rs` in rails-style R4-C Stage 4; fanned
//! out into per-diagnostic-family sub-modules in rails-style R7-4.

use crate::app_manifest::RegistryToolDefectReason;
use crate::doctor::{DoctorDiagnostic, DoctorSeverity, RegistryToolDefect};

mod discriminator;
mod eval;
mod expose;
mod run_trace;
mod tool;

pub(crate) use discriminator::{agent_discriminator_diagnostics, check_record_discriminator};
pub(crate) use eval::{agent_eval_diagnostics, operand_resolves_numeric};
pub(crate) use expose::agent_expose_diagnostics;
pub(crate) use run_trace::{
    agent_run_trace_diagnostics, canonical_payload_event, scan_payload_field_drift,
};
pub(crate) use tool::agent_tool_diagnostics;

// -----------------------------------------------------------------------------
// Diagnostic id: tool_registry_effect_required_diagnostics
// -----------------------------------------------------------------------------

pub(crate) fn registry_tool_effect_diagnostics(
    defects: &[RegistryToolDefect],
) -> Vec<DoctorDiagnostic> {
    defects
        .iter()
        .map(|defect| DoctorDiagnostic {
            path: defect.path.clone(),
            line: defect.line,
            column: 1,
            severity: DoctorSeverity::Error,
            code: "tool_registry_effect_required_diagnostics".to_owned(),
            message: match defect.reason {
                RegistryToolDefectReason::EffectMissing => format!(
                    "registry `tool {}` is missing `effect read|write` — every tool MUST declare its effect so agents can guard write-tools properly.",
                    defect.name
                ),
                RegistryToolDefectReason::EffectInvalid => format!(
                    "registry `tool {}` declares an invalid `effect` — only `read` or `write` are accepted.",
                    defect.name
                ),
            },
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}
