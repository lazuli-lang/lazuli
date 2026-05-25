//! IR-JSON → typed `ResourceLifecycle` / `ResumeRouter` /
//! `LifecycleGate` collectors.
//!
//! Each `collect_*` walks the serialized IR module once and produces
//! a fully-typed projection that downstream emission (`emit_group_file`,
//! `emit_registry_file`) consumes directly.
//!
//! ## Layout
//!
//! * `collect_lifecycle_resources` — per-feature
//!   `resource.lifecycle { states, generated_enum, discriminator_field }`.
//! * `collect_query_resources` — `query.resource` / inferred mapping.
//! * `collect_resume_routers` — `resume_router { source, arms,
//!   none, wildcard }` walks (multiple field-name aliases).
//! * `collect_lifecycle_gates` — per-view / per-route gates discovered
//!   under `surface.audience.view`, top-level `routes`, and inherited
//!   from `audience` / `app.route_guard`.
//! * `push_gate` — dedup helper keyed by
//!   `(feature, platform, audience, route_const)`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::helpers::{
    array_field, canonical, features, lookup_resource, lookup_resource_by_name,
    only_resource_for_feature, parse_to_view, pascal_case, pick_resource_for_query, query_ident,
    route_const_name, route_name_feature, string_field, surface_feature, surface_platform,
    surface_platform_label,
};
use super::parse_ir::{
    extract_gate_from_guard, extract_gate_from_holder, extract_guard_shape, parse_resume_arms,
    parse_source_query, resume_entries, resume_ref_from_guard,
};
use super::{LifecycleGate, LifecycleGateTarget, ResourceLifecycle, ResumeRouter};

pub(super) fn collect_lifecycle_resources(
    root: &Value,
) -> BTreeMap<(String, String), ResourceLifecycle> {
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

pub(super) fn collect_query_resources(
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

pub(super) fn collect_resume_routers(
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

pub(super) fn collect_lifecycle_gates(
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
                    let Some((resource, expected_state, expected_substep, resume_ref)) = gate
                    else {
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
                            expected_substep,
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
                let Some((resource, expected_state, expected_substep, resume_ref)) = gate else {
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
                        expected_substep,
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
        let Some((resource, expected_state, expected_substep, resume_ref)) = gate else {
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
                expected_substep,
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
