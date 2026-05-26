use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{
    AppManifest, AppRoute, ExperienceModule, ExperienceView, Feature, PlatformSurface,
    PlatformView, PolicyAtom, PolicyRef, Query, RouteGuardDefaults, SpanRef, ViewGuard,
};

use super::{RouteGuardDiagnostic, RouteGuardOrigin, RouteGuardSeverity};

mod actor_query;
mod redirects;

use actor_query::check_actor_query;
use redirects::{check_default_redirects, check_guard_redirects};

#[derive(Clone, Copy, PartialEq, Eq)]
enum GuardSource {
    Authored,
    Builtin,
}

struct ResolvedGuard {
    atoms: Vec<PolicyAtom>,
    source: GuardSource,
    span: Option<SpanRef>,
}

struct RouteCtx {
    surface: usize,
    audience: usize,
    platform_view: usize,
    experience: usize,
    experience_view: usize,
    feature: String,
    view: String,
}

pub fn check(
    module: &mut ExperienceModule,
    app_override: Option<&AppManifest>,
    features: &[Feature],
) -> Vec<RouteGuardDiagnostic> {
    let app_owned = app_override.cloned().or_else(|| module.app.clone());
    let app = app_owned.as_ref();
    let mut out = Vec::new();
    let mut seen_redirects = BTreeSet::new();
    let route_names: BTreeSet<String> = module.routes.iter().map(|r| r.name.clone()).collect();
    let route_paths: BTreeSet<String> = module
        .routes
        .iter()
        .filter_map(|r| r.path.clone())
        .collect();

    check_actor_query(app, module, features, &mut out);
    check_when_denied_route_policy_use(module, app, features, &mut out);
    if let Some(defaults) = app.and_then(|app| app.route_guard.as_ref()) {
        check_default_redirects(
            defaults,
            &route_names,
            &route_paths,
            &mut seen_redirects,
            &mut out,
        );
    }

    for index in 0..module.routes.len() {
        let Some(ctx) = route_ctx(module, &module.routes[index]) else {
            continue;
        };
        if runtime_audience_disagrees(&module.routes[index], &ctx) {
            out.push(RouteGuardDiagnostic {
                code: "ROUTE-GUARD-005",
                severity: RouteGuardSeverity::Info,
                origin: RouteGuardOrigin::Lzx,
                span: module.routes[index].span_ref,
                message: format!(
                    "route `{}` mounts view `{}` for audience `{}` under a path that reads as a different runtime audience.",
                    module.routes[index].name,
                    ctx.view,
                    module.routes[index].audience.as_deref().unwrap_or("default")
                ),
            });
        }

        let route_guard = module.routes[index].guard.as_ref();
        let audience = &module.surfaces[ctx.surface].audiences[ctx.audience];
        let platform_view = &audience.views[ctx.platform_view];
        let experience_view = &module.experiences[ctx.experience].views[ctx.experience_view];
        let resolved = resolve_guard(
            &ctx.feature,
            route_guard,
            platform_view.guard.as_ref(),
            experience_view.guard.as_ref(),
            audience.guard.as_ref(),
            app.and_then(|app| app.route_guard.as_ref()),
            features,
        );
        for guard in [
            route_guard,
            platform_view.guard.as_ref(),
            experience_view.guard.as_ref(),
            audience.guard.as_ref(),
        ] {
            if let Some(guard) = guard {
                check_guard_redirects(
                    guard,
                    &route_names,
                    &route_paths,
                    &mut seen_redirects,
                    &mut out,
                );
            }
        }

        for backend in backend_policies(&ctx.feature, experience_view, platform_view, features) {
            if backend.public {
                continue;
            }
            if resolved.source == GuardSource::Builtin {
                out.push(RouteGuardDiagnostic {
                    code: "ROUTE-GUARD-001",
                    severity: RouteGuardSeverity::Error,
                    origin: RouteGuardOrigin::Lzx,
                    span: experience_view.span_ref.or(platform_view.span_ref),
                    message: format!(
                        "view `{}` gates backend `{}` but resolves only to the built-in public route guard.",
                        ctx.view, backend.label
                    ),
                });
                continue;
            }
            let missing = missing_atoms(&resolved.atoms, &backend.atoms);
            if !missing.is_empty() {
                out.push(RouteGuardDiagnostic {
                    code: "ROUTE-GUARD-002",
                    severity: RouteGuardSeverity::Error,
                    origin: RouteGuardOrigin::Lzx,
                    span: resolved.span.or(experience_view.span_ref),
                    message: format!(
                        "view `{}` guard is laxer than backend `{}`; missing atoms: {}.",
                        ctx.view,
                        backend.label,
                        missing.join(", ")
                    ),
                });
            }
        }

        cache_resolved(module, &ctx, &resolved.atoms);
    }
    out
}

