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
//! Fourteen resource-lifecycle structural checks (see
//! `docs/proposals/lifecycle-vocab.md` §5):
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

/// Aggregate every `LIFECYCLE-*` finding across the package's Tier 3
/// facts into the canonical `DoctorDiagnostic` envelope.
///
/// Returns an empty vec when `security_profile == Prototype` — the
/// family is opt-in at strict/production.
pub(crate) fn diagnostics(
    facts: &[Tier3FeatureFacts],
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }

    let mut out: Vec<DoctorDiagnostic> = Vec::new();
    for fact in facts {
        let feature = make_synthetic_feature_for_correctness(fact);
        let header_line = fact.feature_line;
        let path = &fact.path;

        out.extend(enum_duplicate_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(field_double_declared_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(invariant_param_unresolved_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(no_initial_state_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(state_duplicate_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(terminal_has_outgoing_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(timestamp_type_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(transition_from_undeclared_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(transition_to_undeclared_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(unreachable_state_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(policy_required_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(no_jump_needs_linear_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(initial_ambiguous_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
        out.extend(invariant_catalog_mismatch_findings(
            &feature,
            path,
            header_line,
            security_profile,
        ));
    }
    out
}

/// Empty severity-override map. No `[doctor.lifecycle]` TOML section
/// ships today; when one lands the caller threads the parsed overrides
/// in here.
fn empty_overrides() -> std::collections::BTreeMap<String, DoctorSeverityOverride> {
    std::collections::BTreeMap::new()
}

/// G-A2 — dispatch the dormant command-trigger lifecycle-transition pass
/// (`LIFECYCLE-TRANSITION-001..006`).
///
/// `lazuli_analyzer::checks::lifecycle_transition::check` validates that every
/// `command … triggers transition <name>` clause names a transition declared
/// on the command's `updates` target resource, that the chain is contiguous,
/// and that the command doesn't also assign the lifecycle discriminator
/// directly. The pass shipped with passing unit tests but
/// `run_lifecycle_transition_checks` was never called from the CLI / doctor /
/// LSP — so the six codes never fired on real `.lzi` usage (the dormant class
/// the G-A2 tail wave closes).
///
/// Unlike the structural `LIFECYCLE-*` rules above (which read only the
/// resource lifecycle block), this pass walks `commands` + `resources` and
/// resolves cross-feature `updates` targets through `uses`, so the synthetic
/// features must carry `uses` (the correctness adapter blanks it). Severity
/// resolves through `RuleCategory::Lifecycle` like the rest of the family:
/// `Strict` → Warning, `Production` → Error.
pub(crate) fn transition_command_diagnostics(
    facts: &[Tier3FeatureFacts],
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }

    // Build the full feature set in one pass — the transition check resolves
    // cross-feature `updates @feature.<f>.<R>` targets via `uses`, so it must
    // see every feature at once (not one synthetic feature in isolation).
    let features: Vec<lazuli_ir::Feature> = facts
        .iter()
        .map(|fact| {
            let mut feature = make_synthetic_feature_for_correctness(fact);
            feature.uses = fact.uses.clone();
            feature
        })
        .collect();

    let mut out = Vec::new();
    for diagnostic in lazuli_analyzer::checks::run_lifecycle_transition_checks(&features) {
        // Anchor the finding: a transition diagnostic carries the offending
        // command's name in its message ("command `<name>` …"); resolve to the
        // command's source line via the fact bundle, falling back to the
        // feature header. The owning fact is the one whose commands set carries
        // the named command.
        let (path, line, feature_name) = anchor_for_transition(facts, &diagnostic.message);
        let severity = match diagnostic.severity {
            // Profile-gate the hard rejections; advisory overlaps stay Warning.
            lazuli_analyzer::checks::lifecycle_transition::Severity::Error => doctor_severity_for(
                diagnostic.code,
                RuleCategory::Lifecycle,
                security_profile,
                &empty_overrides(),
            ),
            lazuli_analyzer::checks::lifecycle_transition::Severity::Warning => {
                crate::doctor::DoctorSeverity::Warning
            }
        };
        out.push(DoctorDiagnostic {
            path,
            line,
            column: 1,
            severity,
            code: diagnostic.code.to_owned(),
            message: diagnostic.message,
            category: Some(RuleCategory::Lifecycle),
            feature_name,
            construct: None,
            fix: None,
            group: None,
        });
    }
    out
}

/// Resolve the `(path, line, feature)` anchor for a transition diagnostic by
/// matching the ``command `<name>` `` token in the message against each fact's
/// `command_lines` map. Falls back to the first fact's header line when no
/// command name is recoverable.
fn anchor_for_transition(
    facts: &[Tier3FeatureFacts],
    message: &str,
) -> (std::path::PathBuf, usize, Option<String>) {
    let command_name = message
        .split_once("command `")
        .and_then(|(_, rest)| rest.split_once('`').map(|(name, _)| name.to_owned()));

    if let Some(command_name) = command_name {
        for fact in facts {
            if let Some(&line) = fact.command_lines.get(&command_name) {
                return (fact.path.clone(), line.max(1), Some(fact.feature.clone()));
            }
        }
    }
    match facts.first() {
        Some(fact) => (
            fact.path.clone(),
            fact.feature_line.max(1),
            Some(fact.feature.clone()),
        ),
        None => (std::path::PathBuf::new(), 1, None),
    }
}

fn enum_duplicate_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    // 2026-05-27 — Honor `# doctor:allow LIFECYCLE-ENUM-DUPLICATE` at
    // the source-file level. The underlying detector reads only the
    // synthesized IR (which has no source comments), so the canonical
    // escape hatch must be applied here. Until the analyzer records a
    // `synth_origins` entry for lifecycle-emitted enums, every authored
    // `lifecycle <field>` block trips the rule against its own auto-
    // emission — this opt-out lets pilots silence the false positive
    // with a reasoned comment while the framework-side fix lands.
    if file_contains_doctor_allow(path, lifecycle::enum_duplicate::Finding::CODE) {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        lifecycle::enum_duplicate::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::enum_duplicate::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::enum_duplicate::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn field_double_declared_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    // 2026-05-27 — Honor `# doctor:allow LIFECYCLE-FIELD-DOUBLE-DECLARED`
    // at the source-file level. Same root cause as enum_duplicate above:
    // analyzer auto-emits the discriminator field into resource.fields,
    // then the detector flags it against its own emission. Pilots can
    // silence with a reasoned comment until the framework-side fix lands
    // (detector should consult `feature.synth_origins`).
    if file_contains_doctor_allow(path, lifecycle::field_double_declared::Finding::CODE) {
        return Vec::new();
    }
    let severity = doctor_severity_for(
        lifecycle::field_double_declared::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::field_double_declared::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::field_double_declared::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn invariant_param_unresolved_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::invariant_param_unresolved::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::invariant_param_unresolved::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::invariant_param_unresolved::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn no_initial_state_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::no_initial_state::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::no_initial_state::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::no_initial_state::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn state_duplicate_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::state_duplicate::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::state_duplicate::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::state_duplicate::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn terminal_has_outgoing_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::terminal_has_outgoing::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::terminal_has_outgoing::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::terminal_has_outgoing::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn timestamp_type_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::timestamp_type::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::timestamp_type::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::timestamp_type::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn transition_from_undeclared_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::transition_from_undeclared::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::transition_from_undeclared::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::transition_from_undeclared::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn transition_to_undeclared_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::transition_to_undeclared::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::transition_to_undeclared::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::transition_to_undeclared::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn unreachable_state_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::unreachable_state::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::unreachable_state::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::unreachable_state::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn policy_required_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::policy_required::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::policy_required::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::policy_required::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn no_jump_needs_linear_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::no_jump_needs_linear::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::no_jump_needs_linear::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::no_jump_needs_linear::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn initial_ambiguous_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::initial_ambiguous::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::initial_ambiguous::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::initial_ambiguous::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}

fn invariant_catalog_mismatch_findings(
    feature: &lazuli_ir::Feature,
    path: &Path,
    header_line: usize,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let severity = doctor_severity_for(
        lifecycle::invariant_catalog_mismatch::Finding::CODE,
        RuleCategory::Lifecycle,
        security_profile,
        &empty_overrides(),
    );
    lifecycle::invariant_catalog_mismatch::check(feature, path)
        .into_iter()
        .map(|finding| DoctorDiagnostic {
            message: finding.message(),
            path: finding.path,
            line: header_line.max(1),
            column: 1,
            severity,
            code: lifecycle::invariant_catalog_mismatch::Finding::CODE.to_owned(),
            category: Some(RuleCategory::Lifecycle),
            feature_name: Some(feature.name.clone()),
            construct: None,
            fix: None,
            group: None,
        })
        .collect()
}
