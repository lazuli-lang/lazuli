//! Design-token aggregator.
//!
//! Surfaces the L0 #1 / L0 #2 design system checks: per-allowlist token
//! resolution (`token_undefined`), hex/px/font/shadow leak detection,
//! and the three Error-severity `design custom` rules
//! (`design-custom-duplicate`, `design-custom-invalid-value`,
//! `design-custom-reserved-name`).
//!
//! The five leak/undefined checks honor the active security profile via
//! `doctor_rule_severity`; the three `design custom` rules pin Error
//! because they represent structural collisions (reserved-name reuse,
//! duplicate names, malformed values) that always produce wrong code if
//! ignored.
//!
//! When `design.lzi` is missing or fails to parse/lower, the IR-driven
//! rules (`design::custom::*`) suppress instead of firing false
//! positives — `read_design_ir` returns `None` and the block is skipped.
//!
//! See `docs/proposals/design-tokens-custom.md` §4 for the `custom` 9th
//! meta-group catalog and the closed-token contract.

use std::path::Path;

use lazuli_lsp::SecurityProfile;

use crate::doctor::design;
use crate::doctor::{
    DoctorDiagnostic, DoctorSeverity, doctor_rule_path, doctor_rule_severity, read_design_ir,
};

/// Aggregate every design-token finding into the canonical
/// `DoctorDiagnostic` envelope. Returns an empty vec when no
/// `design.tokens.lzi` allowlist is authored (the leak / undefined
/// rules have nothing to compare against).
pub(crate) fn diagnostics(
    project_root: &Path,
    security_profile: SecurityProfile,
) -> Vec<DoctorDiagnostic> {
    let Some(allowlist) = design::read_allowlist(project_root) else {
        return Vec::new();
    };

    let severity = doctor_rule_severity(security_profile);
    let mut diagnostics = Vec::new();

    diagnostics.extend(
        design::token_undefined::check(project_root, &allowlist)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::token_undefined::Finding::CODE.to_owned(),
                    message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }
            }),
    );
    diagnostics.extend(
        design::hex_leak::check(project_root)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::hex_leak::Finding::CODE.to_owned(),
                    message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }
            }),
    );
    diagnostics.extend(
        design::px_leak::check(project_root)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::px_leak::Finding::CODE.to_owned(),
                    message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }
            }),
    );
    diagnostics.extend(
        design::fontfamily_leak::check(project_root, &allowlist)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::fontfamily_leak::Finding::CODE.to_owned(),
                    message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }
            }),
    );
    diagnostics.extend(
        design::shadow_leak::check(project_root)
            .into_iter()
            .map(|finding| {
                let message = finding.message();
                DoctorDiagnostic {
                    path: doctor_rule_path(project_root, finding.path),
                    line: finding.line,
                    column: 1,
                    severity,
                    code: design::shadow_leak::Finding::CODE.to_owned(),
                    message,
                    category: None,
                    feature_name: None,
                    construct: None,
                    fix: None,
                    group: None,
                }
            }),
    );

    // L0 #2 — `custom` 9th meta-group lints per
    // `docs/proposals/design-tokens-custom.md` §4. Three Error-severity
    // rules over the lowered `Design` IR. Severity is hard-coded to Error
    // regardless of security_profile — these are structural collisions
    // (reserved-name reuse, duplicate names, malformed values) that always
    // produce wrong code if ignored.
    if let Some(design) = read_design_ir(project_root) {
        let design_path = project_root.join("design.lzi");
        let display_path = doctor_rule_path(project_root, design_path);
        diagnostics.extend(
            design::custom::check_duplicate(&design)
                .into_iter()
                .map(|finding| {
                    let message = finding.message();
                    DoctorDiagnostic {
                        path: display_path.clone(),
                        line: 1,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: design::custom::DuplicateFinding::CODE.to_owned(),
                        message,
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    }
                }),
        );
        diagnostics.extend(
            design::custom::check_invalid_value(&design)
                .into_iter()
                .map(|finding| {
                    let message = finding.message();
                    DoctorDiagnostic {
                        path: display_path.clone(),
                        line: 1,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: design::custom::InvalidValueFinding::CODE.to_owned(),
                        message,
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    }
                }),
        );
        diagnostics.extend(
            design::custom::check_reserved_name(&design)
                .into_iter()
                .map(|finding| {
                    let message = finding.message();
                    DoctorDiagnostic {
                        path: display_path.clone(),
                        line: 1,
                        column: 1,
                        severity: DoctorSeverity::Error,
                        code: design::custom::ReservedNameFinding::CODE.to_owned(),
                        message,
                        category: None,
                        feature_name: None,
                        construct: None,
                        fix: None,
                        group: None,
                    }
                }),
        );
    }

    diagnostics
}
