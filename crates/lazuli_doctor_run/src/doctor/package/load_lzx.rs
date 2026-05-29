//! Per-file `.lzx` processor for `DoctorPackage::load`.
//!
//! Walks one `DoctorFile` whose path is a `.lzx` source and pushes
//! the lifted `LzxDocument` plus its derived facts into the shared
//! `LoadAccumulator`. Also fires the Wave 3.5 + Wave 4 view test
//! discipline rules (`TEST-VIEW-E2E-MISSING-001`,
//! `TEST-VIEW-EXTENSIBILITY-001`, `TEST-VIEW-DRIFT-001`) under
//! profiles >= `Strict`, with severity escalation honoring the
//! `tdd-iron-hand` preset when configured.

use lazuli_doctor_config::DoctorProfile as SecurityProfile;

use super::super::helpers::resolve_test_discipline_severity;
use super::super::{
    DoctorDiagnostic, DoctorFile, DoctorSeverity, collect_lzx_experience_facts,
    collect_lzx_operational_facts, line_col_for_offset,
};
use super::state::{LoadAccumulator, LoadContext};

pub(super) fn process_lzx_file(
    file: &mut DoctorFile,
    acc: &mut LoadAccumulator,
    ctx: &LoadContext<'_>,
) {
    match lazuli_syntax::parse_lzx_document(&file.source) {
        Ok(document) => {
            collect_lzx_experience_facts(&document, &mut acc.experiences);
            collect_lzx_operational_facts(file, &document, &mut acc.operational);
            // Wave 3.5 + Wave 4 — view test_discipline rules. All
            // operate on the LzxDocument or its lowered IR. Severity
            // per profile: prototype=info, strict=warning,
            // production=error. W1.5: under
            // [doctor.test_discipline].preset = "tdd-iron-hand",
            // every rule below escalates to Error.
            if ctx.security_profile != SecurityProfile::Prototype {
                fire_view_test_discipline(file, ctx, &document);
            }
            file.lzx = Some(document);
        }
        Err(error) => file.local_diagnostics.push(DoctorDiagnostic {
            path: file.path.clone(),
            line: line_col_for_offset(&file.source, error.span().start).0,
            column: line_col_for_offset(&file.source, error.span().start).1,
            severity: DoctorSeverity::Error,
            code: "LZX-PARSE".to_owned(),
            message: error.to_string(),
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        }),
    }
}

fn fire_view_test_discipline(
    file: &mut DoctorFile,
    ctx: &LoadContext<'_>,
    document: &lazuli_syntax::LzxDocument,
) {
    let severity_warn = match ctx.security_profile {
        SecurityProfile::Production => DoctorSeverity::Error,
        _ => DoctorSeverity::Warning,
    };
    // W1.5 — resolve preset once per .lzx file.
    let lzx_test_discipline_preset = ctx
        .lazurite_manifest
        .as_ref()
        .and_then(|m| m.doctor.as_ref())
        .and_then(|d| d.test_discipline.as_ref())
        .and_then(|td| td.preset.as_deref())
        .and_then(lazuli_doctor::test_discipline::preset::TestDisciplinePreset::parse);
    // Wave 3.5 — TEST-VIEW-E2E-MISSING-001 (file-pair). Doctor does
    // NOT invoke Playwright; only Path::exists().
    let view_e2e_code = lazuli_doctor::test_discipline::test_view_e2e_missing_001::Finding::CODE;
    // 2026-05-27 — honor `[testing.playwright].discovery_root` from
    // Lazurite.toml so multi-client pilots (app/clients/<name>/e2e/)
    // stop emitting false missing-spec warnings pointing at the bare
    // ./e2e/ path that doesn't exist in their layout.
    let playwright_discovery_root: Option<std::path::PathBuf> = ctx
        .lazurite_manifest
        .as_ref()
        .and_then(|m| m.testing_playwright_resolved(&ctx.project_root))
        .and_then(|pw| pw.discovery_root)
        .map(std::path::PathBuf::from);
    for finding in
        lazuli_doctor::test_discipline::test_view_e2e_missing_001::check_with_discovery_root(
            document,
            &file.path,
            &file.source,
            &ctx.project_root,
            playwright_discovery_root.as_deref(),
        )
    {
        let message = finding.message();
        file.local_diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: finding.line,
            column: finding.column,
            severity: resolve_test_discipline_severity(
                severity_warn,
                view_e2e_code,
                lzx_test_discipline_preset,
            ),
            code: view_e2e_code.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    // Wave 4 — TEST-VIEW-EXTENSIBILITY-001 + TEST-VIEW-DRIFT-001.
    // Both walk the lowered ExperienceModule.
    let experience_module = lazuli_analyzer::lower_lzx_document(document);
    let view_ext_code = lazuli_doctor::test_discipline::test_view_extensibility_001::Finding::CODE;
    for finding in lazuli_doctor::test_discipline::test_view_extensibility_001::check(
        &experience_module,
        &file.path,
    ) {
        let message = finding.message();
        file.local_diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_test_discipline_severity(
                severity_warn,
                view_ext_code,
                lzx_test_discipline_preset,
            ),
            code: view_ext_code.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    let view_drift_code = lazuli_doctor::test_discipline::test_view_drift_001::Finding::CODE;
    for finding in
        lazuli_doctor::test_discipline::test_view_drift_001::check(&experience_module, &file.path)
    {
        let message = finding.message();
        file.local_diagnostics.push(DoctorDiagnostic {
            path: finding.path,
            line: 1,
            column: 1,
            severity: resolve_test_discipline_severity(
                DoctorSeverity::Error,
                view_drift_code,
                lzx_test_discipline_preset,
            ),
            code: view_drift_code.to_owned(),
            message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
}
