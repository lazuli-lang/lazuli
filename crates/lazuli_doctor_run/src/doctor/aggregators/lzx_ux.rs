//! §7a surface-UX aggregator — un-inerts the dormant `LZX-*` UX rules.
//!
//! Wires the §7a surface-UX doctor rules into the package-doctor dispatch:
//!
//! - `LZX-WIZARD-STEPS-EXPR-001`
//! - `LZX-TAB-GROUP-CASE-001`
//! - `LZX-TAB-VIEW-REF-001`
//! - `LZX-VIEW-MODE-001`
//! - `LZX-BOARD-LANES-001`
//! - `LZX-REPEATABLE-SUM-001`
//! - `LZX-DATE-RANGE-001`
//!
//! Each rule (`crate::doctor::lzx::ux_rules`, `crate::doctor::lzx::date_range_filter`)
//! ships with passing unit tests but was previously reached only from those
//! `#[test]`s — never from the package-doctor run, so it never fired on real
//! `.lzx` usage (the same "declared-but-inert" class as the F4 regressions and
//! the `LIFECYCLE-*` aggregator wiring gap).
//!
//! ## How the rules see real documents
//!
//! Both rules walk the rich per-feature [`lazuli_ir::Surface`] shape
//! (carrying [`lazuli_ir::ViewUx`] / [`lazuli_ir::FilterDecl`]). The doctor
//! package, however, carries Tier-3 feature facts (resources, enums, queries,
//! commands) plus the parsed experience-dialect `.lzx` documents — not a
//! fully-attached `lazuli_ir::Module`. So this aggregator synthesizes the
//! `Module` the same way the dispatched `lifecycle_gate` bridge does:
//!
//! 1. one synthetic `Feature` per Tier-3 fact (resources / enums / queries —
//!    via `make_synthetic_feature_for_correctness`);
//! 2. for each parsed `.lzx` file, lower its experience-dialect surfaces into
//!    the rich `ir::Surface` shape (`lazuli_analyzer::lower_lzx_feature_surfaces`,
//!    which reuses the surface-dialect UX lowering — no reimplementation) and
//!    attach each surface to its owning feature by name.
//!
//! The lowering synthesizes a `query.list` `source` whose feature is the
//! surface's owner, so single-resource features resolve unambiguously
//! (matching `ux_rules::resolve_resource`) and multi-resource features are
//! suppressed — exactly as the rules' own tests assume.
//!
//! ## Severity policy
//!
//! `LZX-*` is `RuleCategory::Correctness`. Severity defers to
//! `doctor_severity_for`: `Strict` → Warning, `Production` → Error
//! (`Prototype` short-circuits the whole family — surface UX is an opt-in
//! v0.2 vocabulary most prototypes skip). The rules' own intrinsic `Warn`
//! findings (non-exhaustive `tab_group`, `wizard_steps` total mismatch) stay
//! advisory Warnings regardless of profile.

use std::path::Path;

use lazuli_doctor::RuleCategory;
use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use super::correctness::make_synthetic_feature_for_correctness;
use crate::doctor::lzx::{date_range_filter, ux_rules};
use crate::doctor::{
    DoctorDiagnostic, DoctorFile, DoctorSeverity, Tier3FeatureFacts, doctor_severity_for,
    line_col_for_offset,
};

/// Run the §7a surface-UX rules over every parsed `.lzx` document in the
/// package, against the synthesized feature set.
///
/// Returns an empty vec under `Prototype` — the family is opt-in at
/// strict/production.
pub(crate) fn diagnostics(
    files: &[DoctorFile],
    facts: &[Tier3FeatureFacts],
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    if security_profile == SecurityProfile::Prototype {
        return Vec::new();
    }

    // Shared synthetic feature set — resources / enums / queries / commands the
    // rules resolve view sources + enum fields against. Built once; each file's
    // surfaces are attached onto a fresh clone so span offsets stay aligned
    // with that file's source.
    let base_features: Vec<lazuli_ir::Feature> = facts
        .iter()
        .map(make_synthetic_feature_for_correctness)
        .collect();

    let mut out = Vec::new();
    for file in files {
        let Some(document) = &file.lzx else {
            continue;
        };
        let surfaces = lazuli_analyzer::lower_lzx_feature_surfaces(document);
        if surfaces.is_empty() {
            continue;
        }

        let mut features = base_features.clone();
        attach_surfaces(&mut features, surfaces);
        let module = module_from_features(features);

        for finding in ux_rules::check(&module) {
            out.push(ux_finding_to_diagnostic(
                finding,
                &file.path,
                &file.source,
                security_profile,
            ));
        }
        for finding in date_range_filter::check(&module) {
            out.push(date_range_finding_to_diagnostic(
                finding,
                &file.path,
                &file.source,
                security_profile,
            ));
        }
    }
    out
}

/// Attach each lowered surface onto the feature it belongs to (matched by the
/// surface's owning feature name). Surfaces whose feature is not in the fact
/// set are dropped — the source-resource rules own that mismatch, not these.
fn attach_surfaces(features: &mut [lazuli_ir::Feature], surfaces: Vec<lazuli_ir::Surface>) {
    for surface in surfaces {
        if let Some(feature) = features.iter_mut().find(|f| f.name == surface.feature) {
            feature.surfaces.push(surface);
        }
    }
}

fn module_from_features(features: Vec<lazuli_ir::Feature>) -> lazuli_ir::Module {
    lazuli_ir::Module {
        workspace: None,
        contracts: Vec::new(),
        app: None,
        registry: None,
        profiles: Vec::new(),
        design: None,
        rbac: None,
        features,
    }
}

fn ux_finding_to_diagnostic(
    finding: ux_rules::Finding,
    path: &Path,
    source: &str,
    security_profile: SecurityProfile,
) -> DoctorDiagnostic {
    let severity = match finding.severity {
        // Structural errors resolve through the profile (strict → warning,
        // production → error) like the sibling correctness rules.
        ux_rules::Severity::Error => severity_for(finding.code, security_profile),
        // Advisory findings (non-exhaustive coverage / total mismatch) stay
        // Warnings regardless of profile.
        ux_rules::Severity::Warn => DoctorSeverity::Warning,
    };
    let (line, column) = line_col_for_offset(source, finding.line);
    DoctorDiagnostic {
        path: path.to_path_buf(),
        line,
        column,
        severity,
        code: finding.code.to_owned(),
        message: finding.message,
        category: Some(RuleCategory::Correctness),
        feature_name: Some(finding.feature),
        construct: None,
        fix: None,
        group: None,
    }
}

fn date_range_finding_to_diagnostic(
    finding: date_range_filter::Finding,
    path: &Path,
    source: &str,
    security_profile: SecurityProfile,
) -> DoctorDiagnostic {
    let (line, column) = line_col_for_offset(source, finding.line);
    DoctorDiagnostic {
        path: path.to_path_buf(),
        line,
        column,
        severity: severity_for(date_range_filter::Finding::CODE, security_profile),
        code: date_range_filter::Finding::CODE.to_owned(),
        message: finding.message,
        category: Some(RuleCategory::Correctness),
        feature_name: Some(finding.feature),
        construct: None,
        fix: None,
        group: None,
    }
}

fn severity_for(code: &str, security_profile: SecurityProfile) -> DoctorSeverity {
    doctor_severity_for(
        code,
        RuleCategory::Correctness,
        security_profile,
        &std::collections::BTreeMap::new(),
    )
}
