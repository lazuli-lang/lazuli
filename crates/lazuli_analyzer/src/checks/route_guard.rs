//! LAZ-67 route-guard analyzer entry point.
//!
//! The implementation lives in `route_guard_helpers` to keep this pass
//! entry file within the Cell ANALYZE-1 size budget.

use lazuli_ir::{AppManifest, ExperienceModule, Feature, SpanRef};

#[path = "route_guard_helpers.rs"]
mod route_guard_helpers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteGuardSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteGuardOrigin {
    App,
    Lzx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteGuardDiagnostic {
    pub code: &'static str,
    pub severity: RouteGuardSeverity,
    pub origin: RouteGuardOrigin,
    pub span: Option<SpanRef>,
    pub message: String,
}

pub fn check(
    module: &mut ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
) -> Vec<RouteGuardDiagnostic> {
    route_guard_helpers::check(module, app, features)
}
