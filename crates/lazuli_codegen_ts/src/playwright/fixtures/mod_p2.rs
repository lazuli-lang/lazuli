fn roles_for_route(
    route: &AppRoute,
    app: Option<&AppManifest>,
    surfaces: &[PlatformSurface],
    experiences: &[Experience],
    policy_lookup: &BTreeMap<(String, String), Vec<PolicyAtom>>,
) -> BTreeSet<String> {
    let default_feature = route_target_feature(route.to.as_deref())
        .or_else(|| route.surface.as_deref().and_then(surface_feature))
        .unwrap_or_else(|| route_feature_from_name(&route.name));
    let view_name =
        route_target_view_name(route.to.as_deref()).unwrap_or_else(|| route.name.clone());
    let route_audience = route.audience.as_deref();
    let (view_guard, audience_guard) = surface_guards(
        route,
        &default_feature,
        &view_name,
        route_audience,
        surfaces,
    );
    let experience_guard = experiences
        .iter()
        .find(|experience| experience.name == default_feature)
        .and_then(|experience| experience.views.iter().find(|view| view.name == view_name))
        .and_then(|view| view.guard.as_ref());

    let guard_chain = [
        route.guard.as_ref(),
        view_guard,
        experience_guard,
        audience_guard,
    ];
    if let Some(policy_texts) = guard_chain
        .iter()
        .find_map(|guard| guard.map(|guard| guard.policy.as_slice()))
    {
        return roles_from_policy_refs(
            policy_texts.iter().map(String::as_str),
            policy_lookup,
            &default_feature,
        );
    }

    if let Some(default_policy) = app
        .and_then(|app| app.route_guard.as_ref())
        .and_then(|defaults| defaults.default_policy.as_deref())
    {
        let roles = roles_from_policy_refs(
            std::iter::once(default_policy),
            policy_lookup,
            &default_feature,
        );
        if !roles.is_empty() {
            return roles;
        }
    }

    route_audience
        .filter(|audience| FIXTURE_ROLES.contains(audience))
        .map(|audience| BTreeSet::from([audience.to_owned()]))
        .unwrap_or_default()
}

fn surface_guards<'a>(
    route: &AppRoute,
    default_feature: &str,
    view_name: &str,
    route_audience: Option<&str>,
    surfaces: &'a [PlatformSurface],
) -> (Option<&'a ViewGuard>, Option<&'a ViewGuard>) {
    let surface = surfaces
        .iter()
        .filter(|surface| surface.platform == Platform::Web)
        .find(|surface| {
            let surface_name = route.surface.as_deref();
            if let Some(surface_name) = surface_name {
                let mut parts = surface_name.split_whitespace();
                if parts.next() != Some(surface.experience.as_str()) {
                    return false;
                }
            }
            surface.experience == default_feature
                || surface.uses_experience.as_deref() == Some(default_feature)
        });
    let audience = surface.and_then(|surface| {
        route_audience
            .and_then(|name| {
                surface
                    .audiences
                    .iter()
                    .find(|audience| audience.name == name)
            })
            .or_else(|| {
                if route_audience.is_none() {
                    surface.audiences.first()
                } else {
                    None
                }
            })
    });
    let view_guard = audience
        .and_then(|audience| audience.views.iter().find(|view| view.name == view_name))
        .and_then(|view| view.guard.as_ref());
    let audience_guard = audience.and_then(|audience| audience.guard.as_ref());
    (view_guard, audience_guard)
}
