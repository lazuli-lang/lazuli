//! Route-context resolution — picks a `(group, view_guard, audience_guard,
//! experience_guard)` tuple for each declared route.
//!
//! Routes can pull guard metadata from four chained sources: the route
//! itself, the matching view on the audience, the surface's experience,
//! or the audience block as a whole. This module walks `surfaces` and
//! `experiences` to find which records apply for a given route, and
//! returns the candidate guards in the documented chain order.
//!
//! `has_route_guard_surface` is the cheap up-front predicate that lets
//! the emitter return an empty `Vec` for projects with no route guards
//! at all — keeping pre-route-guard apps byte-for-byte stable.

use lazuli_ir::{AppManifest, AppRoute, Experience, PlatformSurface, ViewGuard};

use super::RouteGuardTarget;
use super::group::RouteGroupKey;

pub(super) fn has_route_guard_surface(
    app: Option<&AppManifest>,
    routes: &[AppRoute],
    surfaces: &[PlatformSurface],
    experiences: &[Experience],
) -> bool {
    app.and_then(|app| app.route_guard.as_ref()).is_some()
        || routes.iter().any(|route| route.guard.is_some())
        || surfaces.iter().any(|surface| {
            surface.audiences.iter().any(|audience| {
                audience.guard.is_some() || audience.views.iter().any(|view| view.guard.is_some())
            })
        })
        || experiences
            .iter()
            .any(|experience| experience.views.iter().any(|view| view.guard.is_some()))
}

pub(super) fn route_context<'a>(
    route: &AppRoute,
    surfaces: &'a [PlatformSurface],
    experiences: &'a [Experience],
    target: RouteGuardTarget,
) -> (
    RouteGroupKey,
    Option<&'a ViewGuard>,
    Option<&'a ViewGuard>,
    Option<&'a ViewGuard>,
) {
    let view_name =
        route_target_view_name(route.to.as_deref()).unwrap_or_else(|| route.name.clone());
    let route_surface = route.surface.as_deref();
    let route_audience = route.audience.as_deref().unwrap_or("default");
    let route_feature = route_target_feature(route.to.as_deref())
        .or_else(|| surface_feature(route_surface))
        .unwrap_or_else(|| route_feature_from_name(&route.name));

    let surface = surfaces
        .iter()
        .filter(|surface| surface.platform == target.platform())
        .find(|surface| surface_matches(surface, route_surface, &route_feature, target));

    let audience = surface.and_then(|surface| {
        surface
            .audiences
            .iter()
            .find(|audience| audience.name == route_audience)
            .or_else(|| {
                if route.audience.is_none() {
                    surface.audiences.first()
                } else {
                    None
                }
            })
    });

    let view_guard = audience.and_then(|audience| {
        audience
            .views
            .iter()
            .find(|view| view.name == view_name)
            .and_then(|view| view.guard.as_ref())
    });
    let audience_guard = audience.and_then(|audience| audience.guard.as_ref());
    let experience_guard = surface.and_then(|surface| {
        let experience_name = surface
            .uses_experience
            .as_deref()
            .unwrap_or(surface.experience.as_str());
        experiences
            .iter()
            .find(|experience| experience.name == experience_name)
            .and_then(|experience| {
                experience
                    .views
                    .iter()
                    .find(|view| view.name == view_name)
                    .and_then(|view| view.guard.as_ref())
            })
    });

    let group = RouteGroupKey {
        feature: surface
            .map(|surface| surface.experience.clone())
            .unwrap_or(route_feature),
        platform: target.platform_label().to_owned(),
        audience: audience
            .map(|audience| audience.name.clone())
            .or_else(|| route.audience.clone())
            .unwrap_or_else(|| "default".to_owned()),
    };

    (group, view_guard, audience_guard, experience_guard)
}

fn surface_matches(
    surface: &PlatformSurface,
    route_surface: Option<&str>,
    route_feature: &str,
    target: RouteGuardTarget,
) -> bool {
    let Some(route_surface) = route_surface else {
        return surface.experience == route_feature;
    };
    let labels = [
        surface.experience.clone(),
        format!("{} {}", surface.experience, target.platform_label()),
        format!("{}.{}", surface.experience, target.platform_label()),
    ];
    labels.iter().any(|label| label == route_surface)
}

pub(super) fn route_target_feature(to: Option<&str>) -> Option<String> {
    let target = to?.split('(').next()?.trim();
    let (feature, _) = target.split_once(".view.")?;
    (!feature.is_empty()).then(|| feature.to_owned())
}

pub(super) fn route_target_view_name(to: Option<&str>) -> Option<String> {
    let target = to?.split('(').next()?.trim();
    let after_view = target.split(".view.").nth(1)?;
    let view = after_view.split('.').next()?.trim();
    (!view.is_empty()).then(|| view.to_owned())
}

pub(super) fn surface_feature(surface: Option<&str>) -> Option<String> {
    surface?
        .split(|ch: char| ch == ' ' || ch == '.')
        .next()
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
}

pub(super) fn route_feature_from_name(name: &str) -> String {
    name.split(['_', '-'])
        .next()
        .filter(|feature| !feature.is_empty())
        .unwrap_or("app")
        .to_owned()
}
