//! LAZ-67 route-guard analyzer entry point.
//!
//! The implementation lives in `route_guard_helpers` to keep this pass
//! entry file within the Cell ANALYZE-1 size budget.
//!
//! The pass validates that every `.lzx` route declares (or inherits) a
//! valid audience policy + unauthorized/unauthenticated redirect, and
//! emits [`RouteGuardDiagnostic`]s consumed by doctor / LSP / inspect.

use lazuli_ir::{AppManifest, ExperienceModule, Feature, SpanRef};

#[path = "route_guard_helpers/mod.rs"]
mod route_guard_helpers;

/// Severity bucket for a [`RouteGuardDiagnostic`].
///
/// Mirrors the doctor severity ladder: `Error` blocks the build,
/// `Warning` surfaces in diagnostics-only mode, `Info` is advisory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteGuardSeverity {
    /// Hard rejection — build fails.
    Error,
    /// Visible in diagnostics-only mode.
    Warning,
    /// Advisory; suggests a missing-but-recommended slot.
    Info,
}

/// Which surface the diagnostic was lifted from.
///
/// `App` means the finding originated from the app manifest's
/// `route_guard` defaults; `Lzx` means it came from a per-route block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteGuardOrigin {
    /// Diagnostic originates in the app manifest.
    App,
    /// Diagnostic originates in an `.lzx` route block.
    Lzx,
}

/// One emitted finding from the route-guard pass.
///
/// Carries the doctor-style code (e.g. `ROUTE-GUARD-MISSING-POLICY-001`),
/// the severity, the originating surface, an optional `SpanRef` for IDE
/// underlining, and a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteGuardDiagnostic {
    /// Stable diagnostic code (e.g. `"ROUTE-GUARD-MISSING-POLICY-001"`).
    pub code: &'static str,
    /// Severity bucket — drives whether the build blocks.
    pub severity: RouteGuardSeverity,
    /// Surface the diagnostic was lifted from.
    pub origin: RouteGuardOrigin,
    /// Span for IDE underlining; `None` when the finding is module-wide.
    pub span: Option<SpanRef>,
    /// Human-readable message — already formatted, no further interpolation.
    pub message: String,
}

/// Run the LAZ-67 route-guard pass on a full [`ExperienceModule`].
///
/// May mutate `module` to attach resolution hints used by codegen. The
/// pass walks every route and validates audience policy presence,
/// redirect-target resolution, and inherited app-level defaults.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_analyzer::checks::route_guard;
/// use lazuli_ir::{AppManifest, ExperienceModule, Feature};
///
/// let mut module: ExperienceModule = unimplemented!();
/// let app: Option<&AppManifest> = None;
/// let features: Vec<Feature> = vec![];
/// let diags = route_guard::check(&mut module, app, &features);
/// assert!(diags.iter().all(|d| !d.code.is_empty()));
/// ```
pub fn check(
    module: &mut ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
) -> Vec<RouteGuardDiagnostic> {
    route_guard_helpers::check(module, app, features)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_module_produces_no_diagnostics() {
        let mut module = ExperienceModule {
            app: None,
            routes: vec![],
            experiences: vec![],
            surfaces: vec![],
        };
        let diags = check(&mut module, None, &[]);
        assert!(diags.is_empty());
    }
}