fn check_when_denied_route_policy_use(
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

fn route_ctx(module: &ExperienceModule, route: &AppRoute) -> Option<RouteCtx> {
    let (feature, view) = target_view(route.to.as_deref())?;
    let surface = module.surfaces.iter().position(|s| {
        surface_matches(s, route.surface.as_deref(), &feature)
            && route
                .audience
                .as_deref()
                .map(|name| s.audiences.iter().any(|a| a.name == name))
                .unwrap_or(!s.audiences.is_empty())
    })?;
    let s = &module.surfaces[surface];
    let audience = route
        .audience
        .as_deref()
        .and_then(|name| s.audiences.iter().position(|a| a.name == name))
        .or_else(|| (!s.audiences.is_empty()).then_some(0))?;
    let platform_view = s.audiences[audience]
        .views
        .iter()
        .position(|v| v.name == view)?;
    let exp_name = s.uses_experience.as_deref().unwrap_or(&s.experience);
    let experience = module.experiences.iter().position(|e| e.name == exp_name)?;
    let experience_view = module.experiences[experience]
        .views
        .iter()
        .position(|v| v.name == view)?;
    Some(RouteCtx {
        surface,
        audience,
        platform_view,
        experience,
        experience_view,
        feature,
        view,
    })
}

fn resolve_guard(
    feature: &str,
    route: Option<&ViewGuard>,
    platform: Option<&ViewGuard>,
    view: Option<&ViewGuard>,
    audience: Option<&ViewGuard>,
    app: Option<&RouteGuardDefaults>,
    features: &[Feature],
) -> ResolvedGuard {
    for guard in [route, platform, view, audience].into_iter().flatten() {
        // OR-semantics over the policy list: any matching policy admits.
        // Concatenating per-policy atom sets keeps the existing
        // OR-on-atoms runtime contract.
        let atoms = guard
            .policy
            .iter()
            .flat_map(|p| policy_text_atoms(p, feature, features))
            .collect();
        return ResolvedGuard {
            atoms,
            source: GuardSource::Authored,
            span: guard.span_ref,
        };
    }
    if let Some(policy) = app.and_then(|d| d.default_policy.as_deref()) {
        return ResolvedGuard {
            atoms: policy_text_atoms(policy, feature, features),
            source: GuardSource::Authored,
            span: app.and_then(|d| d.span_ref),
        };
    }
    ResolvedGuard {
        atoms: vec![atom("scope", "public")],
        source: GuardSource::Builtin,
        span: None,
    }
}

fn cache_resolved(module: &mut ExperienceModule, ctx: &RouteCtx, atoms: &[PolicyAtom]) {
    module.surfaces[ctx.surface].audiences[ctx.audience].views[ctx.platform_view]
        .resolved_guard_policy = Some(atoms.to_vec());
    module.experiences[ctx.experience].views[ctx.experience_view].resolved_guard_policy =
        Some(atoms.to_vec());
}

struct BackendPolicy {
    label: String,
    atoms: Vec<PolicyAtom>,
    public: bool,
}

fn backend_policies(
    default_feature: &str,
    view: &ExperienceView,
    platform: &PlatformView,
    features: &[Feature],
) -> Vec<BackendPolicy> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some((feature, name)) = view
        .source
        .as_deref()
        .and_then(|s| parse_query_ref(s, default_feature))
    {
        push_backend_query(&feature, &name, features, &mut seen, &mut out);
    }
    for text in view
        .submit
        .iter()
        .chain(view.actions.iter().map(|a| &a.target))
        .chain(platform.submit.iter())
        .chain(platform.actions.iter())
    {
        if let Some((feature, name)) = parse_command_ref(text, default_feature) {
            push_backend_command(&feature, &name, features, &mut seen, &mut out);
        }
    }
    out
}

fn push_backend_command(
    feature: &str,
    name: &str,
    features: &[Feature],
    seen: &mut BTreeSet<String>,
    out: &mut Vec<BackendPolicy>,
) {
    let key = format!("command:{feature}.{name}");
    if !seen.insert(key) {
        return;
    }
    let Some(f) = features.iter().find(|f| f.name == feature) else {
        return;
    };
    let Some(cmd) = f.commands.iter().find(|c| c.name == name) else {
        return;
    };
    let atoms = policy_ref_atoms(
        effective_policy(&cmd.policy, &f.defaults.policy),
        f,
        features,
    );
    out.push(BackendPolicy {
        label: format!("{feature}.command.{name}"),
        public: policy_is_public(&atoms, effective_policy(&cmd.policy, &f.defaults.policy)),
        atoms,
    });
}

