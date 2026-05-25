//! IR-JSON walkers that lift the lifecycle-gate fields into typed
//! `LifecycleGate` / `ResumeRouter` shapes.
//!
//! Because the IR / parser / analyzer cells for LAZ-85/86/87 land in
//! parallel with this emitter, every field probe is defensive: it
//! tries snake_case, camelCase, and the legacy short forms in turn.
//! When the IR shape stabilises, these probes can collapse to the
//! single canonical key.
//!
//! ## Layout
//!
//! * View / route path discovery — `collect_view_paths`.
//! * Resume-router arms / wildcards — `parse_resume_arms`,
//!   `arm_target_path`.
//! * Lifecycle-gate extraction — `extract_gate_from_holder`,
//!   `extract_resolved_gate`, `extract_gate_from_guard`,
//!   `parse_requires_lifecycle`, `parse_state_substep`.
//! * Resume-router references — `resume_ref_from_guard`,
//!   `parse_resume_ref`, `resume_entries`, `parse_source_query`,
//!   `parse_query_ref`.
//! * Guard policy shape — `extract_guard_shape`, `parse_policy_atom`.

use std::collections::BTreeMap;

use serde_json::Value;

use super::helpers::{
    array_field, features, parse_to_view, route_name_feature, string_field,
};
use super::{PolicyAtom, ResumeRef, RouteGuardShape};

pub(super) fn collect_view_paths(root: &Value) -> BTreeMap<(String, String), String> {
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

pub(super) fn parse_resume_arms(
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

pub(super) fn extract_gate_from_holder(
    holder: &Value,
    default_feature: &str,
    app_resume: Option<&ResumeRef>,
) -> Option<(String, String, Option<String>, ResumeRef)> {
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
) -> Option<(String, String, Option<String>, ResumeRef)> {
    let resource = string_field(gate, "resource")?.to_owned();
    let expected_state = string_field(gate, "expected_state")
        .or_else(|| string_field(gate, "state"))
        .or_else(|| string_field(gate, "expectedState"))?
        .to_owned();
    let expected_substep = string_field(gate, "substep")
        .or_else(|| string_field(gate, "expected_substep"))
        .or_else(|| string_field(gate, "expectedSubstep"))
        .map(str::to_owned);
    let resume = string_field(gate, "resume_router")
        .or_else(|| string_field(gate, "resume"))
        .or_else(|| string_field(gate, "resumeRouter"))?;
    Some((
        resource,
        expected_state,
        expected_substep,
        parse_resume_ref(default_feature, resume),
    ))
}

pub(super) fn extract_gate_from_guard(
    guard: &Value,
    default_feature: &str,
    app_resume: Option<&ResumeRef>,
) -> Option<(String, String, Option<String>, ResumeRef)> {
    let requires = guard
        .get("requires_lifecycle")
        .or_else(|| guard.get("requiresLifecycle"))?;
    let (resource, expected_state, expected_substep) = parse_requires_lifecycle(requires)?;
    let resume = resume_ref_from_guard(guard, default_feature).or_else(|| app_resume.cloned())?;
    Some((resource, expected_state, expected_substep, resume))
}

fn parse_requires_lifecycle(value: &Value) -> Option<(String, String, Option<String>)> {
    if let Some(raw) = value.as_str() {
        let (resource, state) = raw.split_once('=')?;
        let (state, substep) = parse_state_substep(state.trim());
        return Some((resource.trim().to_owned(), state, substep));
    }
    let state = string_field(value, "state")
        .or_else(|| string_field(value, "expected_state"))
        .or_else(|| string_field(value, "expectedState"))?
        .to_owned();
    let substep = string_field(value, "substep")
        .or_else(|| string_field(value, "expected_substep"))
        .or_else(|| string_field(value, "expectedSubstep"))
        .map(str::to_owned);
    Some((string_field(value, "resource")?.to_owned(), state, substep))
}

fn parse_state_substep(value: &str) -> (String, Option<String>) {
    let mut parts = value.split_whitespace();
    let state = parts.next().unwrap_or(value).to_owned();
    let substep = match (parts.next(), parts.next(), parts.next()) {
        (Some("substep"), Some(substep), None) => Some(substep.to_owned()),
        _ => None,
    };
    (state, substep)
}

pub(super) fn resume_ref_from_guard(guard: &Value, default_feature: &str) -> Option<ResumeRef> {
    let raw = string_field(guard, "on_lifecycle_pending")
        .or_else(|| string_field(guard, "onLifecyclePending"))?;
    Some(parse_resume_ref(default_feature, raw))
}

pub(super) fn parse_resume_ref(default_feature: &str, raw: &str) -> ResumeRef {
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

pub(super) fn extract_guard_shape(holder: &Value) -> Option<RouteGuardShape> {
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

pub(super) fn resume_entries(feature: &Value) -> Vec<(String, &Value)> {
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

pub(super) fn parse_source_query(resume: &Value, default_feature: &str) -> Option<(String, String)> {
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
