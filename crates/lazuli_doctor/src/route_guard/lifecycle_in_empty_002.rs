//! ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002 — `requires_lifecycle_in <R>
//! []` declares an empty allow-list, which makes the view
//! unreachable (no actor lifecycle state satisfies the gate).
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles.
//!
//! ## Trigger cue
//!
//! Cue: a `requires_lifecycle_in` slot with an empty
//! [`lazuli_ir::RequiresLifecycleIn::allowed_states`] vector. Either
//! drop the slot entirely or populate the allow-list.
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md` §4.3.

use std::path::{Path, PathBuf};

use lazuli_ir::{ExperienceModule, ViewGuard};

/// One ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002 finding — an empty
/// allow-list on a `requires_lifecycle_in` slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub owner: String,
    pub resource: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002";

    /// Render the "empty allow-list = unreachable view" message and
    /// steer the author to either add states or drop the slot.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::lifecycle_in_empty_002::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     owner: "route `home`".into(),
    ///     resource: "Host".into(),
    /// };
    /// assert!(f.message().contains("unreachable"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares `requires_lifecycle_in {} []` with an empty allow-list — the view is unreachable. Add states or drop the slot.",
            self.owner, self.resource
        )
    }
}

/// Walk every guard in `module` and flag the ones whose
/// `requires_lifecycle_in` slot has an empty allow-list.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::lifecycle_in_empty_002::check;
///
/// let module: lazuli_ir::ExperienceModule = unimplemented!("lower an .lzx");
/// let findings = check(&module, Path::new("hostpoint.lzx"));
/// assert!(findings.is_empty() || findings[0].resource.len() > 0);
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

/// Single-guard variant of [`check`] for callers iterating guards
/// elsewhere (LSP per-view diagnostic surfaces).
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::lifecycle_in_empty_002::check_guard;
///
/// let guard: lazuli_ir::ViewGuard = unimplemented!("build a guard");
/// let _ = check_guard(&guard, "route `home`".into(), Path::new("x.lzx"));
/// ```
pub fn check_guard(guard: &ViewGuard, owner: String, path: &Path) -> Option<Finding> {
    let rli = guard.requires_lifecycle_in.as_ref()?;
    if !rli.allowed_states.is_empty() {
        return None;
    }
    Some(Finding {
        path: path.to_path_buf(),
        owner,
        resource: rli.resource.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{AppRoute, ExperienceModule, RequiresLifecycleIn, ViewGuard};

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
    fn fires_when_allow_list_is_empty() {
        let guard = ViewGuard {
            requires_lifecycle_in: Some(RequiresLifecycleIn {
                resource: "Host".into(),
                allowed_states: vec![],
                span_ref: None,
            }),
            ..ViewGuard::default()
        };
        let module = mk_module(vec![mk_route("home", Some(guard))]);
        let findings = check(&module, Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].resource, "Host");
        assert_eq!(Finding::CODE, "ROUTE-GUARD-LIFECYCLE-IN-EMPTY-002");
        assert!(findings[0].message().contains("unreachable"));
    }

    #[test]
    fn quiet_when_allow_list_has_states() {
        let guard = ViewGuard {
            requires_lifecycle_in: Some(RequiresLifecycleIn {
                resource: "Host".into(),
                allowed_states: vec!["pending".into()],
                span_ref: None,
            }),
            ..ViewGuard::default()
        };
        let module = mk_module(vec![mk_route("home", Some(guard))]);
        assert!(check(&module, Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_slot_is_absent() {
        let guard = ViewGuard::default();
        let module = mk_module(vec![mk_route("home", Some(guard))]);
        assert!(check(&module, Path::new("hostpoint.lzx")).is_empty());
    }
}
