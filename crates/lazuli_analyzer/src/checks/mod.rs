//! Analyzer check passes that run over lowered IR without aborting lowering.
//!
//! Each submodule owns one named pass (route-guard, lifecycle-gate,
//! lifecycle-transition, audience-policy, scalar-fixtures) and emits a
//! pass-specific `Diagnostic` shape. The `run_*` entry points in this
//! file are the canonical wiring the CLI / doctor / LSP use to invoke
//! the passes; nothing here does the actual analysis — it lives in the
//! submodules.

pub mod audience_policy;
pub mod lifecycle_gate;
pub mod lifecycle_transition;
pub mod route_guard;
pub mod scalar_fixtures;

use std::path::Path;

/// Run the combined route-guard + audience-policy pass and fold the
/// audience-policy diagnostics into the route-guard shape so callers
/// can consume a single diagnostic stream.
///
/// Used by the CLI's `lazuli build` and `lazuli doctor` to surface
/// route-level guard issues alongside their advisory siblings.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::checks::run_checks;
/// use lazuli_ir::{AppManifest, ExperienceModule, Feature};
///
/// let mut module: ExperienceModule = unimplemented!();
/// let diags = run_checks(&mut module, None, &[]);
/// assert!(diags.iter().all(|d| !d.code.is_empty()));
/// ```
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

/// Run the audience-policy pass in isolation, returning its native
/// [`audience_policy::Diagnostic`] shape.
///
/// Used by LSP completion + hover for "policy missing here" advisories
/// that don't need the route-guard folding.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::checks::run_audience_policy_checks;
/// use lazuli_ir::ExperienceModule;
///
/// let module: ExperienceModule = unimplemented!();
/// let diags = run_audience_policy_checks(&module);
/// assert!(diags.iter().all(|d| !d.code.is_empty()));
/// ```
pub fn run_audience_policy_checks(
    module: &lazuli_ir::ExperienceModule,
) -> Vec<audience_policy::Diagnostic> {
    audience_policy::check(module)
}

/// Run the LAZ-87 lifecycle-gate pass on a module — wired into doctor
/// and the CLI as a separate cell from route-guard so each can be
/// disabled independently.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::checks::run_lifecycle_gate_checks;
/// use lazuli_ir::ExperienceModule;
///
/// let mut module: ExperienceModule = unimplemented!();
/// let diags = run_lifecycle_gate_checks(&mut module, None, &[]);
/// assert!(diags.iter().all(|d| !d.code.is_empty()));
/// ```
pub fn run_lifecycle_gate_checks(
    module: &mut lazuli_ir::ExperienceModule,
    app: Option<&lazuli_ir::AppManifest>,
    features: &[lazuli_ir::Feature],
) -> Vec<lifecycle_gate::LifecycleGateDiagnostic> {
    lifecycle_gate::check(module, app, features)
}

/// Run the lifecycle-transition pass over feature IR (validates the
/// resource state-machine declarations: every `transition <state> ->
/// <state>` references a state that exists, etc.).
///
/// ## Examples
///
/// ```
/// use lazuli_analyzer::checks::run_lifecycle_transition_checks;
///
/// let diags = run_lifecycle_transition_checks(&[]);
/// assert!(diags.is_empty());
/// ```
pub fn run_lifecycle_transition_checks(
    features: &[lazuli_ir::Feature],
) -> Vec<lifecycle_transition::Diagnostic> {
    lifecycle_transition::check(features)
}

/// Run the scalar-fixtures pass — validates that every fixture file
/// under `project_root` matches the resource schema it declares.
///
/// ## Examples
///
/// ```no_run
/// use std::path::Path;
/// use lazuli_analyzer::checks::run_scalar_fixtures_checks;
///
/// let diags = run_scalar_fixtures_checks(Path::new("."));
/// assert!(diags.iter().all(|d| !d.code.is_empty()));
/// ```
pub fn run_scalar_fixtures_checks(project_root: &Path) -> Vec<scalar_fixtures::Diagnostic> {
    scalar_fixtures::check(project_root)
}
