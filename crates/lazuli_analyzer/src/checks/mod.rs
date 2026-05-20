//! Analyzer check passes that run over lowered IR without aborting lowering.

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
    route_guard::check(module, app, features)
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
