//! Cell C.1 — audience-scoped SDK projection per L0 #3 §7.
//!
//! Given a frontend's audience declarations (each carrying a set of
//! `requires @scope.X` atoms), compute which commands/queries are
//! reachable from any of those audiences and emit a filtered
//! `<feat>.gen.ts`. The filter is enforced **at compile time** by the
//! generated TypeScript: a `public` frontend bundle simply does not
//! `export` admin-only mutations, so any import attempt fails the
//! `tsc` build.
//!
//! The audience<->command match is a set intersection of policy atoms.
//! For a command whose effective policy resolves to
//! `{@scope.workspace_admin}` and an audience block declaring
//! `requires @scope.workspace_admin`, the intersection is non-empty
//! and the command is admitted. If the audience required only
//! `@scope.workspace_member` instead, the intersection is empty and
//! the command is dropped.
//!
//! ## Query handling
//!
//! `lazuli_ir::Query` (and `RuntimeQuery`) does not carry an explicit
//! read-policy in v0 — `query.list` / `query.lookup` / `query.sql` are
//! universally readable inside a tenant, with row-level scoping handled
//! via `scope <predicate>` rather than gate atoms. Until policy gates
//! exist on queries, every query in the feature is admitted whenever
//! the audience set is non-empty, and the projection drops queries only
//! when the audience set is empty (matching the "Empty audience list
//! returns empty projection" test).
//!
//! When `Query.policy` lands in a future cell, the same intersection
//! algorithm flips on automatically — there is one place
//! (`policy_atoms_for_query`) to wire it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use lazuli_codegen_spec::{RuntimeCommand, RuntimeFeature, RuntimeQuery};
use lazuli_ir::{
    AppManifest, AppRoute, Experience, Feature, Platform, PlatformSurface, RouteGuardDefaults,
    ViewGuard,
};

use crate::GeneratedFile;
pub use crate::lifecycle_gate_emit::{
    LifecycleGateIntegration, LifecycleGateTarget, emit_lifecycle_gate_artifacts,
    emit_lifecycle_gate_artifacts_from_json,
};
use crate::lzx::lzx_router_adapter::route_guard_pattern_header;
use crate::lzx_audience_slot::ir::Audience;
use crate::lzx_audience_slot::pascal_case;
use crate::runtime::emit_feature_ts;

/// Projection of the per-frontend audience set onto a feature's
/// SDK surface. `audiences` lists the audience names that drove the
/// projection (sorted, dedup'd); `allowed_commands` / `allowed_queries`
/// are the short-name sets retained from the feature.
///
/// The two `BTreeSet<String>` slots store the **short names** of the
/// commands/queries as authored in the DSL (e.g. `"create"`,
/// `"update_email"`, `"list"`, `"by_id"`). Using `BTreeSet` is
/// deliberate: it gives stable iteration order so two projections
/// computed with the same inputs serialize byte-for-byte identically
/// regardless of the order in which audiences were declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudienceProjection {
    /// Sorted, dedup'd audience names that participated in the
    /// projection. Empty when `audiences` parameter was empty.
    pub audiences: Vec<String>,
    /// Short names of commands admitted by at least one audience.
    pub allowed_commands: BTreeSet<String>,
    /// Short names of queries admitted by at least one audience.
    pub allowed_queries: BTreeSet<String>,
}

impl AudienceProjection {
    /// `true` when the projection admits nothing. Used by the doctor
    /// rule `AUDIENCE-EMPTY-SDK` (§11) once it lands; also exposed
    /// for the deterministic-empty tests.
    pub fn is_empty(&self) -> bool {
        self.allowed_commands.is_empty() && self.allowed_queries.is_empty()
    }
}

