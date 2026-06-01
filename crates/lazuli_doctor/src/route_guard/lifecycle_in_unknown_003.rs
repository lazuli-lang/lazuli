//! ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003 — `requires_lifecycle_in
//! <R> [s1, s2, ...]` references a state that the resource's
//! lifecycle does not declare.
//!
//! ## Severity profile
//!
//! Severity: `error` in both strict and production profiles.
//!
//! ## Trigger cue
//!
//! Cue: an allow-list entry that doesn't appear in
//! `feature.resources[].lifecycle.states[].name`. The doctor walks
//! the supplied features to resolve the resource by name.
//!
//! ## Proposal anchor
//!
//! Per `docs/proposals/ir-route-guard-escape-hatch-2026-05-28.md`
//! §4.3.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use lazuli_ir::{ExperienceModule, Feature, ViewGuard};

/// One ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003 finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub owner: String,
    pub resource: String,
    /// The state name that doesn't exist on the resource's lifecycle.
    pub unknown_state: String,
}

impl Finding {
    /// Stable diagnostic code emitted with this finding.
    pub const CODE: &'static str = "ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003";

    /// Render the "unknown lifecycle state in allow-list" message.
    ///
    /// ## Examples
    ///
    /// ```ignore
    /// use std::path::PathBuf;
    /// use lazuli_doctor::route_guard::lifecycle_in_unknown_003::Finding;
    ///
    /// let f = Finding {
    ///     path: PathBuf::from("hostpoint.lzx"),
    ///     owner: "route `home`".into(),
    ///     resource: "Host".into(),
    ///     unknown_state: "typo_state".into(),
    /// };
    /// assert!(f.message().contains("typo_state"));
    /// ```
    pub fn message(&self) -> String {
        format!(
            "{} declares `requires_lifecycle_in {} [..., {state}, ...]` but `{state}` is not a declared lifecycle state on resource `{}`.",
            self.owner,
            self.resource,
            self.resource,
            state = self.unknown_state,
        )
    }
}

/// Walk every guard in `module` and flag allow-list entries that
/// don't appear in the named resource's lifecycle declaration.
///
/// ## Examples
///
/// ```ignore
/// use std::path::Path;
/// use lazuli_doctor::route_guard::lifecycle_in_unknown_003::check;
///
/// let module: lazuli_ir::ExperienceModule = unimplemented!("lower");
/// let features: Vec<lazuli_ir::Feature> = vec![];
/// let _ = check(&module, &features, Path::new("hostpoint.lzx"));
/// ```
pub fn check(module: &ExperienceModule, features: &[Feature], path: &Path) -> Vec<Finding> {
    let states_by_resource = build_states_index(features);
    let mut out = Vec::new();
    for route in &module.routes {
        if let Some(guard) = route.guard.as_ref() {
            out.extend(check_guard(
                guard,
                format!("route `{}`", route.name),
                &states_by_resource,
                path,
            ));
        }
    }
    for experience in &module.experiences {
        for view in &experience.views {
            if let Some(guard) = view.guard.as_ref() {
                out.extend(check_guard(
                    guard,
                    format!("view `{}.{}`", experience.name, view.name),
                    &states_by_resource,
                    path,
                ));
            }
        }
    }
    out
}

fn check_guard(
    guard: &ViewGuard,
    owner: String,
    states_by_resource: &std::collections::BTreeMap<String, BTreeSet<String>>,
    path: &Path,
) -> Vec<Finding> {
    let Some(rli) = guard.requires_lifecycle_in.as_ref() else {
        return Vec::new();
    };
    let Some(declared) = states_by_resource.get(&rli.resource) else {
        // Resource not found — out of scope for this rule (a sibling
        // diagnostic should catch unknown-resource references).
        return Vec::new();
    };
    rli.allowed_states
        .iter()
        .filter(|state| !declared.contains(state.as_str()))
        .map(|state| Finding {
            path: path.to_path_buf(),
            owner: owner.clone(),
            resource: rli.resource.clone(),
            unknown_state: state.clone(),
        })
        .collect()
}

