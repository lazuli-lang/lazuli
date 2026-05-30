//! `app.routes.gen.ts` emitter — builds the per-audience route table the
//! frontend router consumes.
//!
//! The IR carries the routes in their portable Express-style form; this
//! module groups them by audience, resolves guard / lifecycle emit specs
//! against the surrounding features, and renders one TS file per
//! platform.

use std::collections::BTreeMap;

use lazuli_ir::{AppManifest, AppRoute, Experience, Feature, Platform, PlatformSurface};

use crate::GeneratedFile;
use crate::lzx::lzx_router_adapter::{RouterTarget, translate_route_path};

mod emit;
mod resolve;
mod spec;

use emit::emit_routes_file;
use resolve::{
    resolve_field_gate_emit, resolve_guard_emit, resolve_lifecycle_emit, resolve_lifecycle_in_emit,
};
use spec::{LoaderEmit, RouteSpec};

/// Which platform the routes emitter is rendering for.
///
/// Drives both the `dist/` prefix and the router-dialect translation
/// (`$param` vs `[param]`) via [`Self::router_target`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoutesTarget {
    /// Web frontend — emits under `dist/ts-web/`, TanStack Router dialect.
    Web,
    /// Mobile frontend — emits under `dist/ts-mobile/`, Expo Router dialect.
    Mobile,
}

impl RoutesTarget {
    fn dist_prefix(self) -> &'static str {
        match self {
            RoutesTarget::Web => "ts-web",
            RoutesTarget::Mobile => "ts-mobile",
        }
    }

    fn platform_label(self) -> &'static str {
        match self {
            RoutesTarget::Web => "web",
            RoutesTarget::Mobile => "mobile",
        }
    }

    fn platform(self) -> Platform {
        match self {
            RoutesTarget::Web => Platform::Web,
            RoutesTarget::Mobile => Platform::Mobile,
        }
    }

    fn router_target(self) -> RouterTarget {
        match self {
            RoutesTarget::Web => RouterTarget::ViteReact,
            RoutesTarget::Mobile => RouterTarget::Expo,
        }
    }
}

/// Emit one `app.routes.gen.ts` per audience for the given target.
///
/// Filters routes to those owned by the target platform, groups by
/// audience, resolves each route's guard + lifecycle emission against
/// the surrounding features, and renders the resulting TS files. Routes
/// without a `path` are dropped silently — the analyzer is the
/// authority on what counts as a valid route.
///
/// ## Examples
///
/// ```ignore
/// use lazuli_codegen_ts::routes::{emit_routes_artifacts, RoutesTarget};
///
/// let files = emit_routes_artifacts(None, &[], &[], &[], &[], RoutesTarget::Web);
/// assert!(files.is_empty());
/// ```
pub fn emit_routes_artifacts(
    _app: Option<&AppManifest>,
    routes: &[AppRoute],
    surfaces: &[PlatformSurface],
    experiences: &[Experience],
    features: &[Feature],
    target: RoutesTarget,
) -> Vec<GeneratedFile> {
    let mut groups: BTreeMap<String, Vec<RouteSpec>> = BTreeMap::new();
    for route in routes {
        let Some(path) = route.path.as_deref() else {
            continue;
        };
        if !route_matches_target(route, surfaces, experiences, target) {
            continue;
        }
        let audience = route.audience.as_deref().unwrap_or("default").to_owned();
        groups.entry(audience.clone()).or_default().push(RouteSpec {
            name: route.name.clone(),
            path: translate_route_path(target.router_target(), path),
            audience,
            component_key: lower_camel(&route.name),
            route_const: route_const_name(&route.name),
            guard_emit: route
                .guard
                .as_ref()
                .and_then(|g| resolve_guard_emit(g, features)),
            lifecycle_emit: route.guard.as_ref().and_then(|g| {
                // `ir-route-guard-escape-hatch-2026-05-28` §5 Cell B-1
                // — exact-match wins when both shapes are authored
                // (doctor `ROUTE-GUARD-LIFECYCLE-EXCLUSIVE-001` rejects
                // the conflict at lint time; codegen prefers the
                // shipped form for byte-identical legacy emit).
                if let Some(rl) = &g.requires_lifecycle {
                    resolve_lifecycle_emit(rl, features)
                } else if let Some(rli) = &g.requires_lifecycle_in {
                    resolve_lifecycle_in_emit(rli, features)
                } else {
                    None
                }
            }),
            field_gates: route
                .guard
                .as_ref()
                .map(|g| {
                    g.requires_field
                        .iter()
                        .filter_map(|rf| resolve_field_gate_emit(rf, features))
                        .collect()
                })
                .unwrap_or_default(),
            loaders: route
                .loaders
                .iter()
                .map(|l| LoaderEmit {
                    feature: l.feature.clone(),
                    query_export: lower_camel_export(&l.query),
                })
                .collect(),
            pending_component_key: route.pending_view.as_ref().map(|v| lower_camel(v)),
            error_component_key: route.error_view.as_ref().map(|v| lower_camel(v)),
            parent_route_const: route.parent.as_ref().map(|p| route_const_name(p)),
            lazy: route.lazy.unwrap_or(false),
            route_params: route.route_params.clone(),
        });
    }

    let mut files = Vec::new();
    for (audience, specs) in groups.iter_mut() {
        specs.sort_by(|a, b| a.route_const.cmp(&b.route_const));
        specs.dedup_by(|a, b| a.route_const == b.route_const);
        files.push(GeneratedFile {
            path: format!("dist/{}/{}/routes.gen.tsx", target.dist_prefix(), audience),
            contents: emit_routes_file(specs),
        });
    }
    files
}

