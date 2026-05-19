//! Analyzer check passes that run over lowered IR without aborting lowering.

pub mod lifecycle_gate;
pub mod route_guard;

pub fn run_checks(
    module: &mut lazuli_ir::ExperienceModule,
    app: Option<&lazuli_ir::AppManifest>,
    features: &[lazuli_ir::Feature],
) -> Vec<route_guard::RouteGuardDiagnostic> {
    route_guard::check(module, app, features)
}

pub fn run_lifecycle_gate_checks(
    module: &mut lazuli_ir::ExperienceModule,
    app: Option<&lazuli_ir::AppManifest>,
    features: &[lazuli_ir::Feature],
) -> Vec<lifecycle_gate::LifecycleGateDiagnostic> {
    lifecycle_gate::check(module, app, features)
}
