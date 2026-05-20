use std::collections::BTreeSet;

use lazuli_ir::{
    AppManifest, AppRoute, ExperienceModule, ExperienceView, Feature, PlatformSurface,
    PlatformView, PolicyAtom, PolicyRef, Query, RouteGuardDefaults, SpanRef, TypeRef, ViewGuard,
};

use super::{RouteGuardDiagnostic, RouteGuardOrigin, RouteGuardSeverity};

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

fn check_actor_query(
    app: Option<&AppManifest>,
    module: &ExperienceModule,
    features: &[Feature],
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    let guarded = app.and_then(|a| a.route_guard.as_ref()).is_some()
        || module.routes.iter().any(|r| r.guard.is_some())
        || module.surfaces.iter().any(|s| {
            s.audiences
                .iter()
                .any(|a| a.guard.is_some() || a.views.iter().any(|v| v.guard.is_some()))
        })
        || module
            .experiences
            .iter()
            .any(|e| e.views.iter().any(|v| v.guard.is_some()));
    let Some(app) = app.filter(|_| guarded) else {
        return;
    };
    let Some(actor_query) = app.actor_query.as_deref() else {
        return push_004(
            "app declares route guards but no `actor_query`.",
            app.span_ref,
            out,
        );
    };
    let Some((feature, name)) = parse_query_ref(actor_query, "") else {
        return push_004(
            "`actor_query` must be `<feature>.query.<name>`.",
            app.span_ref,
            out,
        );
    };
    match find_query(features, &feature, &name) {
        Some(Query::Sql(q)) if !actor_type(&q.returns) => push_004(
            "`actor_query` should return `LazuliActor | null` compatible data.",
            q.span_ref.or(app.span_ref),
            out,
        ),
        Some(_) => {}
        None => push_004(
            "`actor_query` references a query that does not exist.",
            app.span_ref,
            out,
        ),
    }
}

fn push_004(message: &str, span: Option<SpanRef>, out: &mut Vec<RouteGuardDiagnostic>) {
    out.push(RouteGuardDiagnostic {
        code: "ROUTE-GUARD-004",
        severity: RouteGuardSeverity::Warning,
        origin: RouteGuardOrigin::App,
        span,
        message: message.to_owned(),
    });
}

fn route_ctx(module: &ExperienceModule, route: &AppRoute) -> Option<RouteCtx> {
    let (feature, view) = target_view(route.to.as_deref())?;
    let surface = module
        .surfaces
        .iter()
        .position(|s| {
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

fn find_query<'a>(features: &'a [Feature], feature: &str, name: &str) -> Option<&'a Query> {
    features
        .iter()
        .find(|f| f.name == feature)?
        .queries
        .iter()
        .find(|q| q.name() == name)
}

fn actor_type(ty: &TypeRef) -> bool {
    match ty {
        TypeRef::UserDefined(q) => matches!(q.name.as_str(), "LazuliActor" | "Actor"),
        TypeRef::Unresolved(s) => s.contains("LazuliActor") || s.contains("Actor"),
        _ => false,
    }
}

fn check_default_redirects(
    defaults: &RouteGuardDefaults,
    route_names: &BTreeSet<String>,
    route_paths: &BTreeSet<String>,
    seen: &mut BTreeSet<(String, &'static str)>,
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    for (slot, target) in [
        ("on_unauthenticated", defaults.on_unauthenticated.as_deref()),
        ("on_unauthorized", defaults.on_unauthorized.as_deref()),
    ] {
        if let Some(target) = target {
            push_redirect_003(
                target,
                slot,
                defaults.span_ref,
                RouteGuardOrigin::App,
                route_names,
                route_paths,
                seen,
                out,
            );
        }
    }
}

fn check_guard_redirects(
    guard: &ViewGuard,
    route_names: &BTreeSet<String>,
    route_paths: &BTreeSet<String>,
    seen: &mut BTreeSet<(String, &'static str)>,
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    for (slot, target) in [
        ("on_unauthenticated", guard.on_unauthenticated.as_deref()),
        ("on_unauthorized", guard.on_unauthorized.as_deref()),
    ] {
        if let Some(target) = target {
            push_redirect_003(
                target,
                slot,
                guard.span_ref,
                RouteGuardOrigin::Lzx,
                route_names,
                route_paths,
                seen,
                out,
            );
        }
    }
}

fn push_redirect_003(
    target: &str,
    slot: &'static str,
    span: Option<SpanRef>,
    origin: RouteGuardOrigin,
    route_names: &BTreeSet<String>,
    route_paths: &BTreeSet<String>,
    seen: &mut BTreeSet<(String, &'static str)>,
    out: &mut Vec<RouteGuardDiagnostic>,
) {
    if route_paths.contains(target) || (!target.starts_with('/') && route_names.contains(target)) {
        return;
    }
    if !seen.insert((target.to_owned(), slot)) {
        return;
    }
    out.push(RouteGuardDiagnostic {
        code: "ROUTE-GUARD-003",
        severity: RouteGuardSeverity::Error,
        origin,
        span,
        message: format!(
            "route guard `{slot}` redirect target `{target}` does not resolve to a declared route."
        ),
    });
}

fn target_view(to: Option<&str>) -> Option<(String, String)> {
    let head = to?.split('(').next().unwrap_or_default();
    let parts: Vec<_> = head.split('.').collect();
    (parts.len() >= 3 && parts[1] == "view").then(|| (parts[0].to_owned(), parts[2].to_owned()))
}

fn parse_query_ref(text: &str, default_feature: &str) -> Option<(String, String)> {
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