/// Compute the projection of a feature module against a set of
/// audience declarations.
///
/// **Note on signature** — the L0 #3 §7 spec types this as
/// `audience_names: &[String]` (the list of names referenced by
/// `[frontends.X] audiences = [...]`). Resolving names to `requires`
/// atoms requires walking the `.lzx` Surface IR, which has not yet
/// landed in `lazuli_ir::Module` (parallel parser cell). Until then
/// this function takes the resolved `Audience` records directly so
/// Cell C.1 ships ahead of the parser cell. The orchestrator will add
/// a `compute_audience_projection_by_names(module, &[String])` thin
/// wrapper once the parser cell publishes audiences into `Module`.
///
/// Walks `module.commands` and `module.queries`, intersecting each
/// item's effective policy atom set against the union of every
/// audience's `requires` set. Items with any overlap are admitted.
pub fn compute_audience_projection(
    module: &RuntimeFeature,
    audiences: &[Audience],
) -> AudienceProjection {
    // Audience names — sorted + dedup'd for deterministic output.
    let mut audience_names: BTreeSet<String> = BTreeSet::new();
    for aud in audiences {
        audience_names.insert(aud.name.clone());
    }
    let audience_names: Vec<String> = audience_names.into_iter().collect();

    // Empty audiences → empty projection. Two callsites depend on this
    // contract: (a) the "Empty audience list" test below, and (b) the
    // future `AUDIENCE-EMPTY-SDK` doctor warning. Treat an empty set
    // as "this frontend exposes nothing" rather than "wildcard".
    if audiences.is_empty() {
        return AudienceProjection {
            audiences: audience_names,
            allowed_commands: BTreeSet::new(),
            allowed_queries: BTreeSet::new(),
        };
    }

    // Union of all required atoms across audiences (OR semantics per
    // §7.2). Stored as canonical `@namespace.name` strings so we can
    // compare against `RuntimeCommand.policy_atoms` (which carries
    // `(namespace, name)` tuples).
    let required: BTreeSet<String> = audiences
        .iter()
        .flat_map(|a| {
            a.requires
                .iter()
                .map(|p| format!("@{}.{}", p.namespace, p.name))
        })
        .collect();

    // Walk commands. Empty required set with non-empty audiences (an
    // audience block with no `requires` atoms) admits nothing — that's
    // an authoring smell, but we don't speculate beyond the projection.
    let mut allowed_commands = BTreeSet::new();
    for command in &module.commands {
        let command_atoms = policy_atoms_for_command(command);
        if command_atoms.is_empty() {
            // Command with no policy gate (e.g. `policy @policy.none`
            // or omitted) — universally admitted for any non-empty
            // audience. Matches the runtime spec semantics: such
            // commands carry no gate and are tenant-scoped only.
            allowed_commands.insert(command.short_name.clone());
            continue;
        }
        if command_atoms.iter().any(|atom| required.contains(atom)) {
            allowed_commands.insert(command.short_name.clone());
        }
    }

    // Walk queries. v0 IR has no `Query.policy`; treat all queries as
    // universally admitted when audiences is non-empty. See module
    // docstring for the upgrade path.
    let mut allowed_queries = BTreeSet::new();
    for query in &module.queries {
        let query_atoms = policy_atoms_for_query(query);
        if query_atoms.is_empty() {
            allowed_queries.insert(query.short_name.clone());
            continue;
        }
        if query_atoms.iter().any(|atom| required.contains(atom)) {
            allowed_queries.insert(query.short_name.clone());
        }
    }

    AudienceProjection {
        audiences: audience_names,
        allowed_commands,
        allowed_queries,
    }
}

/// Resolve the canonical `@namespace.name` atom strings on a command.
/// `RuntimeCommand.policy_atoms: Vec<(namespace, name)>` is already the
/// resolved form — the analyzer expanded `policy @policy.admin_only` to
/// its underlying scope/role atoms before lowering. We just stringify.
fn policy_atoms_for_command(command: &RuntimeCommand) -> Vec<String> {
    command
        .policy_atoms
        .iter()
        .map(|(ns, name)| format!("@{}.{}", ns, name))
        .collect()
}

