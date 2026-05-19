//! Analyzer check passes that run over lowered IR without aborting lowering.

pub mod route_guard;

pub fn run_checks(
    module: &mut lazuli_ir::ExperienceModule,
    app: Option<&lazuli_ir::AppManifest>,
    features: &[lazuli_ir::Feature],
) -> Vec<route_guard::RouteGuardDiagnostic> {
    route_guard::check(module, app, features)
}
