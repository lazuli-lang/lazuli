use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use lazuli_ir::{
    AppManifest, AppRoute, Experience, Feature, Platform, PlatformSurface, RequiresLifecycle,
    ViewGuard,
};

use crate::GeneratedFile;
use crate::lzx::lzx_router_adapter::{RouterTarget, translate_route_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoutesTarget {
    Web,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteSpec {
    name: String,
    path: String,
    audience: String,
    component_key: String,
    route_const: String,
    /// W3 Tier 1 — when the .lzx route block declared `policy @policy.X`
    /// (plus optional `on_unauthenticated`/`on_unauthorized`), codegen
    /// emits a `beforeLoad` that calls the runtime's
    /// `tanstackBeforeLoadGuard`. None ⇒ route's `beforeLoad` falls
    /// through to `options.guards?.<key>` (consumer-supplied closure;
    /// the Wave 2 escape hatch stays available for app-bespoke logic).
    guard_emit: Option<GuardEmit>,
    /// W3 Tier 2 + W4 — when the .lzx route block declared
    /// `requires_lifecycle <Resource> = <state>` and the resource
    /// authored a `lifecycle_routes` table, codegen chains the policy
    /// gate with a lifecycle fetch + dispatch via the per-resource
    /// helper.
    lifecycle_emit: Option<LifecycleEmit>,
    /// router-w5 — declarative loaders. Each entry prefetches a
    /// feature-level zero-arg query via TanStack Query's
    /// `ensureQueryData`. Multiple loaders run in parallel.
    loaders: Vec<LoaderEmit>,
    /// router-w6 — pending_view component-key. None when the route
    /// doesn't declare one. Consumers register the component in
    /// ROUTE_COMPONENTS under the same key (joins ClientComponentKey).
    pending_component_key: Option<String>,
    /// router-w6 — error_view component-key.
    error_component_key: Option<String>,
    /// router-w8 — parent route name (used to compute the parent's
    /// route const). None means the route mounts under rootRoute.
    parent_route_const: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LoaderEmit {
    feature: String,
    /// camelCase TS export of the query (e.g. `lookupMyHost`).
    query_export: String,
}

/// Resolved per-route guard payload for codegen. Atoms are decomposed
/// from `@<ns>.<name>` strings at IR-resolution time so the emitted
/// TS doesn't need a runtime lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GuardEmit {
    policy_name: String,
    policy_atoms: Vec<(String, String)>,
    on_unauthenticated: Option<String>,
    on_unauthorized: Option<String>,
    /// W3 Tier 3 — forbid_when checks that run BEFORE the main
    /// policy gate (signed-in actors who already satisfy the listed
    /// atom redirect away rather than seeing the route).
    forbid_when: Vec<ForbidEmit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ForbidEmit {
    atom_namespace: String,
    atom_name: String,
    dispatch_to: String,
}

/// W3 Tier 2 + W4 — lifecycle dispatch payload. Carries the
/// `lookup_my_<resource>` query reference + the per-resource
/// lifecycle-route helper export so the emitted beforeLoad can fetch
/// + redirect deterministically.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleEmit {
    /// snake_case feature name owning the resource (used as the
    /// import path: `../<feature>/<feature>.gen.js`).
    feature: String,
    /// camelCase export name of the lookup_my_<resource> query.
    lookup_export: String,
    /// camelCase export name of the lifecycle_route helper.
    helper_export: String,
    /// Required lifecycle state. The route renders only when the
    /// actor's row reports this state; any other state triggers a
    /// redirect via the helper.
    required_state: String,
}

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
            guard_emit: route.guard.as_ref().and_then(|g| resolve_guard_emit(g, features)),
            lifecycle_emit: route
                .guard
                .as_ref()
                .and_then(|g| g.requires_lifecycle.as_ref())
                .and_then(|rl| resolve_lifecycle_emit(rl, features)),
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
    if let Some(surface) = route.surface.as_deref() {
        if let Some(platform) = surface_platform_label(surface) {
            return platform == target.platform_label();
        }
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

fn emit_routes_file(specs: &[RouteSpec]) -> String {
    let mut s = String::new();
    s.push_str("// Code generated by lazuli; DO NOT EDIT.\n");
    // Imports flow through @lazuli/runtime so the dist file resolves
    // regardless of where it sits in the host filesystem. Pilots must
    // additionally path-map `@tanstack/react-router` + `@tanstack/react-query`
    // to their own node_modules so the lazuli-source-resolved imports and
    // the consumer's imports unify on a single physical module (otherwise
    // pnpm's per-package isolation produces different `#private` brands
    // and the router instance is not assignable to RouterProvider).
    let any_ir_guard = specs.iter().any(|s| s.guard_emit.is_some());
    let any_lifecycle = specs.iter().any(|s| s.lifecycle_emit.is_some());
    let any_forbid_when = specs
        .iter()
        .any(|s| s.guard_emit.as_ref().is_some_and(|g| !g.forbid_when.is_empty()));
    s.push_str("import { createElement, type FunctionComponent, type ReactElement } from \"@lazuli/runtime/react\";\n");
    s.push_str(
        "import { Link, Outlet, createRootRouteWithContext, createRoute, createRouter, redirect, type LinkProps, type NotFoundRouteComponent, type QueryClient } from \"@lazuli/runtime/react/tanstack\";\n",
    );
    if any_ir_guard {
        // tanstackBeforeLoadGuard is the runtime helper that decomposes
        // policy → verdict and throws the configured redirect.
        s.push_str("import { tanstackBeforeLoadGuard } from \"@lazuli/runtime/react/tanstack\";\n");
    }
    if any_lifecycle {
        // queryKeyFor is the canonical TanStack-Query cache key builder;
        // codegen-emitted lifecycle gates call it for the fetchQuery key.
        s.push_str("import { queryKeyFor } from \"@lazuli/runtime/react\";\n");
        s.push_str("import { isLazuliError } from \"@lazuli/runtime\";\n");
    }
    if any_forbid_when {
        // evaluatePolicy is the closed-catalog policy verdict helper;
        // forbid_when arms call it with a single-atom policy to detect
        // whether the actor satisfies the listed atom.
        s.push_str("import { evaluatePolicy } from \"@lazuli/runtime/react\";\n");
    }
    s.push_str("import type { LazuliClient } from \"@lazuli/runtime\";\n");
    let any_loaders = specs.iter().any(|s| !s.loaders.is_empty());
    if any_loaders && !any_lifecycle {
        // queryKeyFor already imported when any_lifecycle is true.
        s.push_str("import { queryKeyFor } from \"@lazuli/runtime/react\";\n");
    }
    // Per-feature SDK imports for every lifecycle gate's
    // lookup_my_<resource> query + helper, plus every loader query.
    let mut feature_imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for spec in specs {
        if let Some(lc) = &spec.lifecycle_emit {
            let bucket = feature_imports.entry(lc.feature.clone()).or_default();
            bucket.insert(lc.lookup_export.clone());
            bucket.insert(lc.helper_export.clone());
        }
        for loader in &spec.loaders {
            feature_imports
                .entry(loader.feature.clone())
                .or_default()
                .insert(loader.query_export.clone());
        }
    }
    for (feature, names) in &feature_imports {
        let list = names.iter().cloned().collect::<Vec<_>>().join(", ");
        s.push_str(&format!(
            "import {{ {} }} from \"../{}/{}.gen.js\";\n",
            list, feature, feature
        ));
    }
    s.push('\n');

    s.push_str("export const routeSpecs = [\n");
    for spec in specs {
        writeln!(s, "  {{").ok();
        writeln!(s, "    name: {},", ts_string(&spec.name)).ok();
        writeln!(s, "    path: {},", ts_string(&spec.path)).ok();
        writeln!(s, "    audience: {},", ts_string(&spec.audience)).ok();
        writeln!(s, "    component: {},", ts_string(&spec.component_key)).ok();
        writeln!(s, "  }},").ok();
    }
    s.push_str("] as const;\n\n");

    if specs.is_empty() {
        s.push_str("export type ClientComponentKey = never;\n");
    } else {
        // ClientComponentKey unions every component-key the consumer
        // must register: the main route component, plus any
        // pending_view / error_view (router-w6) keys declared on a
        // route. Dedup'd alphabetically for deterministic output.
        let mut keys: BTreeSet<String> = BTreeSet::new();
        for spec in specs {
            keys.insert(spec.component_key.clone());
            if let Some(k) = &spec.pending_component_key {
                keys.insert(k.clone());
            }
            if let Some(k) = &spec.error_component_key {
                keys.insert(k.clone());
            }
        }
        let union = keys
            .iter()
            .map(|k| ts_string(k))
            .collect::<Vec<_>>()
            .join(" | ");
        writeln!(s, "export type ClientComponentKey = {};", union).ok();
    }
    s.push_str("export type GeneratedRouterContext = { client: LazuliClient; queryClient: QueryClient };\n");
    s.push_str(
        "export type GeneratedRouterComponents = Record<ClientComponentKey, FunctionComponent<any>>;\n",
    );
    s.push_str("export type GeneratedRouterGuards = Partial<Record<ClientComponentKey, (params: { context: GeneratedRouterContext }) => unknown | Promise<unknown>>>;\n\n");
    s.push_str("export function createGeneratedRouter(options: {\n");
    s.push_str("  client: LazuliClient;\n");
    s.push_str("  queryClient: QueryClient;\n");
    s.push_str("  components: GeneratedRouterComponents;\n");
    s.push_str("  guards?: GeneratedRouterGuards;\n");
    s.push_str("  notFoundComponent?: NotFoundRouteComponent;\n");
    s.push_str("  defaultPreload?: \"intent\" | \"viewport\" | \"render\" | false;\n");
    s.push_str("}) {\n");
    // Root route renders Outlet so child route components actually mount.
    // Emitted as createElement(Outlet) (not JSX) so the dist file doesn't
    // need a resolvable `react/jsx-runtime` from its filesystem walk.
    // notFoundComponent is set on the rootRoute (catches __root__ errors)
    // AND wired as router-level defaultNotFoundComponent (catches mismatches
    // bubbled up from child routes) — both are required because TanStack's
    // fuzzy notFoundMode doesn't always cascade router-level default to
    // root-level errors.
    s.push_str("  const rootRoute = createRootRouteWithContext<GeneratedRouterContext>()({\n");
    s.push_str("    component: () => createElement(Outlet),\n");
    s.push_str("    notFoundComponent: options.notFoundComponent,\n");
    s.push_str("  });\n");
    for spec in specs {
        writeln!(s, "  const {} = createRoute({{", spec.route_const).ok();
        // router-w8 — nested routes pin getParentRoute to a sibling
        // const; default is the root route. Specs are emitted in
        // route_const sort order; if a child appears before its
        // parent the const reference works (Rust closures are
        // resolved at call time, JS arrow's lazy via the closure).
        if let Some(parent) = &spec.parent_route_const {
            writeln!(s, "    getParentRoute: () => {},", parent).ok();
        } else {
            s.push_str("    getParentRoute: () => rootRoute,\n");
        }
        writeln!(s, "    path: {},", ts_string(&spec.path)).ok();
        writeln!(
            s,
            "    component: options.components.{},",
            spec.component_key
        )
        .ok();
        // router-w6 — pending_view / error_view slots, when declared.
        if let Some(key) = &spec.pending_component_key {
            writeln!(s, "    pendingComponent: options.components.{},", key).ok();
        }
        if let Some(key) = &spec.error_component_key {
            writeln!(s, "    errorComponent: options.components.{},", key).ok();
        }
        emit_before_load(&mut s, spec);
        emit_loader(&mut s, spec);
        s.push_str("  });\n");
    }
    // router-w8 — compose the route tree with nested addChildren for
    // routes that declared `parent`. Top-level routes (no parent)
    // attach directly to rootRoute. Each parent that has children
    // gets `.addChildren([...] as const)` recursively.
    let tree_children = build_tree_expr(specs);
    writeln!(
        s,
        "  const routeTree = rootRoute.addChildren([{}] as const);",
        tree_children
    )
    .ok();
    s.push_str("  return createRouter({\n");
    s.push_str("    routeTree,\n");
    s.push_str("    context: { client: options.client, queryClient: options.queryClient },\n");
    s.push_str("    defaultPreload: options.defaultPreload,\n");
    s.push_str("    defaultNotFoundComponent: options.notFoundComponent,\n");
    s.push_str("  });\n");
    s.push_str("}\n\n");
    s.push_str("export const AppLink = Link as <T extends LinkProps[\"to\"]>(\n");
    s.push_str("  props: LinkProps & { to: T },\n");
    s.push_str(") => ReactElement;\n\n");
    // router-w7 — typed nav helpers. One function per route, keyed by
    // the camelCase route name (same as ClientComponentKey for the
    // main component). Returns a `{ to, params? }` literal that
    // spreads into `<Link {...nav.foo()}>`. Routes with path params
    // (`$id`-style segments) accept a typed `params` argument; param-
    // less routes accept no argument. Rename in .lzx → call sites
    // fail typecheck → safe refactoring.
    emit_nav_helpers(&mut s, specs);
    s
}

fn emit_nav_helpers(out: &mut String, specs: &[RouteSpec]) {
    if specs.is_empty() {
        return;
    }
    out.push_str("export const nav = {\n");
    for spec in specs {
        let params = path_params(&spec.path);
        if params.is_empty() {
            out.push_str(&format!(
                "  {}: () => ({{ to: {} as const }}),\n",
                spec.component_key,
                ts_string(&spec.path),
            ));
        } else {
            let arg_fields = params
                .iter()
                .map(|p| format!("{p}: string"))
                .collect::<Vec<_>>()
                .join("; ");
            let params_field = params
                .iter()
                .map(|p| format!("{p}: params.{p}"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "  {}: (params: {{ {} }}) => ({{ to: {} as const, params: {{ {} }} }}),\n",
                spec.component_key,
                arg_fields,
                ts_string(&spec.path),
                params_field,
            ));
        }
    }
    out.push_str("};\n");
}

/// Extract the path-param names from a TanStack-formatted path
/// (`/host/services/$id` → `["id"]`). Order preserved.
fn path_params(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for segment in path.split('/') {
        if let Some(name) = segment.strip_prefix('$') {
            if !name.is_empty() {
                out.push(name.to_owned());
            }
        }
    }
    out
}

/// W3 Tier 1/2 + W4 — decide whether to emit an inline `beforeLoad`
/// (driven by the IR-resolved guard + optional lifecycle gate) or
/// fall through to the Wave-2 escape hatch (`options.guards?.<key>`).
///
/// Combined emission ordering:
///   1. Policy gate (tanstackBeforeLoadGuard) — auth + role + redirect.
///   2. Lifecycle gate — fetch lookup_my_<resource>, redirect via
///      `<resource>LifecycleRoute(state)` when state mismatches.
///
/// Both shapes can coexist: if the route both declared `policy` in
/// .lzx AND the app passes a guard by key, the IR guard runs first;
/// the consumer's guard is never reached because TanStack's
/// beforeLoad slot is single-valued.
fn emit_before_load(out: &mut String, spec: &RouteSpec) {
    if spec.guard_emit.is_none() && spec.lifecycle_emit.is_none() {
        writeln!(
            out,
            "    beforeLoad: options.guards?.{},",
            spec.component_key
        )
        .ok();
        return;
    }
    out.push_str("    beforeLoad: async (params) => {\n");
    if let Some(guard) = &spec.guard_emit {
        // W3 Tier 3 — forbid_when atoms run BEFORE the policy gate.
        // The actor is fetched once via resolveActor, then each atom
        // is checked against the closed-catalog evaluatePolicy helper.
        // A signed-out actor short-circuits to the policy gate
        // (signed-out users can't satisfy a role/scope atom).
        if !guard.forbid_when.is_empty() {
            out.push_str("      const __forbidActor = await options.client.resolveActor();\n");
            out.push_str("      if (__forbidActor) {\n");
            for fw in &guard.forbid_when {
                out.push_str(&format!(
                    "        if (evaluatePolicy(__forbidActor, {{ name: \"@{ns}.{name}\", atoms: [{{ namespace: {ns:?}, name: {name:?} }}] }}) === \"authorized\") {{\n",
                    ns = fw.atom_namespace,
                    name = fw.atom_name,
                ));
                out.push_str(&format!(
                    "          throw redirect({{ to: {:?} }});\n",
                    fw.dispatch_to
                ));
                out.push_str("        }\n");
            }
            out.push_str("      }\n");
        }
        let atoms_literal = guard
            .policy_atoms
            .iter()
            .map(|(ns, name)| format!("{{ namespace: {:?}, name: {:?} }}", ns, name))
            .collect::<Vec<_>>()
            .join(", ");
        let on_unauth = match guard.on_unauthenticated.as_deref() {
            Some(p) => format!("{:?}", p),
            None => "undefined".to_owned(),
        };
        let on_unauth_role = match guard.on_unauthorized.as_deref() {
            Some(p) => format!("{:?}", p),
            None => "undefined".to_owned(),
        };
        out.push_str("      await tanstackBeforeLoadGuard(options.client, {\n");
        out.push_str(&format!(
            "        policy: {{ name: {:?}, atoms: [{}] }},\n",
            guard.policy_name, atoms_literal
        ));
        out.push_str(&format!("        onUnauthenticated: {},\n", on_unauth));
        out.push_str(&format!("        onUnauthorized: {},\n", on_unauth_role));
        out.push_str("      })();\n");
    }
    if let Some(lc) = &spec.lifecycle_emit {
        // Fetch lookup_my_<resource> via the route context's
        // queryClient. The fetchQuery promise resolves (or rejects) the
        // route's beforeLoad atomically; TanStack waits for the redirect
        // throw before painting the route.
        out.push_str("      let __row;\n");
        out.push_str("      try {\n");
        out.push_str("        __row = await params.context.queryClient.fetchQuery({\n");
        out.push_str(&format!(
            "          queryKey: queryKeyFor({}, {{}}),\n",
            lc.lookup_export
        ));
        out.push_str(&format!(
            "          queryFn: () => params.context.client.runQuery({}, {{}}),\n",
            lc.lookup_export
        ));
        out.push_str("        });\n");
        out.push_str("      } catch (err) {\n");
        out.push_str("        if (isLazuliError(err) && err.status === 404) {\n");
        out.push_str(&format!(
            "          throw redirect({{ to: {}(null) }});\n",
            lc.helper_export
        ));
        out.push_str("        }\n");
        out.push_str("        throw err;\n");
        out.push_str("      }\n");
        out.push_str("      const __state = (__row as { lifecycleState?: string }).lifecycleState ?? null;\n");
        out.push_str(&format!(
            "      if (__state !== {:?}) {{\n",
            lc.required_state
        ));
        out.push_str(&format!(
            "        throw redirect({{ to: {}(__state) }});\n",
            lc.helper_export
        ));
        out.push_str("      }\n");
    }
    out.push_str("    },\n");
}

fn resolve_guard_emit(guard: &ViewGuard, features: &[Feature]) -> Option<GuardEmit> {
    let has_policy = !guard.policy.is_empty();
    let has_forbid = !guard.forbid_when.is_empty();
    if !has_policy && !has_forbid {
        return None;
    }
    let (policy_name, policy_atoms) = if let Some(policy_ref) = guard.policy.first() {
        let policy_ref = policy_ref.trim();
        (
            policy_ref.to_owned(),
            resolve_policy_atoms(policy_ref, features),
        )
    } else {
        // forbid_when alone with no main policy: emit a trivial
        // always-authorized policy so the verdict logic falls through.
        (
            "@scope.authenticated".to_owned(),
            vec![("scope".to_owned(), "authenticated".to_owned())],
        )
    };
    Some(GuardEmit {
        policy_name,
        policy_atoms,
        on_unauthenticated: guard.on_unauthenticated.clone(),
        on_unauthorized: guard.on_unauthorized.clone(),
        forbid_when: guard
            .forbid_when
            .iter()
            .map(|fw| ForbidEmit {
                atom_namespace: fw.atom.namespace.clone(),
                atom_name: fw.atom.name.clone(),
                dispatch_to: fw.dispatch_to.clone(),
            })
            .collect(),
    })
}

/// router-w8 — build the comma-joined `routeTree` children list,
/// recursively wrapping each parent in
/// `<parent>.addChildren([<children>] as const)`. Children of a
/// parent are sorted by route_const to keep emission stable.
fn build_tree_expr(specs: &[RouteSpec]) -> String {
    let mut children_of: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut top_level: Vec<String> = Vec::new();
    for spec in specs {
        if let Some(parent) = &spec.parent_route_const {
            children_of
                .entry(parent.clone())
                .or_default()
                .push(spec.route_const.clone());
        } else {
            top_level.push(spec.route_const.clone());
        }
    }
    // Sort each child list for deterministic emission.
    for children in children_of.values_mut() {
        children.sort();
    }
    fn render(route_const: &str, children_of: &BTreeMap<String, Vec<String>>) -> String {
        if let Some(children) = children_of.get(route_const) {
            let parts: Vec<String> = children
                .iter()
                .map(|c| render(c, children_of))
                .collect();
            format!("{route_const}.addChildren([{}] as const)", parts.join(", "))
        } else {
            route_const.to_owned()
        }
    }
    top_level
        .iter()
        .map(|c| render(c, &children_of))
        .collect::<Vec<_>>()
        .join(", ")
}

/// router-w5 — emit `loader: ({ context }) => ...` when the route
/// declared one or more `loader <feature>.<query>` slots. Multiple
/// loaders run in parallel via `Promise.all`; each calls
/// `queryClient.ensureQueryData` so the data is hydrated before the
/// route component paints.
fn emit_loader(out: &mut String, spec: &RouteSpec) {
    if spec.loaders.is_empty() {
        return;
    }
    out.push_str("    loader: async ({ context }) => {\n");
    out.push_str("      await Promise.all([\n");
    for loader in &spec.loaders {
        out.push_str(&format!(
            "        context.queryClient.ensureQueryData({{ queryKey: queryKeyFor({q}, {{}}), queryFn: () => context.client.runQuery({q}, {{}}) }}),\n",
            q = loader.query_export
        ));
    }
    out.push_str("      ]);\n");
    out.push_str("    },\n");
}

/// router-w4 — resolve `requires_lifecycle <Resource> = <state>` into
/// a `LifecycleEmit` by locating the owning feature (the one with a
/// `lookup_my_<snake_resource>` query and a `lifecycle_routes` table
/// on the resource).
fn resolve_lifecycle_emit(
    rl: &RequiresLifecycle,
    features: &[Feature],
) -> Option<LifecycleEmit> {
    let snake = snake_case(&rl.resource);
    let lookup_name = format!("lookup_my_{snake}");
    for feature in features {
        // The resource must live in this feature.
        let Some(resource) = feature.resources.iter().find(|r| r.name == rl.resource) else {
            continue;
        };
        if resource.lifecycle_routes.is_none() {
            return None;
        }
        // The lookup query must exist on the same feature.
        let has_lookup = feature
            .queries
            .iter()
            .any(|q| q.name() == lookup_name.as_str());
        if !has_lookup {
            return None;
        }
        return Some(LifecycleEmit {
            feature: feature.name.clone(),
            lookup_export: lower_camel_export(&lookup_name),
            helper_export: lower_camel_export(&format!("{snake}_lifecycle_route")),
            required_state: rl.state.clone(),
        });
    }
    None
}

fn snake_case(s: &str) -> String {
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

fn lower_camel_export(s: &str) -> String {
    super::runtime::lower_camel_export(s)
}

// resolve_guard_emit defined above (combined policy + forbid_when payload).

/// Decompose a policy reference into its atoms. Handles two shapes:
/// 1. Atom-form: `@<ns>.<name>` (e.g. `@role.host`) — single atom.
/// 2. Named-form: `@policy.<name>` — look up the policy by name across
///    every feature's catalog; flatten its atom strings.
fn resolve_policy_atoms(policy_ref: &str, features: &[Feature]) -> Vec<(String, String)> {
    let bare = policy_ref.strip_prefix('@').unwrap_or(policy_ref);
    let (ns, name) = match bare.split_once('.') {
        Some(parts) => parts,
        None => return Vec::new(),
    };
    if ns == "policy" {
        // Named policy — look it up in every feature's catalog.
        for feature in features {
            for category in &feature.policies.categories {
                if category.name == name {
                    return category
                        .atoms
                        .iter()
                        .filter_map(|atom| parse_atom_string(atom))
                        .collect();
                }
            }
        }
        Vec::new()
    } else {
        // Inline atom shape: `@role.host` etc.
        vec![(ns.to_owned(), name.to_owned())]
    }
}

fn parse_atom_string(s: &str) -> Option<(String, String)> {
    let bare = s.strip_prefix('@').unwrap_or(s);
    let (ns, rest) = bare.split_once('.')?;
    // Strip any trailing `(args)` parameterisation; codegen doesn't
    // forward those today (only `@mfa.required(within:15m)` carries
    // args, and route guards don't use it).
    let name = rest.split('(').next().unwrap_or(rest);
    Some((ns.to_owned(), name.to_owned()))
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

fn pascal_case(value: &str) -> String {
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

fn ts_string(value: &str) -> String {
    format!("{:?}", value)
}
