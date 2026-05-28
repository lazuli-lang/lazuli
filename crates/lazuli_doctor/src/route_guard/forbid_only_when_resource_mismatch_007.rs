//! ROUTE-GUARD-FORBID-ONLY-WHEN-RESOURCE-MISMATCH-007 — a
//! `forbid_when ... only_when lifecycle <R> = <state>` slot targets
//! a resource that doesn't appear in the rest of the guard.
//!
//! ## Severity profile
//!
//! Severity: `warning` (strict + production both). The shape may be
//! intentional in cross-resource gates (e.g., redirect host applicants
//! to /host when the *traveler* lifecycle is `complete`); the
//! diagnostic flags the divergence so authors confirm intent.
//!
//! ## Trigger cue
//!
//! Cue: the guard's other lifecycle slots (`requires_lifecycle`,
//! `requires_lifecycle_in`) target Resource A, but the
//! `only_when_lifecycle` sub-slot targets Resource B.
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! §4.3.

use std::path::{Path, PathBuf};

use lazuli_ir::{ExperienceModule, ViewGuard};

/// One ROUTE-GUARD-FORBID-ONLY-WHEN-RESOURCE-MISMATCH-007 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub owner: String,
    pub forbid_atom: String,
    pub only_when_resource: String,
    /// One of the other resources that DOES appear on the guard.
    pub guard_primary_resource: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-GUARD-FORBID-ONLY-WHEN-RESOURCE-MISMATCH-007";

    /// Render the "cross-resource composition" message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::forbid_only_when_resource_mismatch_007::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     owner: "route `home`".into(),
    ///     forbid_atom: "@role.host".into(),
    ///     only_when_resource: "Host".into(),
    ///     guard_primary_resource: "Traveler".into(),
    /// };
    /// assert!(f.message().contains("Traveler"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares `forbid_when {} ... only_when lifecycle {} = ...` but the rest of the guard targets `{}`. Confirm the cross-resource composition is intentional, or align the `only_when` resource.",
            self.owner,
            self.forbid_atom,
            self.only_when_resource,
            self.guard_primary_resource,
        )
    }
}

/// Walk every guard in `module` and flag `forbid_when ... only_when
/// lifecycle <R> ...` slots whose resource diverges from the guard's
/// primary lifecycle resource.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::forbid_only_when_resource_mismatch_007::check;
///
/// let module: lazuli_ir::ExperienceModule = unimplemented!("lower");
/// let _ = check(&module, Path::new("hostpoint.lzx"));
/// ```
pub fn check(module: &ExperienceModule, path: &Path) -> Vec<Finding> {
    let mut out = Vec::new();
    for route in &module.routes {
        if let Some(guard) = route.guard.as_ref() {
            out.extend(check_guard(guard, format!("route `{}`", route.name), path));
        }
    }
    for experience in &module.experiences {
        for view in &experience.views {
            if let Some(guard) = view.guard.as_ref() {
                out.extend(check_guard(
                    guard,
                    format!("view `{}.{}`", experience.name, view.name),
                    path,
                ));
            }
        }
    }
    out
}

fn check_guard(guard: &ViewGuard, owner: String, path: &Path) -> Vec<Finding> {
    let other_resource = primary_resource(guard);
    let Some(primary) = other_resource else {
        // No other lifecycle slot on the guard → the only_when stands
        // alone; rule does not fire.
        return Vec::new();
    };
    guard
        .forbid_when
        .iter()
        .filter_map(|fw| {
            let owl = fw.only_when_lifecycle.as_ref()?;
            if owl.resource == primary {
                None
            } else {
                Some(Finding {
                    path: path.to_path_buf(),
                    owner: owner.clone(),
                    forbid_atom: fw.atom_ref.clone(),
                    only_when_resource: owl.resource.clone(),
                    guard_primary_resource: primary.clone(),
                })
            }
        })
        .collect()
}

fn primary_resource(guard: &ViewGuard) -> Option<String> {
    if let Some(rl) = guard.requires_lifecycle.as_ref() {
        return Some(rl.resource.clone());
    }
    if let Some(rli) = guard.requires_lifecycle_in.as_ref() {
        return Some(rli.resource.clone());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppRoute, ExperienceModule, ForbidWhen, PolicyAtom, RequiresLifecycle, ViewGuard,
    };

    fn mk_fw(atom: &str, only_when_resource: Option<&str>) -> ForbidWhen {
        ForbidWhen {
            atom_ref: atom.into(),
            atom: PolicyAtom {
                namespace: "role".into(),
                name: "host".into(),
                args: None,
            },
            dispatch_to: "/x".into(),
            only_when_lifecycle: only_when_resource.map(|r| RequiresLifecycle {
                resource: r.into(),
                state: "complete".into(),
                substep: None,
                span_ref: None,
            }),
            span_ref: None,
        }
    }

    fn mk_module(guard: ViewGuard) -> ExperienceModule {
        ExperienceModule {
            app: None,
            routes: vec![AppRoute {
                name: "home".into(),
                path: Some("/home".into()),
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
            }],
            experiences: Vec::new(),
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn fires_when_only_when_resource_differs_from_guard_primary() {
        let guard = ViewGuard {
            requires_lifecycle: Some(RequiresLifecycle {
                resource: "Traveler".into(),
                state: "pending".into(),
                substep: None,
                span_ref: None,
            }),
            forbid_when: vec![mk_fw("@role.host", Some("Host"))],
            ..ViewGuard::default()
        };
        let findings = check(&mk_module(guard), Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].only_when_resource, "Host");
        assert_eq!(findings[0].guard_primary_resource, "Traveler");
        assert_eq!(
            Finding::CODE,
            "ROUTE-GUARD-FORBID-ONLY-WHEN-RESOURCE-MISMATCH-007"
        );
    }

    #[test]
    fn quiet_when_only_when_resource_matches_guard_primary() {
        let guard = ViewGuard {
            requires_lifecycle: Some(RequiresLifecycle {
                resource: "Host".into(),
                state: "pending".into(),
                substep: None,
                span_ref: None,
            }),
            forbid_when: vec![mk_fw("@role.host", Some("Host"))],
            ..ViewGuard::default()
        };
        assert!(check(&mk_module(guard), Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_guard_has_no_other_lifecycle_slot() {
        // No `requires_lifecycle` / `requires_lifecycle_in` → there's
        // no "primary" resource to disagree with.
        let guard = ViewGuard {
            forbid_when: vec![mk_fw("@role.host", Some("Host"))],
            ..ViewGuard::default()
        };
        assert!(check(&mk_module(guard), Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_forbid_when_has_no_only_when_lifecycle() {
        let guard = ViewGuard {
            requires_lifecycle: Some(RequiresLifecycle {
                resource: "Host".into(),
                state: "pending".into(),
                substep: None,
                span_ref: None,
            }),
            forbid_when: vec![mk_fw("@role.host", None)],
            ..ViewGuard::default()
        };
        assert!(check(&mk_module(guard), Path::new("hostpoint.lzx")).is_empty());
    }
}
