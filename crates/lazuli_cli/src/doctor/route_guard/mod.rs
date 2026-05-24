//! Doctor bridge for LAZ-67 route-guard analyzer diagnostics.

use lazuli_analyzer::checks::route_guard::{RouteGuardOrigin, RouteGuardSeverity};
use lazuli_analyzer::checks::run_checks;

use super::{DoctorAppManifest, DoctorDiagnostic, DoctorFile, DoctorSeverity, Tier3FeatureFacts};

pub(super) fn diagnostics(
    files: &[DoctorFile],
    app: Option<&DoctorAppManifest>,
    facts: &[Tier3FeatureFacts],
) -> Vec<DoctorDiagnostic> {
    let features: Vec<lazuli_ir::Feature> = facts
        .iter()
        .map(|fact| {
            let mut feature = super::make_synthetic_feature_for_error_vocab(fact);
            feature.queries = fact.queries.clone();
            feature.apis = fact.apis.clone();
            feature
        })
        .collect();

    let mut module = lazuli_ir::ExperienceModule {
        app: None,
        routes: Vec::new(),
        experiences: Vec::new(),
        surfaces: Vec::new(),
    };
    let mut lzx_path = None;
    for file in files {
        let Some(document) = file.lzx.as_ref() else {
            continue;
        };
        lzx_path.get_or_insert_with(|| file.path.clone());
        let lowered = lazuli_analyzer::lower_lzx_document(document);
        if module.app.is_none() {
            module.app = lowered.app;
        }
        module.routes.extend(lowered.routes);
        module.experiences.extend(lowered.experiences);
        module.surfaces.extend(lowered.surfaces);
    }
    if lzx_path.is_none() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for finding in run_checks(&mut module, app.map(|a| &a.manifest), &features) {
        let path = match finding.origin {
            RouteGuardOrigin::App => app
                .map(|a| a.path.clone())
                .or_else(|| lzx_path.clone())
                .expect("route guard diagnostics require a .lzx path"),
            RouteGuardOrigin::Lzx => lzx_path
                .clone()
                .expect("route guard diagnostics require a .lzx path"),
        };
        let line = super::span_line(files, &path, finding.span, 1);
        out.push(DoctorDiagnostic {
            path,
            line,
            column: 1,
            severity: match finding.severity {
                RouteGuardSeverity::Error => DoctorSeverity::Error,
                RouteGuardSeverity::Warning => DoctorSeverity::Warning,
                RouteGuardSeverity::Info => DoctorSeverity::Info,
            },
            code: finding.code.to_owned(),
            message: finding.message,
            category: None,
            feature_name: None,
            construct: None,
            fix: None,
            group: None,
        });
    }
    out
}