fn push_backend_query(
    feature: &str,
    name: &str,
    features: &[Feature],
    seen: &mut BTreeSet<String>,
    out: &mut Vec<BackendPolicy>,
) {
    let key = format!("query:{feature}.{name}");
    if !seen.insert(key) {
        return;
    }
    let Some(f) = features.iter().find(|f| f.name == feature) else {
        return;
    };
    let Some(query) = f.queries.iter().find(|q| q.name() == name) else {
        return;
    };
    let policy = query_policy(query);
    let atoms = policy_ref_atoms(effective_policy(policy, &f.defaults.policy), f, features);
    out.push(BackendPolicy {
        label: format!("{feature}.query.{name}"),
        public: policy_is_public(&atoms, effective_policy(policy, &f.defaults.policy)),
        atoms,
    });
}

fn query_policy(query: &Query) -> &PolicyRef {
    match query {
        Query::List(q) => &q.policy,
        Query::Lookup(q) => &q.policy,
        Query::Sql(q) => &q.policy,
    }
}

fn effective_policy<'a>(
    policy: &'a PolicyRef,
    default: &'a Option<PolicyRef>,
) -> Option<&'a PolicyRef> {
    if policy.is_none() {
        default.as_ref()
    } else {
        Some(policy)
    }
}

fn policy_ref_atoms(
    policy: Option<&PolicyRef>,
    feature: &Feature,
    features: &[Feature],
) -> Vec<PolicyAtom> {
    match policy {
        None | Some(PolicyRef::None) => Vec::new(),
        Some(PolicyRef::Atom(a)) => {
            if let Some(local) = a.strip_prefix("policy.") {
                category_atoms(&feature.name, local, features)
            } else {
                parse_atom(a).into_iter().collect()
            }
        }
        Some(PolicyRef::Local(n)) => category_atoms(&feature.name, n, features),
        Some(PolicyRef::External { feature, name }) => category_atoms(feature, name, features),
        Some(PolicyRef::Unresolved(s)) => policy_text_atoms(s, &feature.name, features),
    }
}

fn policy_text_atoms(text: &str, default_feature: &str, features: &[Feature]) -> Vec<PolicyAtom> {
    let raw = text.trim().trim_start_matches('@');
    if let Some(tail) = raw.strip_prefix("policy.") {
        let mut parts = tail.splitn(2, '.');
        let first = parts.next().unwrap_or_default();
        if let Some(second) = parts.next() {
            if features.iter().any(|f| f.name == first) {
                return category_atoms(first, second, features);
            }
        }
        return category_atoms(default_feature, tail, features);
    }
    parse_atom(raw).into_iter().collect()
}

fn category_atoms(feature: &str, name: &str, features: &[Feature]) -> Vec<PolicyAtom> {
    let mut atoms: Vec<_> = features
        .iter()
        .find(|f| f.name == feature)
        .into_iter()
        .flat_map(|f| &f.policies.categories)
        .find(|c| c.name == name)
        .into_iter()
        .flat_map(|c| c.atoms.iter())
        .filter_map(|a| parse_atom(a))
        .collect();
    sort_atoms(&mut atoms);
    atoms
}

fn parse_atom(text: &str) -> Option<PolicyAtom> {
    let raw = text.trim().trim_start_matches('@');
    let (namespace, name) = raw.split_once('.')?;
    (namespace != "policy" && !namespace.is_empty() && !name.is_empty())
        .then(|| atom(namespace, name))
}

fn atom(namespace: &str, name: &str) -> PolicyAtom {
    PolicyAtom {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
        args: None,
    }
}

fn policy_is_public(atoms: &[PolicyAtom], policy: Option<&PolicyRef>) -> bool {
    policy.is_none()
        || atoms
            .iter()
            .any(|a| a.namespace == "scope" && a.name == "public")
}

fn missing_atoms(guard: &[PolicyAtom], backend: &[PolicyAtom]) -> Vec<String> {
    backend
        .iter()
        .filter(|atom| !guard.iter().any(|g| g == *atom))
        .map(|a| format!("@{}.{}", a.namespace, a.name))
        .collect()
}

fn sort_atoms(atoms: &mut Vec<PolicyAtom>) {
    atoms.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));
    atoms.dedup();
}

fn target_view(to: Option<&str>) -> Option<(String, String)> {
    let head = to?.split('(').next().unwrap_or_default();
    let parts: Vec<_> = head.split('.').collect();
    (parts.len() >= 3 && parts[1] == "view").then(|| (parts[0].to_owned(), parts[2].to_owned()))
}

fn surface_feature(surface: &str) -> Option<String> {
    surface
        .split(|ch: char| ch == ' ' || ch == '.')
        .next()
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
}