/// Resolve policy atoms on a query. v0 IR carries `policy_atoms` on
/// the runtime spec (mirroring commands) for forward-compatibility,
/// even though the analyzer leaves it empty for query.list/lookup/sql
/// today. When the parser starts populating query policy, this
/// function picks it up with no further changes.
fn policy_atoms_for_query(query: &RuntimeQuery) -> Vec<String> {
    query
        .policy_atoms
        .iter()
        .map(|(ns, name)| format!("@{}.{}", ns, name))
        .collect()
}

/// Emit a feature's `<feat>.gen.ts` with the projection applied. The
/// output is structurally identical to `emit_feature_ts(feature)` but
/// with commands/queries NOT in `projection.allowed_*` filtered out
/// before emission.
///
/// Filtering happens at the spec level (not via string editing) so
/// determinism is preserved end-to-end: identical projections produce
/// identical bytes regardless of the input feature's command order.
pub fn emit_feature_sdk_filtered(
    feature: &RuntimeFeature,
    projection: &AudienceProjection,
) -> String {
    let filtered = RuntimeFeature {
        name: feature.name.clone(),
        source_path: feature.source_path.clone(),
        resources: feature.resources.clone(),
        commands: feature
            .commands
            .iter()
            .filter(|c| projection.allowed_commands.contains(&c.short_name))
            .cloned()
            .collect(),
        queries: feature
            .queries
            .iter()
            .filter(|q| projection.allowed_queries.contains(&q.short_name))
            .cloned()
            .collect(),
    };
    emit_feature_ts(&filtered)
}

/// Target prefix for route-guard artifacts. Route guards are emitted beside
/// audience SDK files (`dist/ts-web/...` or `dist/ts-mobile/...`) without
/// changing runtime code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RouteGuardTarget {
    Web,
    Mobile,
}

