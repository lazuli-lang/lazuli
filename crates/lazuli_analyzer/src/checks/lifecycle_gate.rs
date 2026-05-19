//! LAZ-87 lifecycle-gate analyzer entry point.
//!
//! The implementation lives in `lifecycle_gate_helpers` so this public entry
//! file stays small and mirrors the LAZ-67 route-guard pass.

use lazuli_ir::{AppManifest, ExperienceModule, Feature, SpanRef};

#[path = "lifecycle_gate_helpers.rs"]
mod lifecycle_gate_helpers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleGateSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleGateOrigin {
    App,
    Lzx,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateDiagnostic {
    pub code: &'static str,
    pub severity: LifecycleGateSeverity,
    pub origin: LifecycleGateOrigin,
    pub span: Option<SpanRef>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LifecycleGateInput {
    pub views: Vec<LifecycleGateView>,
    pub resumes: Vec<LifecycleGateResume>,
    pub app_on_lifecycle_pending: Option<String>,
    pub app_span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateView {
    pub feature: String,
    pub name: String,
    pub policy_present: bool,
    pub requires: Option<RequiresLifecycle>,
    pub on_lifecycle_pending: Option<String>,
    pub span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiresLifecycle {
    pub resource: String,
    pub state: String,
    pub span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateResume {
    pub feature: String,
    pub name: String,
    pub source: Option<LifecycleGateResumeSource>,
    pub arms: Vec<LifecycleGateResumeArm>,
    pub span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateResumeSource {
    pub feature: Option<String>,
    pub kind: Option<String>,
    pub query: String,
    pub text: String,
    pub span: Option<SpanRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateResumeArm {
    pub state: String,
    pub target_view: String,
    pub span: Option<SpanRef>,
}

pub fn check(
    module: &mut ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
) -> Vec<LifecycleGateDiagnostic> {
    lifecycle_gate_helpers::check(module, app, features)
}

pub fn check_input(
    input: &LifecycleGateInput,
    features: &[Feature],
) -> Vec<LifecycleGateDiagnostic> {
    lifecycle_gate_helpers::check_input(input, features)
}