fn route_matches_target(
    route: &AppRoute,
    surfaces: &[PlatformSurface],
    experiences: &[Experience],
    target: RoutesTarget,
) -> bool {
    if let Some(surface) = route.surface.as_deref()
        && let Some(platform) = surface_platform_label(surface)
    {
        return platform == target.platform_label();
    }
    let Some(surface) = matching_surface(route, surfaces, experiences, target) else {
        return target == RoutesTarget::Web;
    };
    surface.platform == target.platform()
}

fn matching_surface<'a>(
    route: &AppRoute,
    surfaces: &'a [PlatformSurface],
    _experiences: &[Experience],
    target: RoutesTarget,
) -> Option<&'a PlatformSurface> {
    let route_feature = route_target_feature(route.to.as_deref())
        .or_else(|| route.surface.as_deref().and_then(surface_feature))
        .unwrap_or_else(|| route_feature_from_name(&route.name));
    surfaces
        .iter()
        .filter(|surface| surface.platform == target.platform())
        .find(|surface| {
            route
                .surface
                .as_deref()
                .map(|label| surface_matches(surface, label))
                .unwrap_or(surface.experience == route_feature)
        })
}

pub(super) fn snake_case(s: &str) -> String {
    // Resource names are PascalCase; convert to snake_case for the
    // lookup_my_<snake> query name + helper export.
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_lowercase());
    }
    out
}

pub(super) fn lower_camel_export(s: &str) -> String {
    super::runtime::lower_camel_export(s)
}

fn surface_matches(surface: &PlatformSurface, label: &str) -> bool {
    label == surface.experience
        || label == format!("{} web", surface.experience)
        || label == format!("{} mobile", surface.experience)
        || label == format!("{}.web", surface.experience)
        || label == format!("{}.mobile", surface.experience)
}

fn surface_platform_label(surface: &str) -> Option<&'static str> {
    let tail = surface.rsplit([' ', '.']).next()?;
    match tail {
        "web" => Some("web"),
        "mobile" => Some("mobile"),
        _ => None,
    }
}

fn surface_feature(surface: &str) -> Option<String> {
    surface
        .split([' ', '.'])
        .next()
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
}

fn route_target_feature(to: Option<&str>) -> Option<String> {
    let target = to?.split('(').next()?.trim();
    let (feature, _) = target.split_once(".view.")?;
    (!feature.is_empty()).then(|| feature.to_owned())
}

fn route_feature_from_name(name: &str) -> String {
    name.split('_')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("app")
        .to_owned()
}

fn route_const_name(name: &str) -> String {
    format!("{}Route", lower_camel(name))
}

fn lower_camel(value: &str) -> String {
    let pascal = pascal_case(value);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}

pub(super) fn pascal_case(value: &str) -> String {
    let mut out = String::new();
    for word in value.split(['_', '-', ' ']) {
        if word.is_empty() {
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for upper in first.to_uppercase() {
                out.push(upper);
            }
        }
        out.push_str(&chars.as_str().to_ascii_lowercase());
    }
    out
}
