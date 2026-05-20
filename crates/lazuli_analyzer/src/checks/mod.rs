//! Analyzer check passes that run over lowered IR without aborting lowering.

pub mod audience_policy;
pub mod lifecycle_gate;
pub mod lifecycle_transition;
pub mod route_guard;
pub mod scalar_fixtures;

use std::path::Path;

pub fn run_checks(
    module: &mut lazuli_ir::ExperienceModule,
    app: Option<&lazuli_ir::AppManifest>,
    features: &[lazuli_ir::Feature],
) -> Vec<route_guard::RouteGuardDiagnostic> {
    let mut diagnostics = route_guard::check(module, app, features);
    diagnostics.extend(
        audience_policy::check(module)
            .into_iter()
            .map(|diagnostic| route_guard::RouteGuardDiagnostic {
                code: diagnostic.code,
                severity: route_guard::RouteGuardSeverity::Info,
                origin: route_guard::RouteGuardOrigin::Lzx,
                span: diagnostic.span,
                message: diagnostic.message,
            }),
    );
    diagnostics
}

pub fn run_audience_policy_checks(
    module: &lazuli_ir::ExperienceModule,
) -> Vec<audience_policy::Diagnostic> {
    audience_policy::check(module)
}

pub fn run_lifecycle_gate_checks(
    module: &mut lazuli_ir::ExperienceModule,
    app: Option<&lazuli_ir::AppManifest>,
    features: &[lazuli_ir::Feature],
) -> Vec<lifecycle_gate::LifecycleGateDiagnostic> {
    lifecycle_gate::check(module, app, features)
}

pub fn run_lifecycle_transition_checks(
    features: &[lazuli_ir::Feature],
) -> Vec<lifecycle_transition::Diagnostic> {
    lifecycle_transition::check(features)
}

pub fn run_scalar_fixtures_checks(project_root: &Path) -> Vec<scalar_fixtures::Diagnostic> {
    scalar_fixtures::check(project_root)
}
