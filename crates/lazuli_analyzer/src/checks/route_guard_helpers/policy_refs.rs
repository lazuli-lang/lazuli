//! Cross-surface policy reference collection (ROUTE-POLICY-001).
//!
//! Determines which policies are referenced from route-guards vs.
//! command/query/api surfaces so we can flag `when_denied_route`
//! declarations that only apply to non-route consumers.

use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{AppManifest, ExperienceModule, Feature, PolicyRef, ViewGuard};

use super::{
    RouteGuardDiagnostic, RouteGuardOrigin, RouteGuardSeverity, route_feature_from_name,
    surface_feature, target_view,
};
use super::backend_policies::{effective_policy, query_policy};

pub(super) fn check_when_denied_route_policy_use(
    module: &ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    let route_refs = collect_route_policy_refs(module, app, features);
    let nonroute_refs = collect_nonroute_policy_refs(features);
    let mut declared = BTreeMap::new();
    for feature in features {
        for category in &feature.policies.categories {
            if category.when_denied_route.is_some() {
                declared.insert(
                    (feature.name.clone(), category.name.clone()),
                    category.name.clone(),
                );
            }
        }
    }
    for (key, name) in declared {
        if nonroute_refs.contains(&key) && !route_refs.contains(&key) {
            out.push(RouteGuardDiagnostic {
                code: "ROUTE-POLICY-001",
                severity: RouteGuardSeverity::Error,
                origin: RouteGuardOrigin::App,
                span: None,
                message: format!(
                    "policy `{}` declares `when_denied_route` but is referenced only by command/query/api surfaces; route-only denial targets must be used by a view guard.",
                    name
                ),
            });
        }
    }
}

fn collect_route_policy_refs(
    module: &ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for route in &module.routes {
        let default_feature = target_view(route.to.as_deref())
            .map(|(feature, _)| feature)
            .or_else(|| route.surface.as_deref().and_then(surface_feature))
            .unwrap_or_else(|| route_feature_from_name(&route.name));
        collect_guard_refs(route.guard.as_ref(), &default_feature, features, &mut out);
    }
    for experience in &module.experiences {
        for view in &experience.views {
            collect_guard_refs(view.guard.as_ref(), &experience.name, features, &mut out);
        }
    }
    for surface in &module.surfaces {
        for audience in &surface.audiences {
            collect_guard_refs(
                audience.guard.as_ref(),
                &surface.experience,
                features,
                &mut out,
            );
            for view in &audience.views {
                collect_guard_refs(view.guard.as_ref(), &surface.experience, features, &mut out);
            }
        }
    }
    if !module.routes.is_empty()
        && let Some(default_policy) = app
            .and_then(|app| app.route_guard.as_ref())
            .and_then(|defaults| defaults.default_policy.as_deref())
        && let Some(key) = policy_text_category(default_policy, "", features)
    {
        out.insert(key);
    }
    out
}

fn collect_guard_refs(
    guard: Option<&ViewGuard>,
    default_feature: &str,
    features: &[Feature],
    out: &mut BTreeSet<(String, String)>,
) {
    let Some(guard) = guard else {
        return;
    };
    for policy in &guard.policy {
        if let Some(key) = policy_text_category(policy, default_feature, features) {
            out.insert(key);
        }
    }
}

fn collect_nonroute_policy_refs(features: &[Feature]) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for feature in features {
        for command in &feature.commands {
            if let Some(key) = policy_ref_category(
                effective_policy(&command.policy, &feature.defaults.policy),
                &feature.name,
                features,
            ) {
                out.insert(key);
            }
        }
        for query in &feature.queries {
            if let Some(key) = policy_ref_category(
                effective_policy(query_policy(query), &feature.defaults.policy),
                &feature.name,
                features,
            ) {
                out.insert(key);
            }
        }
        for api in &feature.apis {
            if let Some(key) = policy_ref_category(Some(&api.policy), &feature.name, features) {
                out.insert(key);
            }
        }
    }
    out
}

fn policy_ref_category(
    policy: Option<&PolicyRef>,
    default_feature: &str,
    features: &[Feature],
) -> Option<(String, String)> {
    match policy? {
        PolicyRef::None => None,
        PolicyRef::Atom(atom) => atom
            .strip_prefix("policy.")
            .map(|name| (default_feature.to_owned(), name.to_owned())),
        PolicyRef::Local(name) => Some((default_feature.to_owned(), name.clone())),
        PolicyRef::External { feature, name } => Some((feature.clone(), name.clone())),
        PolicyRef::Unresolved(text) => policy_text_category(text, default_feature, features),
    }
}

fn policy_text_category(
    text: &str,
    default_feature: &str,
    features: &[Feature],
) -> Option<(String, String)> {
    let raw = text.trim().trim_start_matches('@');
    let tail = raw.strip_prefix("policy.")?;
    let mut parts = tail.splitn(2, '.');
    let first = parts.next().unwrap_or_default();
    if let Some(second) = parts.next()
        && features.iter().any(|feature| feature.name == first)
    {
        return Some((first.to_owned(), second.to_owned()));
    }
    (!default_feature.is_empty()).then(|| (default_feature.to_owned(), tail.to_owned()))
}
