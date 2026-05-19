//! Lifecycle-gate TS emitter (LAZ-88 / CODEGEN-1).
//!
//! The IR/parser/analyzer cells for this proposal land in parallel, so this
//! emitter reads the additive lifecycle-gate fields through serialized IR JSON.
//! That keeps this codegen cell compiling before `ResumeRouter` and
//! `ResolvedLifecycleGate` become concrete Rust types, while still consuming the
//! same field names the final IR shape serializes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use lazuli_ir::Module;
use serde_json::Value;

use crate::GeneratedFile;
use crate::lzx::lzx_router_adapter::lifecycle_gate_pattern_header;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleGateTarget {
    Web,
    Mobile,
}

impl LifecycleGateTarget {
    fn dist_prefix(self) -> &'static str {
        match self {
            LifecycleGateTarget::Web => "ts-web",
            LifecycleGateTarget::Mobile => "ts-mobile",
        }
    }

    fn platform_label(self) -> &'static str {
        match self {
            LifecycleGateTarget::Web => "web",
            LifecycleGateTarget::Mobile => "mobile",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleGateIntegration {
    TanStack,
    Hoc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceLifecycle {
    feature: String,
    name: String,
    discriminator_field: String,
    state_type: String,
    states: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResumeRouter {
    feature: String,
    name: String,
    resource: ResourceLifecycle,
    source_feature: String,
    source_query: String,
    source_query_ident: String,
    arms: BTreeMap<String, String>,
    none_target: String,
    wildcard_target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleGate {
    feature: String,
    platform: String,
    audience: String,
    view_name: String,
    path: String,
    component: String,
    route_const: String,
    resource: String,
    expected_state: String,
    resume_feature: String,
    resume_name: String,
    guard: RouteGuardShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteGuardShape {
    name: Option<String>,
    atoms: Vec<PolicyAtom>,
    on_unauthenticated: Option<String>,
    on_unauthorized: Option<String>,
}

impl Default for RouteGuardShape {
    fn default() -> Self {
        Self {
            name: None,
            atoms: Vec::new(),
            on_unauthenticated: None,
            on_unauthorized: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PolicyAtom {
    namespace: String,
    name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResumeRef {
    feature: String,
    name: String,
}

/// Emit lifecycle-gate artifacts from the typed module. This is a no-op until
/// the additive IR fields from LAZ-85/86/87 are present in serialized form.
pub fn emit_lifecycle_gate_artifacts(
    module: &Module,
    target: LifecycleGateTarget,
    integration: LifecycleGateIntegration,
) -> Vec<GeneratedFile> {
    let Ok(value) = serde_json::to_value(module) else {
        return Vec::new();
    };
    emit_lifecycle_gate_artifacts_from_json(&value, target, integration)
}

/// Testable JSON entrypoint for the parallel IR-cell window.
pub fn emit_lifecycle_gate_artifacts_from_json(
    root: &Value,
    target: LifecycleGateTarget,
    integration: LifecycleGateIntegration,
) -> Vec<GeneratedFile> {
    let view_paths = collect_view_paths(root);
    let resources = collect_lifecycle_resources(root);
    let query_resources = collect_query_resources(root, &resources);
    let resumes = collect_resume_routers(root, &resources, &query_resources, &view_paths);
    let gates = collect_lifecycle_gates(root, target, &view_paths);

    if resumes.is_empty() && gates.is_empty() {
        return Vec::new();
    }

    let resume_by_key: BTreeMap<(String, String), ResumeRouter> = resumes
        .iter()
        .map(|resume| {
            (
                (resume.feature.clone(), resume.name.clone()),
                resume.clone(),
            )
        })
        .collect();

    let mut groups: BTreeMap<(String, String, String), Vec<LifecycleGate>> = BTreeMap::new();
    for gate in gates {
        if gate.platform != target.platform_label() {
            continue;
        }
        groups
            .entry((
                gate.feature.clone(),
                gate.platform.clone(),
                gate.audience.clone(),
            ))
            .or_default()
            .push(gate);
    }

    for gates in groups.values_mut() {
        gates.sort_by(|a, b| a.route_const.cmp(&b.route_const));
        gates.dedup_by(|a, b| a.route_const == b.route_const);
    }

    let mut files = Vec::new();
    for ((feature, platform, audience), gates) in &groups {
        let mut used_resumes = BTreeMap::new();
        for gate in gates {
            if let Some(resume) =
                resume_by_key.get(&(gate.resume_feature.clone(), gate.resume_name.clone()))
            {
                used_resumes.insert(
                    (resume.feature.clone(), resume.name.clone()),
                    resume.clone(),
                );
            }
        }
        if used_resumes.is_empty() {
            continue;
        }
        files.push(GeneratedFile {
            path: format!(
                "dist/{}/{}/{}.{}.{}.gen.ts",
                target.dist_prefix(),
                feature,
                feature,
                platform,
                audience
            ),
            contents: emit_group_file(
                feature,
                gates,
                used_resumes
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
                    .as_slice(),
                integration,
            ),
        });
    }

    if !groups.is_empty() {
        let all_gates: Vec<LifecycleGate> = groups
            .values()
            .flat_map(|gates| gates.iter().cloned())
            .collect();
        files.push(GeneratedFile {
            path: format!("dist/{}/app/lifecycle_gates.gen.ts", target.dist_prefix()),
            contents: emit_registry_file(&all_gates),
        });
    }

    files
}

fn collect_lifecycle_resources(root: &Value) -> BTreeMap<(String, String), ResourceLifecycle> {
    let mut out = BTreeMap::new();
    for feature in features(root) {
        let feature_name = string_field(feature, "name").unwrap_or("app");
        for resource in array_field(feature, "resources") {
            let Some(resource_name) = string_field(resource, "name") else {
                continue;
            };
            let Some(lifecycle) = resource.get("lifecycle") else {
                continue;
            };
            let states: Vec<String> = array_field(lifecycle, "states")
                .iter()
                .filter_map(|state| string_field(state, "name").map(str::to_owned))
                .collect();
            if states.is_empty() {
                continue;
            }
            let state_type = string_field(lifecycle, "generated_enum")
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{}LifecycleState", pascal_case(resource_name)));
            out.insert(
                (feature_name.to_owned(), canonical(resource_name)),
                ResourceLifecycle {
                    feature: feature_name.to_owned(),
                    name: pascal_case(resource_name),
                    discriminator_field: string_field(lifecycle, "discriminator_field")
                        .unwrap_or("lifecycle_state")
                        .to_owned(),
                    state_type,
                    states,
                },
            );
        }
    }
    out
}

fn collect_query_resources(
    root: &Value,
    resources: &BTreeMap<(String, String), ResourceLifecycle>,
) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    for feature in features(root) {
        let feature_name = string_field(feature, "name").unwrap_or("app");
        let feature_resources: Vec<&ResourceLifecycle> = resources
            .values()
            .filter(|resource| resource.feature == feature_name)
            .collect();
        for query in array_field(feature, "queries") {
            let Some(query_name) = string_field(query, "name") else {
                continue;
            };
            if let Some(resource) = string_field(query, "resource")
                .or_else(|| string_field(query, "returns_resource"))
                .or_else(|| string_field(query, "return_resource"))
            {
                out.insert(
                    (feature_name.to_owned(), query_name.to_owned()),
                    resource.to_owned(),
                );
                continue;
            }
            if let Some(resource) = pick_resource_for_query(query_name, &feature_resources) {
                out.insert(
                    (feature_name.to_owned(), query_name.to_owned()),
                    resource.name.clone(),
                );
            }
        }
    }
    out
}

fn collect_resume_routers(
    root: &Value,
    resources: &BTreeMap<(String, String), ResourceLifecycle>,
    query_resources: &BTreeMap<(String, String), String>,
    view_paths: &BTreeMap<(String, String), String>,
) -> Vec<ResumeRouter> {
    let mut out = Vec::new();
    for feature in features(root) {
        let feature_name = string_field(feature, "name").unwrap_or("app");
        for (resume_name, resume) in resume_entries(feature) {
            let (source_feature, source_query) = parse_source_query(resume, feature_name)
                .unwrap_or_else(|| {
                    (
                        feature_name.to_owned(),
                        string_field(resume, "source_query")
                            .unwrap_or("lookup")
                            .to_owned(),
                    )
                });
            let resource_name = string_field(resume, "resource")
                .or_else(|| {
                    query_resources
                        .get(&(source_feature.clone(), source_query.clone()))
                        .map(String::as_str)
                })
                .or_else(|| only_resource_for_feature(resources, &source_feature))
                .unwrap_or(feature_name);
            let Some(resource) = lookup_resource(resources, &source_feature, resource_name)
                .or_else(|| lookup_resource_by_name(resources, resource_name))
            else {
                continue;
            };
            let arms = parse_resume_arms(resume, feature_name, view_paths);
            let none_target = arms
                .get("none")
                .cloned()
                .or_else(|| arms.get("*").cloned())
                .unwrap_or_else(|| "/".to_owned());
            out.push(ResumeRouter {
                feature: feature_name.to_owned(),
                name: resume_name,
                resource,
                source_query_ident: query_ident(&source_feature, &source_query),
                source_feature,
                source_query,
                wildcard_target: arms.get("*").cloned(),
                arms,
                none_target,
            });
        }
    }
    out.sort_by(|a, b| a.feature.cmp(&b.feature).then(a.name.cmp(&b.name)));
    out
}

fn collect_lifecycle_gates(
    root: &Value,
    target: LifecycleGateTarget,
    view_paths: &BTreeMap<(String, String), String>,
) -> Vec<LifecycleGate> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let app_resume = root
        .get("app")
        .and_then(|app| app.get("route_guard"))
        .and_then(|guard| resume_ref_from_guard(guard, "app"));

    for feature in features(root) {
        let feature_name = string_field(feature, "name").unwrap_or("app");
        for surface in array_field(feature, "surfaces") {
            let platform = surface_platform(surface).unwrap_or(target.platform_label());
            for audience in array_field(surface, "audiences") {
                let audience_name = string_field(audience, "name").unwrap_or("default");
                let audience_gate =
                    extract_gate_from_holder(audience, feature_name, app_resume.as_ref());
                for view in array_field(audience, "views") {
                    let Some(view_name) = string_field(view, "name") else {
                        continue;
                    };
                    let gate = extract_gate_from_holder(view, feature_name, app_resume.as_ref())
                        .or_else(|| audience_gate.clone());
                    let Some((resource, expected_state, resume_ref)) = gate else {
                        continue;
                    };
                    let path = string_field(view, "route")
                        .map(str::to_owned)
                        .or_else(|| {
                            view_paths
                                .get(&(feature_name.to_owned(), view_name.to_owned()))
                                .cloned()
                        })
                        .unwrap_or_else(|| format!("/{}", view_name.replace('_', "-")));
                    push_gate(
                        &mut out,
                        &mut seen,
                        LifecycleGate {
                            feature: feature_name.to_owned(),
                            platform: platform.to_owned(),
                            audience: audience_name.to_owned(),
                            view_name: view_name.to_owned(),
                            path,
                            component: format!("{}Screen", pascal_case(view_name)),
                            route_const: route_const_name(view_name),
                            resource,
                            expected_state,
                            resume_feature: resume_ref.feature,
                            resume_name: resume_ref.name,
                            guard: extract_guard_shape(view).unwrap_or_default(),
                        },
                    );
                }
            }
        }
    }

    for surface in array_field(root, "surfaces") {
        let feature_name = string_field(surface, "experience").unwrap_or("app");
        let platform = surface_platform(surface).unwrap_or(target.platform_label());
        for audience in array_field(surface, "audiences") {
            let audience_name = string_field(audience, "name").unwrap_or("default");
            let audience_gate =
                extract_gate_from_holder(audience, feature_name, app_resume.as_ref());
            for view in array_field(audience, "views") {
                let Some(view_name) = string_field(view, "name") else {
                    continue;
                };
                let gate = extract_gate_from_holder(view, feature_name, app_resume.as_ref())
                    .or_else(|| audience_gate.clone());
                let Some((resource, expected_state, resume_ref)) = gate else {
                    continue;
                };
                let path = view_paths
                    .get(&(feature_name.to_owned(), view_name.to_owned()))
                    .cloned()
                    .unwrap_or_else(|| format!("/{}", view_name.replace('_', "-")));
                push_gate(
                    &mut out,
                    &mut seen,
                    LifecycleGate {
                        feature: feature_name.to_owned(),
                        platform: platform.to_owned(),
                        audience: audience_name.to_owned(),
                        view_name: view_name.to_owned(),
                        path,
                        component: format!("{}Screen", pascal_case(view_name)),
                        route_const: route_const_name(view_name),
                        resource,
                        expected_state,
                        resume_feature: resume_ref.feature,
                        resume_name: resume_ref.name,
                        guard: extract_guard_shape(view).unwrap_or_default(),
                    },
                );
            }
        }
    }

    for route in array_field(root, "routes") {
        let Some(path) = string_field(route, "path") else {
            continue;
        };
        let route_feature = string_field(route, "to")
            .and_then(parse_to_view)
            .map(|(feature, _)| feature)
            .or_else(|| string_field(route, "surface").and_then(surface_feature))
            .unwrap_or_else(|| route_name_feature(string_field(route, "name").unwrap_or("app")));
        let view_name = string_field(route, "to")
            .and_then(parse_to_view)
            .map(|(_, view)| view)
            .or_else(|| string_field(route, "name").map(str::to_owned))
            .unwrap_or_else(|| "route".to_owned());
        let platform = string_field(route, "surface")
            .and_then(surface_platform_label)
            .unwrap_or(target.platform_label());
        let audience = string_field(route, "audience").unwrap_or("default");
        let gate = route
            .get("guard")
            .and_then(|guard| extract_gate_from_guard(guard, &route_feature, app_resume.as_ref()));
        let Some((resource, expected_state, resume_ref)) = gate else {
            continue;
        };
        push_gate(
            &mut out,
            &mut seen,
            LifecycleGate {
                feature: route_feature,
                platform: platform.to_owned(),
                audience: audience.to_owned(),
                view_name: view_name.clone(),
                path: path.to_owned(),
                component: format!("{}Screen", pascal_case(&view_name)),
                route_const: route_const_name(&view_name),
                resource,
                expected_state,
                resume_feature: resume_ref.feature,
                resume_name: resume_ref.name,
                guard: extract_guard_shape(route).unwrap_or_default(),
            },
        );
    }

    out.sort_by(|a, b| {
        a.feature
            .cmp(&b.feature)
            .then(a.platform.cmp(&b.platform))
            .then(a.audience.cmp(&b.audience))
            .then(a.route_const.cmp(&b.route_const))
    });
    out
}

fn push_gate(
    out: &mut Vec<LifecycleGate>,
    seen: &mut BTreeSet<(String, String, String, String)>,
    gate: LifecycleGate,
) {
    let key = (
        gate.feature.clone(),
        gate.platform.clone(),
        gate.audience.clone(),
        gate.route_const.clone(),
    );
    if seen.insert(key) {
        out.push(gate);
    }
}

fn emit_group_file(
    feature: &str,
    gates: &[LifecycleGate],
    resumes: &[ResumeRouter],
    integration: LifecycleGateIntegration,
) -> String {
    let mut s = String::new();
    s.push_str("// Code generated by lazuli; DO NOT EDIT.\n");
    s.push_str(&lifecycle_gate_pattern_header());

    writeln!(
        s,
        "import {{ isLazuliError, type LazuliClient }} from \"@lazuli/runtime\";"
    )
    .ok();
    match integration {
        LifecycleGateIntegration::TanStack => {
            writeln!(s, "import {{ redirect }} from \"@tanstack/react-router\";").ok();
            writeln!(
                s,
                "import {{ withTanStackGuard, type RouteGuardSpec }} from \"@lazuli/runtime/react\";"
            )
            .ok();
        }
        LifecycleGateIntegration::Hoc => {
            writeln!(
                s,
                "import {{ withLifecycleGate, withRouteGuard, type RouteGuardSpec }} from \"@lazuli/runtime/react\";"
            )
            .ok();
        }
    }

    let mut component_imports: BTreeSet<&str> = BTreeSet::new();
    for gate in gates {
        component_imports.insert(gate.component.as_str());
    }
    for component in component_imports {
        writeln!(
            s,
            "import {{ {} }} from \"./components/{}.js\";",
            component, component
        )
        .ok();
    }

    let mut value_imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut type_imports: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for resume in resumes {
        let path = sdk_import_path(feature, &resume.source_feature);
        value_imports
            .entry(path.clone())
            .or_default()
            .insert(resume.source_query_ident.clone());
        type_imports
            .entry(path)
            .or_default()
            .insert(resume.resource.state_type.clone());
    }
    for (path, names) in value_imports {
        writeln!(
            s,
            "import {{ {} }} from \"{}\";",
            names.into_iter().collect::<Vec<_>>().join(", "),
            path
        )
        .ok();
    }
    for (path, names) in type_imports {
        writeln!(
            s,
            "import type {{ {} }} from \"{}\";",
            names.into_iter().collect::<Vec<_>>().join(", "),
            path
        )
        .ok();
    }
    writeln!(s).ok();

    for resume in resumes {
        write_resume_router(&mut s, resume);
        write_evaluator(&mut s, resume);
    }

    for gate in gates {
        write_route_spec(&mut s, gate);
        match integration {
            LifecycleGateIntegration::TanStack => write_tanstack_route_options(&mut s, gate),
            LifecycleGateIntegration::Hoc => write_hoc_guard(&mut s, gate),
        }
    }

    s
}

fn write_resume_router(s: &mut String, resume: &ResumeRouter) {
    writeln!(
        s,
        "// Generated from resume {} ({} surface).",
        resume.name, resume.feature
    )
    .ok();
    writeln!(
        s,
        "export function {}Resume(state: {} | null): string {{",
        lower_camel(&resume.name),
        resume.resource.state_type
    )
    .ok();
    writeln!(s, "  switch (state) {{").ok();
    writeln!(
        s,
        "    case null: return {};",
        ts_string(&resume.none_target)
    )
    .ok();
    for state in &resume.resource.states {
        let target = resume
            .arms
            .get(state)
            .or(resume.wildcard_target.as_ref())
            .unwrap_or(&resume.none_target);
        writeln!(
            s,
            "    case {}: return {};",
            ts_string(state),
            ts_string(target)
        )
        .ok();
    }
    match &resume.wildcard_target {
        Some(target) => {
            writeln!(s, "    default: return {};", ts_string(target)).ok();
        }
        None => {
            writeln!(
                s,
                "    default: {{ const _exhaustive: never = state; return {}; }}",
                ts_string(&resume.none_target)
            )
            .ok();
        }
    }
    writeln!(s, "  }}").ok();
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

fn write_evaluator(s: &mut String, resume: &ResumeRouter) {
    let state_field = lower_camel(&resume.resource.discriminator_field);
    writeln!(
        s,
        "export async function {}EvaluateGate(",
        lower_camel(&resume.name)
    )
    .ok();
    writeln!(s, "  client: LazuliClient,").ok();
    writeln!(s, "  expectedState: {},", resume.resource.state_type).ok();
    writeln!(
        s,
        "): Promise<{{ verdict: \"pass\" }} | {{ verdict: \"redirect\"; to: string }}> {{"
    )
    .ok();
    writeln!(s, "  try {{").ok();
    writeln!(
        s,
        "    const row = await client.runQuery({}, {{}});",
        resume.source_query_ident
    )
    .ok();
    writeln!(s, "    if (row === null || row === undefined) {{").ok();
    writeln!(
        s,
        "      return {{ verdict: \"redirect\", to: {}Resume(null) }};",
        lower_camel(&resume.name)
    )
    .ok();
    writeln!(s, "    }}").ok();
    writeln!(
        s,
        "    const state = row.{} as {};",
        state_field, resume.resource.state_type
    )
    .ok();
    writeln!(
        s,
        "    if (state === expectedState) return {{ verdict: \"pass\" }};"
    )
    .ok();
    writeln!(
        s,
        "    return {{ verdict: \"redirect\", to: {}Resume(state) }};",
        lower_camel(&resume.name)
    )
    .ok();
    writeln!(s, "  }} catch (err) {{").ok();
    writeln!(s, "    if (isLazuliError(err) && err.status === 404) {{").ok();
    writeln!(
        s,
        "      return {{ verdict: \"redirect\", to: {}Resume(null) }};",
        lower_camel(&resume.name)
    )
    .ok();
    writeln!(s, "    }}").ok();
    writeln!(s, "    if (isLazuliError(err) && err.status >= 500) {{").ok();
    writeln!(s, "      return {{ verdict: \"pass\" }};").ok();
    writeln!(s, "    }}").ok();
    writeln!(s, "    throw err;").ok();
    writeln!(s, "  }}").ok();
    writeln!(s, "}}").ok();
    writeln!(s).ok();
}

fn write_route_spec(s: &mut String, gate: &LifecycleGate) {
    writeln!(s, "export const {} = {{", gate.route_const).ok();
    writeln!(s, "  path: {},", ts_string(&gate.path)).ok();
    writeln!(s, "  component: {},", gate.component).ok();
    writeln!(s, "  policy: {{").ok();
    if let Some(name) = &gate.guard.name {
        writeln!(s, "    name: {},", ts_string(name)).ok();
    }
    writeln!(s, "    atoms: [").ok();
    for atom in &gate.guard.atoms {
        writeln!(
            s,
            "      {{ namespace: {}, name: {} }},",
            ts_string(&atom.namespace),
            ts_string(&atom.name)
        )
        .ok();
    }
    writeln!(s, "    ],").ok();
    writeln!(s, "  }},").ok();
    if let Some(path) = &gate.guard.on_unauthenticated {
        writeln!(s, "  onUnauthenticated: {},", ts_string(path)).ok();
    }
    if let Some(path) = &gate.guard.on_unauthorized {
        writeln!(s, "  onUnauthorized: {},", ts_string(path)).ok();
    }
    writeln!(
        s,
        "}} as const satisfies RouteGuardSpec<typeof {}>;",
        gate.component
    )
    .ok();
    writeln!(s).ok();
}

fn write_tanstack_route_options(s: &mut String, gate: &LifecycleGate) {
    let options_name = format!("{}Options", gate.route_const);
    let resume_fn = lower_camel(&gate.resume_name);
    writeln!(s, "export const {} = {{", options_name).ok();
    writeln!(s, "  path: {},", ts_string(&gate.path)).ok();
    writeln!(
        s,
        "  beforeLoad: async ({{ context }}: {{ context: {{ client: LazuliClient }} }}) => {{"
    )
    .ok();
    writeln!(s, "    await withTanStackGuard(").ok();
    writeln!(s, "      {{}},").ok();
    writeln!(s, "      {}.policy,", gate.route_const).ok();
    writeln!(s, "      {{").ok();
    writeln!(
        s,
        "        onUnauthenticated: {}.onUnauthenticated,",
        gate.route_const
    )
    .ok();
    writeln!(
        s,
        "        onUnauthorized: {}.onUnauthorized,",
        gate.route_const
    )
    .ok();
    writeln!(s, "        redirect,").ok();
    writeln!(s, "      }},").ok();
    writeln!(s, "    )({{ context }});").ok();
    writeln!(
        s,
        "    const lifecycleVerdict = await {}EvaluateGate(",
        resume_fn
    )
    .ok();
    writeln!(s, "      context.client,").ok();
    writeln!(s, "      {},", ts_string(&gate.expected_state)).ok();
    writeln!(s, "    );").ok();
    writeln!(s, "    if (lifecycleVerdict.verdict === \"redirect\") {{").ok();
    writeln!(
        s,
        "      throw redirect({{ to: lifecycleVerdict.to, replace: true }});"
    )
    .ok();
    writeln!(s, "    }}").ok();
    writeln!(s, "  }},").ok();
    writeln!(s, "  component: {},", gate.component).ok();
    writeln!(s, "}} as const;").ok();
    writeln!(s).ok();
}

fn write_hoc_guard(s: &mut String, gate: &LifecycleGate) {
    let resume_fn = lower_camel(&gate.resume_name);
    writeln!(
        s,
        "export const {}Guarded = withRouteGuard(",
        gate.component
    )
    .ok();
    writeln!(s, "  withLifecycleGate(").ok();
    writeln!(s, "    {},", gate.component).ok();
    writeln!(s, "    {{").ok();
    writeln!(s, "      evaluateGate: {}EvaluateGate,", resume_fn).ok();
    writeln!(
        s,
        "      expectedState: {},",
        ts_string(&gate.expected_state)
    )
    .ok();
    writeln!(s, "    }},").ok();
    writeln!(s, "  ),").ok();
    writeln!(s, "  {},", gate.route_const).ok();
    writeln!(s, ");").ok();
    writeln!(s).ok();
}

fn emit_registry_file(gates: &[LifecycleGate]) -> String {
    let mut s = String::new();
    s.push_str("// Code generated by lazuli; DO NOT EDIT.\n");
    s.push_str(&lifecycle_gate_pattern_header());
    writeln!(s, "export const lifecycleGates = {{").ok();
    let mut sorted = gates.to_vec();
    sorted.sort_by(|a, b| a.view_name.cmp(&b.view_name));
    sorted.dedup_by(|a, b| a.view_name == b.view_name);
    for gate in sorted {
        writeln!(
            s,
            "  {}: {{ resource: {}, state: {}, resume: {} }},",
            ts_string(&gate.view_name),
            ts_string(&gate.resource),
            ts_string(&gate.expected_state),
            ts_string(&gate.resume_name)
        )
        .ok();
    }
    writeln!(s, "}} as const;").ok();
    s
}

fn collect_view_paths(root: &Value) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    for route in array_field(root, "routes") {
        let Some(path) = string_field(route, "path") else {
            continue;
        };
        if let Some(name) = string_field(route, "name") {
            out.insert((route_name_feature(name), name.to_owned()), path.to_owned());
        }
        if let Some((feature, view)) = string_field(route, "to").and_then(parse_to_view) {
            out.insert((feature, view), path.to_owned());
        }
    }
    for feature in features(root) {
        let feature_name = string_field(feature, "name").unwrap_or("app");
        for surface in array_field(feature, "surfaces") {
            for audience in array_field(surface, "audiences") {
                for view in array_field(audience, "views") {
                    if let (Some(name), Some(path)) =
                        (string_field(view, "name"), string_field(view, "route"))
                    {
                        out.insert((feature_name.to_owned(), name.to_owned()), path.to_owned());
                    }
                }
            }
        }
    }
    out
}

fn parse_resume_arms(
    resume: &Value,
    feature: &str,
    view_paths: &BTreeMap<(String, String), String>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(map) = resume.get("arms").and_then(Value::as_object) {
        for (state, target) in map {
            if let Some(path) = arm_target_path(target, feature, view_paths) {
                out.insert(state.clone(), path);
            }
        }
    }
    for arm in array_field(resume, "arms") {
        let state = string_field(arm, "state")
            .or_else(|| string_field(arm, "name"))
            .or_else(|| string_field(arm, "arm"));
        let Some(state) = state else {
            continue;
        };
        if let Some(path) = arm_target_path(arm, feature, view_paths) {
            out.insert(state.to_owned(), path);
        }
    }
    for key in ["none", "wildcard", "*"] {
        if let Some(value) = resume.get(key) {
            let state = if key == "wildcard" { "*" } else { key };
            if let Some(path) = arm_target_path(value, feature, view_paths) {
                out.insert(state.to_owned(), path);
            }
        }
    }
    out
}

fn arm_target_path(
    value: &Value,
    feature: &str,
    view_paths: &BTreeMap<(String, String), String>,
) -> Option<String> {
    if let Some(path) = string_field(value, "path").or_else(|| value.as_str()) {
        if path.starts_with('/') {
            return Some(path.to_owned());
        }
    }
    let view = string_field(value, "view")
        .or_else(|| string_field(value, "target_view"))
        .or_else(|| string_field(value, "target"))
        .or_else(|| value.as_str())?;
    let view = view
        .trim()
        .strip_prefix("view ")
        .unwrap_or(view.trim())
        .trim_start_matches("@view.")
        .to_owned();
    let (target_feature, target_view) = parse_to_view(&view).unwrap_or_else(|| {
        let tail = view.rsplit('.').next().unwrap_or(&view).to_owned();
        (feature.to_owned(), tail)
    });
    view_paths
        .get(&(target_feature, target_view.clone()))
        .cloned()
        .or_else(|| Some(format!("/{}", target_view.replace('_', "-"))))
}

fn extract_gate_from_holder(
    holder: &Value,
    default_feature: &str,
    app_resume: Option<&ResumeRef>,
) -> Option<(String, String, ResumeRef)> {
    holder
        .get("resolved_lifecycle_gate")
        .and_then(|gate| extract_resolved_gate(gate, default_feature))
        .or_else(|| {
            holder
                .get("resolvedLifecycleGate")
                .and_then(|gate| extract_resolved_gate(gate, default_feature))
        })
        .or_else(|| {
            holder
                .get("guard")
                .and_then(|guard| extract_gate_from_guard(guard, default_feature, app_resume))
        })
}

fn extract_resolved_gate(
    gate: &Value,
    default_feature: &str,
) -> Option<(String, String, ResumeRef)> {
    let resource = string_field(gate, "resource")?.to_owned();
    let expected_state = string_field(gate, "expected_state")
        .or_else(|| string_field(gate, "state"))
        .or_else(|| string_field(gate, "expectedState"))?
        .to_owned();
    let resume = string_field(gate, "resume_router")
        .or_else(|| string_field(gate, "resume"))
        .or_else(|| string_field(gate, "resumeRouter"))?;
    Some((
        resource,
        expected_state,
        parse_resume_ref(default_feature, resume),
    ))
}

fn extract_gate_from_guard(
    guard: &Value,
    default_feature: &str,
    app_resume: Option<&ResumeRef>,
) -> Option<(String, String, ResumeRef)> {
    let requires = guard
        .get("requires_lifecycle")
        .or_else(|| guard.get("requiresLifecycle"))?;
    let (resource, expected_state) = parse_requires_lifecycle(requires)?;
    let resume = resume_ref_from_guard(guard, default_feature).or_else(|| app_resume.cloned())?;
    Some((resource, expected_state, resume))
}

fn parse_requires_lifecycle(value: &Value) -> Option<(String, String)> {
    if let Some(raw) = value.as_str() {
        let (resource, state) = raw.split_once('=')?;
        return Some((resource.trim().to_owned(), state.trim().to_owned()));
    }
    Some((
        string_field(value, "resource")?.to_owned(),
        string_field(value, "state")
            .or_else(|| string_field(value, "expected_state"))
            .or_else(|| string_field(value, "expectedState"))?
            .to_owned(),
    ))
}

fn resume_ref_from_guard(guard: &Value, default_feature: &str) -> Option<ResumeRef> {
    let raw = string_field(guard, "on_lifecycle_pending")
        .or_else(|| string_field(guard, "onLifecyclePending"))?;
    Some(parse_resume_ref(default_feature, raw))
}

fn parse_resume_ref(default_feature: &str, raw: &str) -> ResumeRef {
    let mut value = raw.trim();
    if let Some(rest) = value.strip_prefix("@resume ") {
        value = rest.trim();
    }
    if let Some(rest) = value.strip_prefix("@resume.") {
        value = rest.trim();
    }
    if let Some(rest) = value.strip_prefix("@resume") {
        value = rest.trim();
    }
    let value = value.trim_start_matches('.');
    if let Some((feature, name)) = value.split_once('.') {
        ResumeRef {
            feature: feature.to_owned(),
            name: name.to_owned(),
        }
    } else {
        ResumeRef {
            feature: default_feature.to_owned(),
            name: value.to_owned(),
        }
    }
}

fn extract_guard_shape(holder: &Value) -> Option<RouteGuardShape> {
    let guard = holder.get("guard").unwrap_or(holder);
    let mut out = RouteGuardShape::default();
    match guard.get("policy") {
        Some(Value::String(name)) => out.name = Some(name.clone()),
        Some(Value::Object(policy)) => {
            out.name = policy
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned);
            out.atoms = policy
                .get("atoms")
                .and_then(Value::as_array)
                .map(|atoms| atoms.iter().filter_map(parse_policy_atom).collect())
                .unwrap_or_default();
        }
        _ => {}
    }
    if out.atoms.is_empty() {
        out.atoms = holder
            .get("resolved_guard_policy")
            .or_else(|| holder.get("resolvedPolicy"))
            .and_then(Value::as_array)
            .map(|atoms| atoms.iter().filter_map(parse_policy_atom).collect())
            .unwrap_or_default();
    }
    out.on_unauthenticated = string_field(guard, "on_unauthenticated")
        .or_else(|| string_field(guard, "onUnauthenticated"))
        .map(str::to_owned);
    out.on_unauthorized = string_field(guard, "on_unauthorized")
        .or_else(|| string_field(guard, "onUnauthorized"))
        .map(str::to_owned);
    (out.name.is_some()
        || !out.atoms.is_empty()
        || out.on_unauthenticated.is_some()
        || out.on_unauthorized.is_some())
    .then_some(out)
}

fn parse_policy_atom(value: &Value) -> Option<PolicyAtom> {
    if let Some(raw) = value.as_str() {
        let raw = raw.trim().trim_start_matches('@');
        let (namespace, name) = raw.split_once('.')?;
        return Some(PolicyAtom {
            namespace: namespace.to_owned(),
            name: name.to_owned(),
        });
    }
    Some(PolicyAtom {
        namespace: string_field(value, "namespace")?.to_owned(),
        name: string_field(value, "name")?.to_owned(),
    })
}

fn resume_entries(feature: &Value) -> Vec<(String, &Value)> {
    for key in ["resume_routers", "resumeRouters", "resumes"] {
        if let Some(value) = feature.get(key) {
            if let Some(items) = value.as_array() {
                return items
                    .iter()
                    .filter_map(|item| {
                        string_field(item, "name").map(|name| (name.to_owned(), item))
                    })
                    .collect();
            }
            if let Some(map) = value.as_object() {
                return map
                    .iter()
                    .map(|(name, item)| (name.clone(), item))
                    .collect();
            }
        }
    }
    Vec::new()
}

fn parse_source_query(resume: &Value, default_feature: &str) -> Option<(String, String)> {
    for key in [
        "source",
        "source_query",
        "resume_source_query",
        "resumeSourceQuery",
    ] {
        let Some(value) = resume.get(key) else {
            continue;
        };
        if let Some(raw) = value.as_str() {
            return parse_query_ref(raw, default_feature);
        }
        let feature = string_field(value, "feature").unwrap_or(default_feature);
        let name = string_field(value, "name")
            .or_else(|| string_field(value, "query"))
            .or_else(|| string_field(value, "query_name"))?;
        return Some((feature.to_owned(), name.to_owned()));
    }
    None
}

fn parse_query_ref(raw: &str, default_feature: &str) -> Option<(String, String)> {
    let cleaned = raw
        .trim()
        .strip_prefix("source query.lookup ")
        .unwrap_or(raw.trim())
        .trim()
        .strip_prefix("query.lookup ")
        .unwrap_or(raw.trim())
        .trim();
    let cleaned = cleaned.trim_start_matches('@');
    let parts: Vec<&str> = cleaned.split('.').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [name] => Some((default_feature.to_owned(), (*name).to_owned())),
        [feature, "query", name] => Some(((*feature).to_owned(), (*name).to_owned())),
        [feature, name] => Some(((*feature).to_owned(), (*name).to_owned())),
        _ => None,
    }
}

fn features(root: &Value) -> Vec<&Value> {
    array_field(root, "features")
}

fn array_field<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn lookup_resource(
    resources: &BTreeMap<(String, String), ResourceLifecycle>,
    feature: &str,
    resource: &str,
) -> Option<ResourceLifecycle> {
    resources
        .get(&(feature.to_owned(), canonical(resource)))
        .cloned()
}

fn lookup_resource_by_name(
    resources: &BTreeMap<(String, String), ResourceLifecycle>,
    resource: &str,
) -> Option<ResourceLifecycle> {
    let key = canonical(resource);
    resources
        .iter()
        .find(|((_, name), _)| name == &key)
        .map(|(_, value)| value.clone())
}

fn only_resource_for_feature<'a>(
    resources: &'a BTreeMap<(String, String), ResourceLifecycle>,
    feature: &str,
) -> Option<&'a str> {
    let mut matches = resources
        .values()
        .filter(|resource| resource.feature == feature)
        .map(|resource| resource.name.as_str());
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn pick_resource_for_query<'a>(
    query_name: &str,
    resources: &[&'a ResourceLifecycle],
) -> Option<&'a ResourceLifecycle> {
    let q = canonical(query_name);
    resources
        .iter()
        .copied()
        .find(|resource| q.contains(&canonical(&resource.name)))
        .or_else(|| resources.first().copied())
}

fn surface_platform(surface: &Value) -> Option<&str> {
    string_field(surface, "target")
        .or_else(|| string_field(surface, "platform"))
        .and_then(surface_platform_label)
}

fn surface_platform_label(raw: &str) -> Option<&str> {
    let lc = raw.to_ascii_lowercase();
    if lc.contains("mobile") {
        Some("mobile")
    } else if lc.contains("web") || lc.contains("vite") || lc.contains("tanstack") {
        Some("web")
    } else {
        None
    }
}

fn surface_feature(surface: &str) -> Option<String> {
    surface
        .split([' ', '.'])
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn route_name_feature(name: &str) -> String {
    name.split(['_', '-'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("app")
        .to_owned()
}

fn parse_to_view(raw: &str) -> Option<(String, String)> {
    let target = raw.split('(').next()?.trim();
    let (feature, rest) = target.split_once(".view.")?;
    let view = rest.split('.').next()?.trim();
    Some((feature.to_owned(), view.to_owned()))
}

fn query_ident(feature: &str, query_name: &str) -> String {
    let stripped = query_name.strip_prefix("by_").unwrap_or(query_name);
    format!("lookup{}By{}", pascal_case(feature), pascal_case(stripped))
}

fn sdk_import_path(current_feature: &str, source_feature: &str) -> String {
    if current_feature == source_feature {
        format!("./{}.gen.js", current_feature)
    } else {
        format!("../{}/{}.gen.js", source_feature, source_feature)
    }
}

fn route_const_name(name: &str) -> String {
    format!("{}Route", lower_camel(name))
}

fn ts_string(value: &str) -> String {
    serde_json::to_string(value).expect("string literal serializes")
}

fn canonical(value: &str) -> String {
    value
        .chars()
        .filter(|ch| *ch != '_' && *ch != '-' && !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn pascal_case(value: &str) -> String {
    let mut out = String::new();
    for word in value.split(|ch: char| ch == '_' || ch == '-' || ch == ' ') {
        if word.is_empty() {
            continue;
        }
        if is_acronym(word) {
            out.push_str(&word.to_ascii_uppercase());
            continue;
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            for ch in first.to_uppercase() {
                out.push(ch);
            }
        }
        out.push_str(&chars.as_str().to_ascii_lowercase());
    }
    if out.is_empty() {
        let mut chars = value.chars();
        if let Some(first) = chars.next() {
            for ch in first.to_uppercase() {
                out.push(ch);
            }
            out.push_str(chars.as_str());
        }
    }
    out
}

fn lower_camel(value: &str) -> String {
    let pascal = pascal_case(value);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::new();
            for ch in first.to_lowercase() {
                out.push(ch);
            }
            out.push_str(chars.as_str());
            out
        }
        None => String::new(),
    }
}

fn is_acronym(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "id" | "url" | "uri" | "api" | "html" | "json" | "sql" | "ttl"
    )
}