fn route_feature_from_name(name: &str) -> String {
    name.split('_')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or("app")
        .to_owned()
}

pub(super) fn parse_query_ref(text: &str, default_feature: &str) -> Option<(String, String)> {
    let head = text.split('(').next().unwrap_or(text).trim();
    let parts: Vec<_> = head.split('.').collect();
    match parts.as_slice() {
        [feature, "query", name] => Some(((*feature).to_owned(), (*name).to_owned())),
        [feature, "query", _kind, name] => Some(((*feature).to_owned(), (*name).to_owned())),
        ["query", name] => Some((default_feature.to_owned(), (*name).to_owned())),
        _ => None,
    }
}

fn parse_command_ref(text: &str, default_feature: &str) -> Option<(String, String)> {
    let head = text.split('(').next().unwrap_or(text).trim();
    let parts: Vec<_> = head.split('.').collect();
    match parts.as_slice() {
        [feature, "command", name] => Some(((*feature).to_owned(), (*name).to_owned())),
        [name] => Some((default_feature.to_owned(), (*name).to_owned())),
        _ => None,
    }
}

fn surface_matches(surface: &PlatformSurface, label: Option<&str>, feature: &str) -> bool {
    let Some(label) = label else {
        return surface.experience == feature;
    };
    label == surface.experience
        || label == format!("{} web", surface.experience)
        || label == format!("{} mobile", surface.experience)
        || label == format!("{}.web", surface.experience)
        || label == format!("{}.mobile", surface.experience)
}

fn runtime_audience_disagrees(route: &AppRoute, _ctx: &RouteCtx) -> bool {
    let Some(audience) = route.audience.as_deref() else {
        return false;
    };
    let Some(path) = route.path.as_deref() else {
        return false;
    };
    let first = path.trim_start_matches('/').split('/').next().unwrap_or("");
    matches!(
        first,
        "admin" | "account" | "host" | "public" | "sales" | "buyer" | "seller"
    ) && first != audience
}

#[cfg(test)]
mod tests {
    use lazuli_ir::{AppRoute, ExperienceModule, Feature, ViewGuard};
    use serde_json::json;

    use super::check;

    fn feature_with_route_denial_policy() -> Feature {
        serde_json::from_value(json!({
            "name": "host",
            "purpose": null,
            "defaults": {},
            "uses": [],
            "enums": [],
            "resources": [],
            "events": [],
            "rules": [],
            "policies": {
                "categories": [
                    {
                        "name": "host_only",
                        "atoms": ["@scope.authenticated", "@role.host"],
                        "when_denied_route": {
                            "unauthenticated": { "kind": "view", "value": "sign_in" }
                        }
                    }
                ],
                "fields": []
            },
            "commands": [],
            "queries": [
                {
                    "kind": "List",
                    "name": "list_hosts",
                    "policy": { "kind": "Local", "value": "host_only" }
                }
            ],
            "workflows": [],
            "jobs": [],
            "webhooks": [],
            "surfaces": [],
            "extensions": [],
            "escape_routes": []
        }))
        .expect("feature json")
    }

    fn empty_module() -> ExperienceModule {
        ExperienceModule {
            app: None,
            routes: Vec::new(),
            experiences: Vec::new(),
            surfaces: Vec::new(),
        }
    }

    #[test]
    fn route_policy_001_flags_route_denial_policy_on_nonroute_only_policy() {
        let mut module = empty_module();
        let features = vec![feature_with_route_denial_policy()];

        let diagnostics = check(&mut module, None, &features);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "ROUTE-POLICY-001"),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn route_policy_001_allows_policy_also_used_by_route_guard() {
        let mut module = empty_module();
        module.routes.push(AppRoute {
            name: "host_home".to_string(),
            path: Some("/host".to_string()),
            routes: Vec::new(),
            route_params: Vec::new(),
            to: Some("host.view.host_home".to_string()),
            surface: Some("host web".to_string()),
            audience: Some("host".to_string()),
            lazy: None,
            prerender: None,
            guard: Some(ViewGuard {
                policy: vec!["@policy.host_only".to_string()],
                on_unauthenticated: None,
                on_unauthorized: None,
                requires_lifecycle: None,
                on_lifecycle_pending: None,
                forbid_when: Vec::new(),
                span_ref: None,
            }),
            loaders: Vec::new(),
            pending_view: None,
            error_view: None,
            parent: None,
            span_ref: None,
        });
        let features = vec![feature_with_route_denial_policy()];

        let diagnostics = check(&mut module, None, &features);

        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "ROUTE-POLICY-001"),
            "{diagnostics:#?}"
        );
    }
}