fn build_states_index(
    features: &[Feature],
) -> std::collections::BTreeMap<String, BTreeSet<String>> {
    let mut out: std::collections::BTreeMap<String, BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for feature in features {
        for resource in &feature.resources {
            let Some(lifecycle) = resource.lifecycle.as_ref() else {
                continue;
            };
            let entry = out.entry(resource.name.clone()).or_default();
            for state in &lifecycle.states {
                entry.insert(state.name.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazuli_ir::{
        AppRoute, Defaults, ExperienceModule, Feature, Lifecycle, LifecycleState,
        LifecycleStateKind, Policies, RequiresLifecycleIn, Resource, ViewGuard,
    };

    fn mk_feature_with_resource(resource_name: &str, states: Vec<&str>) -> Feature {
        let lifecycle = Lifecycle {
            discriminator_field: "status".into(),
            generated_enum: format!("{}Status", resource_name),
            states: states
                .into_iter()
                .map(|s| LifecycleState {
                    name: s.into(),
                    kind: LifecycleStateKind::Intermediate,
                    span_ref: None,
                })
                .collect(),
            transitions: Vec::new(),
            invariants: Vec::new(),
            invariant_handlers: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        };
        Feature {
            name: "test_feat".into(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            knowledge: None,
            defaults: Defaults::default(),
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: vec![Resource {
                name: resource_name.into(),
                public_contract: None,
                tenancy: None,
                soft_delete: false,
                soft_delete_actor: false,
                timestamps: None,
                fields: Vec::new(),
                constraints: Vec::new(),
                validate: None,
                validates: Vec::new(),
                retention: None,
                previous_names: Vec::new(),
                span_ref: None,
                lifecycle: Some(lifecycle),
                invariants: Vec::new(),
                lock: None,
                composite_key: None,
                conventions: Vec::new(),
                lifecycle_routes: None,
                polymorphic_refs: Vec::new(),
                many_through: Vec::new(),
                restrict_on_delete: Vec::new(),
                append_only: false,
            }],
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies::default(),
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: Vec::new(),
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: Vec::new(),
            mcp_servers: Vec::new(),
            previous_names: Vec::new(),
            synth_origins: std::collections::BTreeMap::new(),
            span_ref: None,
        }
    }

    fn mk_route_module(states_authored: Vec<&str>) -> ExperienceModule {
        let guard = ViewGuard {
            requires_lifecycle_in: Some(RequiresLifecycleIn {
                resource: "Host".into(),
                allowed_states: states_authored.into_iter().map(|s| s.into()).collect(),
                span_ref: None,
            }),
            ..ViewGuard::default()
        };
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
    fn fires_for_unknown_state_in_allow_list() {
        let feature = mk_feature_with_resource("Host", vec!["pending", "complete"]);
        let module = mk_route_module(vec!["pending", "typo_state"]);

        let findings = check(&module, &[feature], Path::new("hostpoint.lzx"));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].unknown_state, "typo_state");
        assert_eq!(Finding::CODE, "ROUTE-GUARD-LIFECYCLE-IN-UNKNOWN-003");
        assert!(findings[0].message().contains("typo_state"));
    }

    #[test]
    fn quiet_when_all_states_are_declared() {
        let feature = mk_feature_with_resource("Host", vec!["pending", "complete"]);
        let module = mk_route_module(vec!["pending", "complete"]);
        assert!(check(&module, &[feature], Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn quiet_when_resource_is_not_in_feature_index() {
        // No features supplied → can't resolve resource; rule is
        // silent (a sibling rule catches unknown-resource references).
        let module = mk_route_module(vec!["pending"]);
        assert!(check(&module, &[], Path::new("hostpoint.lzx")).is_empty());
    }

    #[test]
    fn fires_for_multiple_unknown_states() {
        let feature = mk_feature_with_resource("Host", vec!["pending"]);
        let module = mk_route_module(vec!["typo1", "typo2"]);
        let findings = check(&module, &[feature], Path::new("hostpoint.lzx"));
        assert_eq!(findings.len(), 2);
    }
}
