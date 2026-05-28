//! ROUTE-LIFECYCLE-CANONICAL-FORM-001 — both `requires_lifecycle <R>
//! = <state>` shorthand AND `requires_lifecycle_in <R> [<state>]`
//! single-element forms appear in the same project. Per §6.5 the
//! allow-list form is canonical for any list shape; this diagnostic
//! steers projects toward consistency.
//!
//! ## Rule statement
//!
//! Fires when a project (one `ExperienceModule`) uses BOTH the
//! shorthand form `requires_lifecycle <R> = <state>` AND the
//! single-element allow-list form `requires_lifecycle_in <R>
//! [<state>]` for the same resource on different guards. The two
//! shapes are semantically equivalent (per §6.5) and the
//! coexistence is what doctor steers the author away from.
//!
//! Additionally, the proposal §3 of the Cell A spec calls out: fire
//! when the singular shorthand is used more than once in a pilot
//! (advise `_in [<state>]` for grep-ability). This rule fires when
//! a project has 2+ shorthand `requires_lifecycle` slots — the cue
//! is "you're typing the shorthand at scale; switch to the
//! grep-friendly allow-list form".
//!
//! ## Severity profile
//!
//! Severity: `warning` at strict, `error` at production. The
//! `lazuli_doctor` severity table can override this per-profile;
//! the rule reports `warning` and the dispatcher escalates at
//! production.
//!
//! ## Trigger cue
//!
//! Cue: project has BOTH shorthand and `_in` allow-list shapes for
//! the same resource, OR the shorthand appears in 2+ sites on the
//! same module.
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! §4.3 + §6.5.

use std::path::{Path, PathBuf};

use lazuli_ir::{ExperienceModule, ViewGuard};

/// One ROUTE-LIFECYCLE-CANONICAL-FORM-001 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    /// Cause that fired the rule (the project mixes forms, OR the
    /// shorthand appears at scale).
    pub cause: CanonicalFormCause,
    /// The resource the cue points at, when scoped per-resource.
    pub resource: Option<String>,
}

/// Why the canonical-form rule fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalFormCause {
    /// Both forms (`requires_lifecycle <R> = <s>` AND
    /// `requires_lifecycle_in <R> [<s>]`) appear on the same project.
    MixedForms {
        shorthand_sites: usize,
        in_form_sites: usize,
    },
    /// The shorthand appears in 2+ sites for the same resource;
    /// suggest the `_in` form for grep-ability.
    ShorthandAtScale { count: usize },
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-LIFECYCLE-CANONICAL-FORM-001";

    /// Render the "canonical form" guidance message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::lifecycle_canonical_form_001::{
    ///     CanonicalFormCause, Finding,
    /// };
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     cause: CanonicalFormCause::ShorthandAtScale { count: 3 },
    ///     resource: Some("Host".into()),
    /// };
    /// assert!(f.message().contains("3 sites"));
    /// ```
    pub fn message(&self) -> String {
        match &self.cause {
            CanonicalFormCause::MixedForms {
                shorthand_sites,
                in_form_sites,
            } => format!(
                "Project mixes `requires_lifecycle {} = ...` ({} site(s)) and `requires_lifecycle_in {} [...]` ({} site(s)) — pick the canonical allow-list form (per §6.5) for both, or collapse to the shorthand if the project's convention is single-state.",
                self.resource.as_deref().unwrap_or("<resource>"),
                shorthand_sites,
                self.resource.as_deref().unwrap_or("<resource>"),
                in_form_sites,
            ),
            CanonicalFormCause::ShorthandAtScale { count } => format!(
                "Resource `{}` uses the shorthand `requires_lifecycle = <state>` in {} sites — consider switching to `requires_lifecycle_in {} [<state>]` for grep-ability and consistency with multi-state allow-lists (per §6.5).",
                self.resource.as_deref().unwrap_or("<resource>"),
                count,
                self.resource.as_deref().unwrap_or("<resource>"),
            ),
        }
    }
}

