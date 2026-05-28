//! ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001 — a single guard declares both
//! `requires_lifecycle` (exact-match) AND `requires_lifecycle_in`
//! (allow-list) for the same resource. The two slots are mutually
//! exclusive.
//!
//! ## Rule statement
//!
//! Fires when a `ViewGuard` (route, view, or audience guard) holds
//! both [`lazuli_ir::ViewGuard::requires_lifecycle`] AND
//! [`lazuli_ir::ViewGuard::requires_lifecycle_in`]. The author MUST
//! pick one form — the doctor catches the cross-form ambiguity at
//! lint time so codegen never has to resolve the conflict.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles.
//!
//! ## Trigger cue
//!
//! Cue: a guard that mixes `requires_lifecycle Host = pending` with
//! `requires_lifecycle_in Host [pending, complete]` — pick exactly
//! one. The allow-list form is canonical for any list shape
//! (`ROUTE-LIFECYCLE-CANONICAL-FORM-001`).
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! §4.3 + §4.2.

use std::path::{Path, PathBuf};

use lazuli_ir::{AppRoute, ExperienceModule, ExperienceView, ViewGuard};

/// One ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001 finding — a guard mixes
/// the exact-match and allow-list lifecycle slots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Source `.lzx` file the offending guard was authored in.
    pub path: PathBuf,
    /// Human-readable owner — `"route <name>"` for AppRoute guards,
    /// `"view <experience>.<view>"` for ExperienceView guards.
    pub owner: String,
    /// PascalCase resource name (matches the exact-match slot's resource).
    pub resource: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001";

    /// Render the "mutually exclusive" message, steering the author
    /// to keep one form and delete the other.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::lifecycle_exclusive_001::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     owner: "route `host_home`".into(),
    ///     resource: "Host".into(),
    /// };
    /// assert!(f.message().contains("pick exactly one form"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares both `requires_lifecycle {} = ...` AND `requires_lifecycle_in {} [...]` — pick exactly one form. The allow-list form is canonical for any list shape (per `ROUTE-LIFECYCLE-CANONICAL-FORM-001`).",
            self.owner, self.resource, self.resource
        )
    }
}

/// Walk every guard in an [`ExperienceModule`] and flag the ones
/// that declare both lifecycle slots.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::lifecycle_exclusive_001::check;
///
/// let module: lazuli_ir::ExperienceModule = unimplemented!("lower an .lzx module");
/// let findings = check(&module, Path::new("hostpoint.lzx"));
/// for f in findings {
///     eprintln!("{}: {}", f.owner, f.resource);
/// }
/// ```
pub fn check(module: &ExperienceModule, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for route in &module.routes {
        if let Some(guard) = route.guard.as_ref()
            && let Some(f) = check_guard(guard, format!("route `{}`", route.name), path)
        {
            out.push(f);
        }
    }
    for experience in &module.experiences {
        for view in &experience.views {
            if let Some(guard) = view.guard.as_ref()
                && let Some(f) = check_guard(
                    guard,
                    format!("view `{}.{}`", experience.name, view.name),
                    path,
                )
            {
                out.push(f);
            }
        }
    }
    out
}

/// Helper: returns a finding when a single guard mixes the two slots.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::lifecycle_exclusive_001::check_guard;
///
/// let guard: lazuli_ir::ViewGuard = unimplemented!("build a guard");
/// let maybe = check_guard(&guard, "route `home`".into(), Path::new("hostpoint.lzx"));
/// assert!(maybe.is_none() || maybe.unwrap().resource == "Host");
/// ```
pub fn check_guard(guard: &ViewGuard, owner: String, path: &Path) -> Option<Finding> {
    let (Some(rl), Some(rli)) = (
        guard.requires_lifecycle.as_ref(),
        guard.requires_lifecycle_in.as_ref(),
    ) else {
        return None;
    };
    // Surface the exact-match resource (matches the canonical-form rule's anchor).
    let _ = rli; // resource may differ; the rule still fires per §4.3.
    Some(Finding {
        path: path.to_path_buf(),
        owner,
        resource: rl.resource.clone(),
    })
}

/// Convenience for callers that already iterate routes / views.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::lifecycle_exclusive_001::check_route;
///
/// let route: lazuli_ir::AppRoute = unimplemented!("build a route");
/// let _ = check_route(&route, Path::new("hostpoint.lzx"));
/// ```
pub fn check_route(route: &AppRoute, path: &Path) -> Option<Finding> {
    let guard = route.guard.as_ref()?;
    check_guard(guard, format!("route `{}`", route.name), path)
}

