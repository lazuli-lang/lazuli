//! Route-guard codegen — per-audience SDK files + a central registry
//! describing how the runtime should gate every authored route.
//!
//! Authors declare a `route_guard` block on the app, on a route, on a
//! view, on an experience, or on an audience. The emitter folds those
//! sources into a single resolved guard per route, groups routes by
//! `(feature, platform, audience)`, and writes one TS file per group
//! plus one cross-app `route-guards.gen.ts` registry. The runtime
//! consumes the registry; the per-audience files give app code typed
//! `RouteGuardSpec<typeof Screen>` consts to bind on.
//!
//! No-op contract: if no route, view, experience, audience, or app
//! authored a `route_guard`, this emitter returns an empty file list
//! so projects without guards stay byte-for-byte unchanged.
//!
//! Sub-modules:
//!
//! - `context` — resolves the surface/audience/experience that applies
//!   to a given route, building the `(group, view_guard, audience_guard,
//!   experience_guard)` tuple consumed by `resolve_route_guard`.
//! - `policy` — `@policy.<x>` resolution + atom parsing/sorting.
//! - `group` — `RouteGroupKey`, `ResolvedGuard`, `RouteObject`,
//!   `RouteRegistryEntry`, and the `ts_string` literal helper.
//! - `emit` — TS emission for both per-audience SDK files and the
//!   central `route-guards.gen.ts` registry.

mod context;
mod emit;
mod group;
mod policy;

use std::collections::BTreeMap;

use lazuli_ir::{AppManifest, AppRoute, Experience, Feature, Platform, PlatformSurface, ViewGuard};

use crate::GeneratedFile;

use self::context::{has_route_guard_surface, route_context};
use self::emit::{
    emit_audience_route_guard_sdk, emit_route_guard_registry, route_component_name,
    route_const_name,
};
use self::group::{ResolvedGuard, RouteGroupKey, RouteObject, RouteRegistryEntry};
use self::policy::{PolicyRefList, RoutePolicyAtom, build_policy_lookup, resolve_policy};

/// Target prefix for route-guard artifacts. Route guards are emitted beside
/// audience SDK files (`dist/ts-web/...` or `dist/ts-mobile/...`) without
/// changing runtime code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteGuardTarget {
    Web,
    Mobile,
}

impl RouteGuardTarget {
    pub(super) fn dist_prefix(self) -> &'static str {
        match self {
            RouteGuardTarget::Web => "ts-web",
            RouteGuardTarget::Mobile => "ts-mobile",
        }
    }

    pub(super) fn platform_label(self) -> &'static str {
        match self {
            RouteGuardTarget::Web => "web",
            RouteGuardTarget::Mobile => "mobile",
        }
    }

    pub(super) fn platform(self) -> Platform {
        match self {
            RouteGuardTarget::Web => Platform::Web,
            RouteGuardTarget::Mobile => Platform::Mobile,
        }
    }
}

/// Emit route-guard metadata artifacts for the new guard-bearing `.lzx` IR.
///
/// No-op contract: when no route/view/audience guard is declared and the app
/// has no `route_guard` block, this returns an empty vec even if
/// `actor_query` is set. That keeps existing projects byte-for-byte stable.
///
/// ## Examples
///
/// ```
/// use lazuli_codegen_ts::lzx_audience_slot::{emit_route_guard_artifacts, RouteGuardTarget};
/// let files = emit_route_guard_artifacts(None, &[], &[], &[], &[], RouteGuardTarget::Web);
/// assert!(files.is_empty());
/// ```
pub fn emit_route_guard_artifacts(
    app: Option<&AppManifest>,
    routes: &[AppRoute],
    surfaces: &[PlatformSurface],
    experiences: &[Experience],
    features: &[Feature],
    target: RouteGuardTarget,
) -> Vec<GeneratedFile> {
    if !has_route_guard_surface(app, routes, surfaces, experiences) {
        return Vec::new();
    }

    let policy_lookup = build_policy_lookup(features);
    let app_defaults = app.and_then(|app| app.route_guard.as_ref());
    let mut groups: BTreeMap<RouteGroupKey, Vec<RouteObject>> = BTreeMap::new();
    let mut registry_entries: Vec<RouteRegistryEntry> = Vec::new();

    for route in routes {
        let Some(path) = route.path.as_ref() else {
            continue;
        };

        let (group, view_guard, audience_guard, experience_guard) =
            route_context(route, surfaces, experiences, target);
        let default_feature = group.feature.as_str();
        let Some(guard) = resolve_route_guard(
            route.guard.as_ref(),
            view_guard,
            experience_guard,
            audience_guard,
            app_defaults,
            &policy_lookup,
            default_feature,
        ) else {
            continue;
        };

        let const_name = route_const_name(&route.name);
        let object = RouteObject {
            path: path.clone(),
            const_name: const_name.clone(),
            component: route_component_name(route),
            guard,
        };
        registry_entries.push(RouteRegistryEntry {
            path: path.clone(),
            const_name,
            group: group.clone(),
        });
        groups.entry(group).or_default().push(object);
    }

    for objects in groups.values_mut() {
        objects.sort_by(|a, b| a.const_name.cmp(&b.const_name));
    }
    registry_entries.sort_by(|a, b| a.path.cmp(&b.path).then(a.const_name.cmp(&b.const_name)));

    let mut files = Vec::new();
    for (group, objects) in &groups {
        files.push(GeneratedFile {
            path: group.file_path(target),
            contents: emit_audience_route_guard_sdk(objects),
        });
    }

    files.push(GeneratedFile {
        path: format!("dist/{}/app/route-guards.gen.ts", target.dist_prefix()),
        contents: emit_route_guard_registry(app, app_defaults, &policy_lookup, &registry_entries),
    });

    files
}

fn resolve_route_guard(
    route_guard: Option<&ViewGuard>,
    view_guard: Option<&ViewGuard>,
    experience_guard: Option<&ViewGuard>,
    audience_guard: Option<&ViewGuard>,
    app_defaults: Option<&lazuli_ir::RouteGuardDefaults>,
    policies: &BTreeMap<(String, String), Vec<RoutePolicyAtom>>,
    default_feature: &str,
) -> Option<ResolvedGuard> {
    let guard_chain = [route_guard, view_guard, experience_guard, audience_guard];
    let policy_texts = guard_chain
        .iter()
        .find_map(|guard| guard.map(|guard| guard.policy.policy_refs()))
        .or_else(|| {
            app_defaults
                .and_then(|defaults| defaults.default_policy.as_deref())
                .map(|policy| vec![policy])
        })?;
    let resolved_policies = policy_texts
        .into_iter()
        .map(|policy| resolve_policy(policy, policies, default_feature))
        .collect();
    let on_unauthenticated = guard_chain
        .iter()
        .find_map(|guard| guard.and_then(|guard| guard.on_unauthenticated.clone()))
        .or_else(|| app_defaults.and_then(|defaults| defaults.on_unauthenticated.clone()));
    let on_unauthorized = guard_chain
        .iter()
        .find_map(|guard| guard.and_then(|guard| guard.on_unauthorized.clone()))
        .or_else(|| app_defaults.and_then(|defaults| defaults.on_unauthorized.clone()));

    Some(ResolvedGuard {
        policies: resolved_policies,
        on_unauthenticated,
        on_unauthorized,
    })
}
