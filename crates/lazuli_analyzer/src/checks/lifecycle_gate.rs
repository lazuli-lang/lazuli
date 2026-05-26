//! LAZ-87 lifecycle-gate analyzer entry point.
//!
//! The implementation lives in `lifecycle_gate_helpers` so this public entry
//! file stays small and mirrors the LAZ-67 route-guard pass.
//!
//! The pass cross-walks `view requires <resource>.<state>` declarations against
//! the resource lifecycle topology and emits structured
//! [`LifecycleGateDiagnostic`]s that doctor / LSP / inspect all consume from
//! the same shape.

use lazuli_ir::{AppManifest, ExperienceModule, Feature, SpanRef};

#[path = "lifecycle_gate_helpers/mod.rs"]
mod lifecycle_gate_helpers;

/// Severity bucket for a [`LifecycleGateDiagnostic`].
///
/// Mirrors the doctor severity ladder: `Error` blocks the build, `Warning`
/// surfaces in diagnostics-only mode, `Info` is purely advisory (e.g. a
/// lifecycle-pending fallback that the analyzer noticed wasn't set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleGateSeverity {
    /// Hard rejection — build fails.
    Error,
    /// Visible in diagnostics-only mode.
    Warning,
    /// Advisory; suggests a missing-but-recommended slot.
    Info,
}

/// Which surface the diagnostic was lifted from.
///
/// `App` means the finding originated from the app-manifest slot
/// (`app.on_lifecycle_pending`); `Lzx` means it came from a per-view
/// requires/resume in an `.lzx` file. Used by LSP to route diagnostics
/// back to the originating document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleGateOrigin {
    /// Diagnostic originates in the app manifest.
    App,
    /// Diagnostic originates in an `.lzx` view block.
    Lzx,
}

/// One emitted finding from the lifecycle-gate pass.
///
/// Carries the doctor-style code (e.g. `LIFECYCLE-GATE-MISSING-001`),
/// the severity, the originating surface, an optional `SpanRef` for IDE
/// underlining, and a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateDiagnostic {
    /// Stable diagnostic code (e.g. `"LIFECYCLE-GATE-MISSING-001"`).
    pub code: &'static str,
    /// Severity bucket — drives whether the build blocks.
    pub severity: LifecycleGateSeverity,
    /// Surface the diagnostic was lifted from.
    pub origin: LifecycleGateOrigin,
    /// Span for IDE underlining; `None` when the finding is module-wide.
    pub span: Option<SpanRef>,
    /// Human-readable message — already formatted, no further interpolation.
    pub message: String,
}

/// Pre-projected input shape for [`check_input`].
///
/// Mirrors the per-view + per-resume slots the gate actually reasons
/// about. Useful when callers (LSP, doctor) want to drive the gate
/// from already-projected IR without re-walking the full
/// [`ExperienceModule`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LifecycleGateInput {
    /// Views to validate (one per `view <name>` block across all features).
    pub views: Vec<LifecycleGateView>,
    /// Resume blocks to validate (one per `resume <name>` block).
    pub resumes: Vec<LifecycleGateResume>,
    /// App-level fallback target for lifecycle-pending state.
    pub app_on_lifecycle_pending: Option<String>,
    /// Span of the app manifest (drives `App`-origin diagnostics).
    pub app_span: Option<SpanRef>,
}

/// Projection of one `view <name>` block — the input the gate needs to
/// reason about a single view's lifecycle requirements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateView {
    /// Owning feature name.
    pub feature: String,
    /// View name (unique within the feature).
    pub name: String,
    /// Whether the view declares an audience policy at all.
    pub policy_present: bool,
    /// `requires <resource>.<state>` projection, if present.
    pub requires: Option<RequiresLifecycle>,
    /// Per-view fallback for lifecycle-pending state.
    pub on_lifecycle_pending: Option<String>,
    /// Source span (for IDE diagnostics).
    pub span: Option<SpanRef>,
}

/// Lifecycle requirement parsed from `view requires <resource>.<state>[.<substep>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiresLifecycle {
    /// Resource name (e.g. `"Customer"`).
    pub resource: String,
    /// Lifecycle state name (e.g. `"active"`).
    pub state: String,
    /// Optional substep within the state.
    pub substep: Option<String>,
    /// Source span (for IDE diagnostics).
    pub span: Option<SpanRef>,
}

/// Projection of one `resume <name>` block (state-machine resumes after
/// a lifecycle-pending suspension).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateResume {
    /// Owning feature name.
    pub feature: String,
    /// Resume block name.
    pub name: String,
    /// `from <query>` source declaration, if present.
    pub source: Option<LifecycleGateResumeSource>,
    /// Resume arms: one entry per `when <state>[.<substep>] -> <view>`.
    pub arms: Vec<LifecycleGateResumeArm>,
    /// Source span (for IDE diagnostics).
    pub span: Option<SpanRef>,
}

/// Source clause of a resume block (`from <feature>.<kind>.<query>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateResumeSource {
    /// Originating feature (optional — defaults to the resume's feature).
    pub feature: Option<String>,
    /// Source kind (e.g. `"query"`).
    pub kind: Option<String>,
    /// Target query name.
    pub query: String,
    /// Verbatim source text (for diagnostic messages).
    pub text: String,
    /// Source span.
    pub span: Option<SpanRef>,
}

/// One arm of a `resume` block — `when <state>[.<substep>] -> <view>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleGateResumeArm {
    /// Lifecycle state the arm matches.
    pub state: String,
    /// Optional substep refinement.
    pub substep: Option<String>,
    /// Target view to resume into.
    pub target_view: String,
    /// Source span (for IDE diagnostics).
    pub span: Option<SpanRef>,
}

/// Run the LAZ-87 lifecycle-gate pass on a full [`ExperienceModule`].
///
/// Convenience wrapper around [`check_input`] that projects the module
/// + app + features into the gate's input shape and forwards. May
/// mutate `module` to attach `lifecycle_pending` resolution hints.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::checks::lifecycle_gate;
/// use lazuli_ir::{AppManifest, ExperienceModule, Feature};
///
/// let mut module: ExperienceModule = unimplemented!();
/// let app: Option<&AppManifest> = None;
/// let features: Vec<Feature> = vec![];
/// let diags = lifecycle_gate::check(&mut module, app, &features);
/// assert!(diags.iter().all(|d| !d.code.is_empty()));
/// ```
pub fn check(
    module: &mut ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
) -> Vec<LifecycleGateDiagnostic> {
    lifecycle_gate_helpers::check(module, app, features)
}

/// Run the LAZ-87 gate against a pre-projected [`LifecycleGateInput`].
///
/// Lower-level entry point — used by doctor and LSP when they already
/// have the per-view / per-resume projection and don't want to re-walk
/// the full module.
///
/// ## Examples
///
/// ```
/// use lazuli_analyzer::checks::lifecycle_gate::{check_input, LifecycleGateInput};
///
/// let input = LifecycleGateInput::default();
/// let diags = check_input(&input, &[]);
/// assert!(diags.is_empty());
/// ```
pub fn check_input(
    input: &LifecycleGateInput,
    features: &[Feature],
) -> Vec<LifecycleGateDiagnostic> {
    lifecycle_gate_helpers::check_input(input, features)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_produces_no_diagnostics() {
        let input = LifecycleGateInput::default();
        assert!(check_input(&input, &[]).is_empty());
    }
}