/// Walk the project's guards once and emit canonical-form findings:
/// (1) mixed forms (both shorthand AND `_in` for the same resource),
/// (2) shorthand at scale (2+ shorthand sites for one resource with
/// no `_in` companion).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::lifecycle_canonical_form_001::check;
///
/// let module: lazuli_ir::ExperienceModule = unimplemented!("lower");
/// let findings = check(&module, Path::new("hostpoint.lzx"));
/// for f in findings {
///     eprintln!("canonical-form: {:?} on {:?}", f.cause, f.resource);
/// }
/// ```
pub fn check(module: &ExperienceModule, path: &Path) -> Vec<Finding> {
    let mut shorthand_by_resource: std::collections::BTreeMap<String, usize> = Default::default();
    let mut in_form_by_resource: std::collections::BTreeMap<String, usize> = Default::default();

    let mut visit_guard = |guard: &ViewGuard| {
        if let Some(rl) = guard.requires_lifecycle.as_ref() {
            *shorthand_by_resource
                .entry(rl.resource.clone())
                .or_default() += 1;
        }
        if let Some(rli) = guard.requires_lifecycle_in.as_ref() {
            *in_form_by_resource.entry(rli.resource.clone()).or_default() += 1;
        }
    };
    for route in &module.routes {
        if let Some(guard) = route.guard.as_ref() {
            visit_guard(guard);
        }
    }
    for experience in &module.experiences {
        for view in &experience.views {
            if let Some(guard) = view.guard.as_ref() {
                visit_guard(guard);
            }
        }
    }

    let mut out = Vec::new();
    // 1) Mixed forms: same resource appears in both maps.
    for (resource, shorthand_sites) in &shorthand_by_resource {
        if let Some(in_form_sites) = in_form_by_resource.get(resource) {
            out.push(Finding {
                path: path.to_path_buf(),
                cause: CanonicalFormCause::MixedForms {
                    shorthand_sites: *shorthand_sites,
                    in_form_sites: *in_form_sites,
                },
                resource: Some(resource.clone()),
            });
        }
    }
    // 2) Shorthand at scale: same resource uses shorthand in 2+ sites
    //    AND no `_in` form exists (otherwise the mixed-form variant
    //    already covers the resource).
    for (resource, count) in &shorthand_by_resource {
        if *count >= 2 && !in_form_by_resource.contains_key(resource) {
            out.push(Finding {
                path: path.to_path_buf(),
                cause: CanonicalFormCause::ShorthandAtScale { count: *count },
                resource: Some(resource.clone()),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppRoute, ExperienceModule, RequiresLifecycle, RequiresLifecycleIn, ViewGuard,
    };

    fn mk_route(name: &str, guard: ViewGuard) -> AppRoute {
        AppRoute {
            name: name.into(),
            path: Some(format!("/{name}")),
            routes: Vec::new(),
            route_params: Vec::new(),
            to: None,
            surface: None,
            audience: None,
            lazy: None,
            prerender: None,
            guard: Some(guard),
            loaders: Vec::new(),
            pending_view: None,
            error_view: None,
            parent: None,
            span_ref: None,
        }
    }

    fn mk_shorthand_guard(resource: &str) -> ViewGuard {
        ViewGuard {
            requires_lifecycle: Some(RequiresLifecycle {
                resource: resource.into(),
                state: "pending".into(),
                substep: None,
                span_ref: None,
            }),
            ..ViewGuard::default()
        }
    }

    fn mk_in_guard(resource: &str) -> ViewGuard {
        ViewGuard {
            requires_lifecycle_in: Some(RequiresLifecycleIn {
                resource: resource.into(),
                allowed_states: vec!["pending".into()],
                span_ref: None,
            }),
            ..ViewGuard::default()
        }
    }

    #[test]
    fn fires_mixed_forms_when_project_uses_both_shapes_for_same_resource() {
        let module = ExperienceModule {
            app: None,
            routes: vec![
                mk_route("a", mk_shorthand_guard("Host")),
                mk_route("b", mk_in_guard("Host")),
            ],
            experiences: Vec::new(),
            surfaces: Vec::new(),
        };
        let findings = check(&module, Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert!(matches!(
            findings[0].cause,
            CanonicalFormCause::MixedForms { .. }
        ));
        assert_eq!(findings[0].resource.as_deref(), Some("Host"));
        assert_eq!(Finding::CODE, "ROUTE-LIFECYCLE-CANONICAL-FORM-001");
    }

    #[test]
    fn fires_shorthand_at_scale_when_shorthand_used_twice_for_same_resource() {
        let module = ExperienceModule {
            app: None,
            routes: vec![
                mk_route("a", mk_shorthand_guard("Host")),
                mk_route("b", mk_shorthand_guard("Host")),
            ],
            experiences: Vec::new(),
            surfaces: Vec::new(),
        };
        let findings = check(&module, Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert!(matches!(
            findings[0].cause,
            CanonicalFormCause::ShorthandAtScale { count: 2 }
        ));
    }

    #[test]
    fn quiet_when_single_shorthand_use_and_no_in_form() {
        // 1 shorthand usage = below the at-scale threshold AND no
        // mixed-form trigger. Stay silent.
        let module = ExperienceModule {
            app: None,
            routes: vec![mk_route("a", mk_shorthand_guard("Host"))],
            experiences: Vec::new(),
            surfaces: Vec::new(),
        };
        assert!(check(&module, Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_only_in_form_is_used_across_project() {
        let module = ExperienceModule {
            app: None,
            routes: vec![
                mk_route("a", mk_in_guard("Host")),
                mk_route("b", mk_in_guard("Host")),
                mk_route("c", mk_in_guard("Traveler")),
            ],
            experiences: Vec::new(),
            surfaces: Vec::new(),
        };
        assert!(check(&module, Path::new("hostpoint.lzx")).is_empty());
    }
}