/// Convenience for callers iterating views.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::lifecycle_exclusive_001::check_view;
///
/// let view: lazuli_ir::ExperienceView = unimplemented!("build a view");
/// let _ = check_view("host", &view, Path::new("hostpoint.lzx"));
/// ```
pub fn check_view(experience: &str, view: &ExperienceView, path: &Path) -> Option<Finding> {
    let guard = view.guard.as_ref()?;
    check_guard(guard, format!("view `{}.{}`", experience, view.name), path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppRoute, ExperienceModule, RequiresLifecycle, RequiresLifecycleIn, ViewGuard,
    };

    fn mk_route(name: &str, guard: Option<ViewGuard>) -> AppRoute {
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
            guard,
            loaders: Vec::new(),
            pending_view: None,
            error_view: None,
            parent: None,
            span_ref: None,
        }
    }

    fn mk_module(routes: Vec<AppRoute>) -> ExperienceModule {
        ExperienceModule {
            app: None,
            routes,
            experiences: Vec::new(),
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn fires_when_guard_mixes_both_lifecycle_slots() {
        let guard = ViewGuard {
            requires_lifecycle: Some(RequiresLifecycle {
                resource: "Host".into(),
                state: "pending".into(),
                substep: None,
                span_ref: None,
            }),
            requires_lifecycle_in: Some(RequiresLifecycleIn {
                resource: "Host".into(),
                allowed_states: vec!["pending".into(), "complete".into()],
                span_ref: None,
            }),
            ..ViewGuard::default()
        };
        let module = mk_module(vec![mk_route("host_home", Some(guard))]);

        let findings = check(&module, Path::new("hostpoint.lzx"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Host");
        assert!(findings[0].owner.contains("host_home"));
        assert_eq!(Finding::CODE, "ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001");
        assert!(findings[0].message().contains("pick exactly one form"));
    }

    #[test]
    fn quiet_when_only_exact_match_form_is_used() {
        let guard = ViewGuard {
            requires_lifecycle: Some(RequiresLifecycle {
                resource: "Host".into(),
                state: "pending".into(),
                substep: None,
                span_ref: None,
            }),
            ..ViewGuard::default()
        };
        let module = mk_module(vec![mk_route("host_home", Some(guard))]);
        assert!(check(&module, Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_only_allow_list_form_is_used() {
        let guard = ViewGuard {
            requires_lifecycle_in: Some(RequiresLifecycleIn {
                resource: "Host".into(),
                allowed_states: vec!["pending".into()],
                span_ref: None,
            }),
            ..ViewGuard::default()
        };
        let module = mk_module(vec![mk_route("host_home", Some(guard))]);
        assert!(check(&module, Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn fires_when_view_guard_mixes_both_lifecycle_slots() {
        use lazuli_ir::{Experience, ExperienceView};
        let guard = ViewGuard {
            requires_lifecycle: Some(RequiresLifecycle {
                resource: "Traveler".into(),
                state: "complete".into(),
                substep: None,
                span_ref: None,
            }),
            requires_lifecycle_in: Some(RequiresLifecycleIn {
                resource: "Traveler".into(),
                allowed_states: vec!["pending".into(), "complete".into()],
                span_ref: None,
            }),
            ..ViewGuard::default()
        };
        let module = ExperienceModule {
            app: None,
            routes: Vec::new(),
            experiences: vec![Experience {
                name: "traveler".into(),
                imports: Vec::new(),
                views: vec![ExperienceView {
                    name: "home".into(),
                    anchor: None,
                    routes: Vec::new(),
                    extensible_by: Vec::new(),
                    source: None,
                    submit: None,
                    blocks: Vec::new(),
                    actions: Vec::new(),
                    opens: Vec::new(),
                    tests: Vec::new(),
                    guard: Some(guard),
                    resolved_guard_policy: None,
                    resolved_lifecycle_gate: None,
                    span_ref: None,
                }],
                resume_routers: Vec::new(),
                extensions: Vec::new(),
                span_ref: None,
            }],
            surfaces: Vec::new(),
        };
        let findings = check(&module, Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].owner.contains("traveler.home"));
    }
}