impl RouteGuardTarget {
    fn dist_prefix(self) -> &'static str {
        match self {
            RouteGuardTarget::Web => "ts-web",
            RouteGuardTarget::Mobile => "ts-mobile",
        }
    }

    fn platform_label(self) -> &'static str {
        match self {
            RouteGuardTarget::Web => "web",
            RouteGuardTarget::Mobile => "mobile",
        }
    }

    fn platform(self) -> Platform {
        match self {
            RouteGuardTarget::Web => Platform::Web,
            RouteGuardTarget::Mobile => Platform::Mobile,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RouteGroupKey {
    feature: String,
    platform: String,
    audience: String,
}

impl RouteGroupKey {
    fn file_path(&self, target: RouteGuardTarget) -> String {
        format!(
            "dist/{}/{}/{}.{}.{}.gen.ts",
            target.dist_prefix(),
            self.feature,
            self.feature,
            self.platform,
            self.audience
        )
    }

    fn registry_import_path(&self) -> String {
        format!(
            "../{}/{}.{}.{}.gen.js",
            self.feature, self.feature, self.platform, self.audience
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedPolicy {
    name: Option<String>,
    atoms: Vec<RoutePolicyAtom>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedGuard {
    policies: Vec<ResolvedPolicy>,
    on_unauthenticated: Option<String>,
    on_unauthorized: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoutePolicyAtom {
    namespace: String,
    name: String,
}

trait PolicyRefList {
    fn policy_refs(&self) -> Vec<&str>;
}

impl PolicyRefList for String {
    fn policy_refs(&self) -> Vec<&str> {
        vec![self.as_str()]
    }
}

impl PolicyRefList for Vec<String> {
    fn policy_refs(&self) -> Vec<&str> {
        self.iter().map(String::as_str).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteObject {
    path: String,
    const_name: String,
    component: String,
    guard: ResolvedGuard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteRegistryEntry {
    path: String,
    const_name: String,
    group: RouteGroupKey,
}

/// Emit route-guard metadata artifacts for the new guard-bearing `.lzx` IR.
///
/// No-op contract: when no route/view/audience guard is declared and the app
/// has no `route_guard` block, this returns an empty vec even if
/// `actor_query` is set. That keeps existing projects byte-for-byte stable.
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

fn has_route_guard_surface(
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

fn route_context<'a>(
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

fn resolve_route_guard(
    route_guard: Option<&ViewGuard>,
    view_guard: Option<&ViewGuard>,
    experience_guard: Option<&ViewGuard>,
    audience_guard: Option<&ViewGuard>,
    app_defaults: Option<&RouteGuardDefaults>,
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

fn build_policy_lookup(features: &[Feature]) -> BTreeMap<(String, String), Vec<RoutePolicyAtom>> {
    let mut out = BTreeMap::new();
    for feature in features {
        for category in &feature.policies.categories {
            let mut atoms: Vec<RoutePolicyAtom> = category
                .atoms
                .iter()
                .filter_map(|atom| parse_policy_atom(atom))
                .collect();
            sort_policy_atoms(&mut atoms);
            out.insert((feature.name.clone(), category.name.clone()), atoms);
        }
    }
    out
}

fn resolve_policy(
    policy: &str,
    policies: &BTreeMap<(String, String), Vec<RoutePolicyAtom>>,
    default_feature: &str,
) -> ResolvedPolicy {
    if !policy.starts_with("@policy.") {
        if let Some(atom) = parse_policy_atom(policy) {
            return ResolvedPolicy {
                name: None,
                atoms: vec![atom],
            };
        }
    }

    if let Some((feature, category)) = parse_policy_ref(policy, default_feature) {
        let atoms = policies
            .get(&(feature, category))
            .cloned()
            .unwrap_or_default();
        return ResolvedPolicy {
            name: Some(policy.to_owned()),
            atoms,
        };
    }

    ResolvedPolicy {
        name: Some(policy.to_owned()),
        atoms: Vec::new(),
    }
}

fn parse_policy_ref(policy: &str, default_feature: &str) -> Option<(String, String)> {
    let tail = policy.strip_prefix("@policy.")?;
    let mut parts = tail.split('.');
    let first = parts.next()?.trim();
    let second = parts.next();
    if let Some(second) = second {
        let rest = std::iter::once(second)
            .chain(parts)
            .collect::<Vec<_>>()
            .join(".");
        Some((first.to_owned(), rest))
    } else {
        Some((default_feature.to_owned(), first.to_owned()))
    }
}

fn parse_policy_atom(value: &str) -> Option<RoutePolicyAtom> {
    let raw = value.trim().trim_start_matches('@');
    let (namespace, name) = raw.split_once('.')?;
    if namespace.is_empty() || name.is_empty() || namespace == "policy" {
        return None;
    }
    Some(RoutePolicyAtom {
        namespace: namespace.to_owned(),
        name: name.to_owned(),
    })
}

fn sort_policy_atoms(atoms: &mut Vec<RoutePolicyAtom>) {
    atoms.sort_by(|a, b| {
        policy_namespace_rank(&a.namespace)
            .cmp(&policy_namespace_rank(&b.namespace))
            .then(a.namespace.cmp(&b.namespace))
            .then(a.name.cmp(&b.name))
    });
    atoms.dedup_by(|a, b| a.namespace == b.namespace && a.name == b.name);
}

fn policy_namespace_rank(namespace: &str) -> u8 {
    match namespace {
        "scope" => 0,
        "role" => 1,
        "actor" => 2,
        _ => 9,
    }
}

fn emit_audience_route_guard_sdk(routes: &[RouteObject]) -> String {
    let mut s = String::new();
    s.push_str("// Code generated by lazuli; DO NOT EDIT.\n");
    s.push_str(&route_guard_pattern_header());
    writeln!(
        s,
        "import type {{ RouteGuardSpec }} from \"@lazuli/runtime/react\";"
    )
    .ok();

    let components: BTreeSet<&str> = routes
        .iter()
        .map(|route| route.component.as_str())
        .collect();
    for component in components {
        writeln!(
            s,
            "import {{ {} }} from \"./components/{}.js\";",
            component, component
        )
        .ok();
    }
    writeln!(s).ok();

    for (index, route) in routes.iter().enumerate() {
        writeln!(s, "export const {} = {{", route.const_name).ok();
        writeln!(s, "  path: {},", ts_string(&route.path)).ok();
        writeln!(s, "  component: {},", route.component).ok();
        write_policy_list_property(&mut s, "policy", &route.guard.policies, 2);
        if let Some(path) = &route.guard.on_unauthenticated {
            writeln!(s, "  onUnauthenticated: {},", ts_string(path)).ok();
        }
        if let Some(path) = &route.guard.on_unauthorized {
            writeln!(s, "  onUnauthorized: {},", ts_string(path)).ok();
        }
        writeln!(
            s,
            "}} as const satisfies RouteGuardSpec<typeof {}>;",
            route.component
        )
        .ok();
        if index + 1 < routes.len() {
            writeln!(s).ok();
        }
    }

    s
}

fn emit_route_guard_registry(
    app: Option<&AppManifest>,
    app_defaults: Option<&RouteGuardDefaults>,
    policies: &BTreeMap<(String, String), Vec<RoutePolicyAtom>>,
    entries: &[RouteRegistryEntry],
) -> String {
    let mut s = String::new();
    s.push_str("// Code generated by lazuli; DO NOT EDIT.\n");
    s.push_str(&route_guard_pattern_header());
    writeln!(
        s,
        "import type {{ ActorQueryRef, RouteGuard, RouteGuardRegistry }} from \"@lazuli/runtime/react\";"
    )
    .ok();

    let mut imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in entries {
        imports
            .entry(entry.group.registry_import_path())
            .or_default()
            .insert(entry.const_name.clone());
    }
    for (path, names) in imports {
        let joined = names.into_iter().collect::<Vec<_>>().join(", ");
        writeln!(s, "import {{ {} }} from \"{}\";", joined, path).ok();
    }
    writeln!(s).ok();

    writeln!(
        s,
        "const resolvedRouteGuards: Record<string, RouteGuard> = {{"
    )
    .ok();
    for entry in entries {
        writeln!(s, "  {}: {},", ts_string(&entry.path), entry.const_name).ok();
    }
    writeln!(s, "}};").ok();
    writeln!(s).ok();

    writeln!(s, "export const routeGuardRegistry = {{").ok();
    writeln!(s, "  defaults: {{").ok();
    if let Some(default_policy) = app_defaults.and_then(|defaults| defaults.default_policy.as_ref())
    {
        let policy = resolve_policy(default_policy, policies, "");
        write_policy_list_property(&mut s, "policy", std::slice::from_ref(&policy), 4);
    } else {
        writeln!(s, "    policy: null,").ok();
    }
    write_nullable_string_property(
        &mut s,
        "unauthenticated",
        app_defaults.and_then(|defaults| defaults.on_unauthenticated.as_deref()),
        4,
    );
    write_nullable_string_property(
        &mut s,
        "unauthorized",
        app_defaults.and_then(|defaults| defaults.on_unauthorized.as_deref()),
        4,
    );
    write_nullable_string_property(
        &mut s,
        "skeleton",
        app_defaults.and_then(|defaults| defaults.skeleton.as_deref()),
        4,
    );
    writeln!(s, "  }},").ok();
    match app.and_then(|app| app.actor_query.as_ref()) {
        Some(actor_query) => {
            writeln!(
                s,
                "  actorQuery: {} as ActorQueryRef,",
                ts_string(actor_query)
            )
            .ok();
        }
        None => {
            writeln!(s, "  actorQuery: null,").ok();
        }
    }
    writeln!(s, "  routes: resolvedRouteGuards,").ok();
    writeln!(s, "}} as const satisfies RouteGuardRegistry;").ok();

    s
}

fn write_policy_list_property(
    s: &mut String,
    property: &str,
    policies: &[ResolvedPolicy],
    indent: usize,
) {
    let pad = " ".repeat(indent);
    if let [policy] = policies {
        writeln!(s, "{}{}: [{{", pad, property).ok();
        write_policy_object_body(s, policy, indent);
        writeln!(s, "{}}}],", pad).ok();
        return;
    }
    writeln!(s, "{}{}: [", pad, property).ok();
    for policy in policies {
        write_policy_object(s, policy, indent + 2);
    }
    writeln!(s, "{}],", pad).ok();
}

fn write_policy_object(s: &mut String, policy: &ResolvedPolicy, indent: usize) {
    let pad = " ".repeat(indent);
    writeln!(s, "{}{{", pad).ok();
    write_policy_object_body(s, policy, indent);
    writeln!(s, "{}}},", pad).ok();
}

fn write_policy_object_body(s: &mut String, policy: &ResolvedPolicy, indent: usize) {
    let pad = " ".repeat(indent);
    if let Some(name) = &policy.name {
        writeln!(s, "{}  name: {},", pad, ts_string(name)).ok();
    }
    writeln!(s, "{}  atoms: [", pad).ok();
    for atom in &policy.atoms {
        writeln!(
            s,
            "{}    {{ namespace: {}, name: {} }},",
            pad,
            ts_string(&atom.namespace),
            ts_string(&atom.name)
        )
        .ok();
    }
    writeln!(s, "{}  ],", pad).ok();
}

fn write_nullable_string_property(
    s: &mut String,
    property: &str,
    value: Option<&str>,
    indent: usize,
) {
    let pad = " ".repeat(indent);
    match value {
        Some(value) => {
            writeln!(s, "{}{}: {},", pad, property, ts_string(value)).ok();
        }
        None => {
            writeln!(s, "{}{}: null,", pad, property).ok();
        }
    }
}

fn route_const_name(name: &str) -> String {
    let pascal = pascal_case(name);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::with_capacity(pascal.len() + "Route".len());
            for c in first.to_lowercase() {
                out.push(c);
            }
            out.push_str(chars.as_str());
            out.push_str("Route");
            out
        }
        None => "route".to_owned(),
    }
}

fn route_component_name(route: &AppRoute) -> String {
    format!("{}Screen", pascal_case(&route.name))
}

fn route_target_feature(to: Option<&str>) -> Option<String> {
    let target = to?.split('(').next()?.trim();
    let (feature, _) = target.split_once(".view.")?;
    (!feature.is_empty()).then(|| feature.to_owned())
}

fn route_target_view_name(to: Option<&str>) -> Option<String> {
    let target = to?.split('(').next()?.trim();
    let after_view = target.split(".view.").nth(1)?;
    let view = after_view.split('.').next()?.trim();
    (!view.is_empty()).then(|| view.to_owned())
}

fn surface_feature(surface: Option<&str>) -> Option<String> {
    surface?
        .split(|ch: char| ch == ' ' || ch == '.')
        .next()
        .filter(|feature| !feature.is_empty())
        .map(str::to_owned)
}

fn route_feature_from_name(name: &str) -> String {
    name.split(['_', '-'])
        .next()
        .filter(|feature| !feature.is_empty())
        .unwrap_or("app")
        .to_owned()
}

fn ts_string(value: &str) -> String {
    serde_json::to_string(value).expect("string literal must serialize")
}

// ---------------------------------------------------------------------------
// Tests — Cell C.1 ships ≥6 tests covering admission, exclusion, union,
// empty, filtered emission, and determinism.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use lazuli_codegen_spec::{
        FieldKind, QueryKind, RuntimeArg, RuntimeCommand, RuntimeEffect, RuntimeFeature,
        RuntimeField, RuntimeInput, RuntimeQuery, RuntimeResource, Tenancy,
    };

    use crate::lzx_audience_slot::ir::{Audience, PolicyAtom, View};

    // -----------------------------------------------------------------------
    // Fixtures
    // -----------------------------------------------------------------------

    fn slug_resource() -> RuntimeResource {
        RuntimeResource {
            name: "slug".to_owned(),
            tenancy: Tenancy::Org,
            soft_delete: false,
            retention: None,
            fields: vec![
                RuntimeField {
                    name: "key".to_owned(),
                    kind: FieldKind::Text,
                },
                RuntimeField {
                    name: "title".to_owned(),
                    kind: FieldKind::Text,
                },
            ],
        }
    }

    fn admin_only_command(name: &str) -> RuntimeCommand {
        RuntimeCommand {
            short_name: name.to_owned(),
            policy_name: "@policy.admin_only".to_owned(),
            policy_atoms: vec![("scope".to_owned(), "workspace_admin".to_owned())],
            rate_limit: String::new(),
            validators: vec![],
            effect: RuntimeEffect::CreatesFromInput,
            inputs: vec![RuntimeInput {
                field_name: "Key".to_owned(),
                kind: FieldKind::Text,
            }],
            emits: vec![],
            invalidates: vec![],
            deprecated: None,
        }
    }

    fn member_command(name: &str) -> RuntimeCommand {
        RuntimeCommand {
            short_name: name.to_owned(),
            policy_name: "@policy.member_read".to_owned(),
            policy_atoms: vec![("scope".to_owned(), "workspace_member".to_owned())],
            rate_limit: String::new(),
            validators: vec![],
            effect: RuntimeEffect::CreatesFromInput,
            inputs: vec![RuntimeInput {
                field_name: "Key".to_owned(),
                kind: FieldKind::Text,
            }],
            emits: vec![],
            invalidates: vec![],
            deprecated: None,
        }
    }

    fn ungated_query(name: &str, kind: QueryKind) -> RuntimeQuery {
        RuntimeQuery {
            short_name: name.to_owned(),
            kind,
            policy_name: String::new(),
            policy_atoms: vec![],
            args: vec![RuntimeArg {
                field_name: "ID".to_owned(),
                kind: FieldKind::Integer,
                optional: false,
            }],
            cache: None,
            paginate: 0,
            filters: vec![],
            search: None,
            lookup_by: vec![],
        }
    }

    fn slug_feature() -> RuntimeFeature {
        RuntimeFeature {
            name: "slug".to_owned(),
            source_path: "features/slug/slug.lzi".to_owned(),
            resources: vec![slug_resource()],
            commands: vec![
                admin_only_command("create"),
                admin_only_command("delete"),
                member_command("rename"),
            ],
            queries: vec![
                ungated_query("list", QueryKind::List),
                ungated_query("by_key", QueryKind::Lookup),
            ],
        }
    }

    fn admin_audience() -> Audience {
        Audience {
            name: "admin".to_owned(),
            requires: vec![PolicyAtom {
                namespace: "scope".to_owned(),
                name: "workspace_admin".to_owned(),
                args: None,
            }],
            views: Vec::<View>::new(),
            span_ref: None,
        }
    }

    fn public_audience() -> Audience {
        Audience {
            name: "public".to_owned(),
            requires: vec![PolicyAtom {
                namespace: "scope".to_owned(),
                name: "workspace_member".to_owned(),
                args: None,
            }],
            views: Vec::<View>::new(),
            span_ref: None,
        }
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// Spec §7.2 — admin audience admits a command whose effective
    /// policy resolves to `@scope.workspace_admin`.
    #[test]
    fn admin_audience_admits_admin_command() {
        let feature = slug_feature();
        let projection =
            compute_audience_projection(&feature, &[admin_audience()]);

        assert!(projection.allowed_commands.contains("create"));
        assert!(projection.allowed_commands.contains("delete"));
        assert_eq!(projection.audiences, vec!["admin".to_owned()]);
    }

    /// Spec §7.2 — public audience does NOT admit an admin_only
    /// command (intersection of required atoms is empty).
    #[test]
    fn public_audience_excludes_admin_only_command() {
        let feature = slug_feature();
        let projection =
            compute_audience_projection(&feature, &[public_audience()]);

        assert!(!projection.allowed_commands.contains("create"));
        assert!(!projection.allowed_commands.contains("delete"));
        assert!(
            projection.allowed_commands.contains("rename"),
            "member-gated command should be admitted for public audience"
        );
    }

    /// Multiple audiences union — admin + public together admit both
    /// admin-only and member commands. Audience names appear sorted
    /// in the projection regardless of input order.
    #[test]
    fn multiple_audiences_union_correctly() {
        let feature = slug_feature();
        let projection = compute_audience_projection(
            &feature,
            &[public_audience(), admin_audience()],
        );

        assert!(projection.allowed_commands.contains("create"));
        assert!(projection.allowed_commands.contains("delete"));
        assert!(projection.allowed_commands.contains("rename"));
        assert_eq!(
            projection.audiences,
            vec!["admin".to_owned(), "public".to_owned()],
            "audience names should be sorted regardless of input order"
        );
    }

    /// Empty audiences → empty projection. No commands or queries
    /// admitted. This drives the `AUDIENCE-EMPTY-SDK` doctor warning
    /// when it lands.
    #[test]
    fn empty_audience_list_returns_empty_projection() {
        let feature = slug_feature();
        let projection = compute_audience_projection(&feature, &[]);

        assert!(projection.allowed_commands.is_empty());
        assert!(projection.allowed_queries.is_empty());
        assert!(projection.audiences.is_empty());
        assert!(projection.is_empty());
    }

    /// `emit_feature_sdk_filtered` strips filtered commands from the
    /// emitted TS. The public bundle has no `deleteSlug` const because
    /// the projection excluded `delete`.
    #[test]
    fn emit_filtered_sdk_excludes_admin_commands_for_public() {
        let feature = slug_feature();
        let projection =
            compute_audience_projection(&feature, &[public_audience()]);
        let output = emit_feature_sdk_filtered(&feature, &projection);

        // `rename` is the only member-gated command — its identifier
        // is `renameSlug` per the runtime emitter's convention.
        assert!(
            output.contains("renameSlug"),
            "expected renameSlug to be emitted; got:\n{output}"
        );
        // `create` and `delete` MUST be absent (admin-only).
        assert!(
            !output.contains("createSlug"),
            "createSlug should be filtered out of public bundle; got:\n{output}"
        );
        assert!(
            !output.contains("deleteSlug"),
            "deleteSlug should be filtered out of public bundle; got:\n{output}"
        );
    }

    /// Deterministic emission — same projection produces identical
    /// bytes regardless of how the audiences vec is permuted.
    #[test]
    fn projection_emission_is_deterministic() {
        let feature = slug_feature();
        let proj_a = compute_audience_projection(
            &feature,
            &[admin_audience(), public_audience()],
        );
        let proj_b = compute_audience_projection(
            &feature,
            &[public_audience(), admin_audience()],
        );
        assert_eq!(proj_a, proj_b);

        let out_a = emit_feature_sdk_filtered(&feature, &proj_a);
        let out_b = emit_feature_sdk_filtered(&feature, &proj_b);
        assert_eq!(out_a, out_b);
    }

    /// All queries are admitted whenever the audience set is
    /// non-empty (v0 IR — queries have no policy gate). This locks
    /// the documented contract so a future tightening is an explicit
    /// IR change, not a silent regression.
    #[test]
    fn queries_admitted_for_any_nonempty_audience() {
        let feature = slug_feature();

        for aud in [admin_audience(), public_audience()] {
            let projection = compute_audience_projection(&feature, &[aud]);
            assert!(projection.allowed_queries.contains("list"));
            assert!(projection.allowed_queries.contains("by_key"));
        }
    }
}
