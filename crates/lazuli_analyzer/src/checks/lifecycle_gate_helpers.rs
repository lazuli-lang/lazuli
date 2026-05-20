use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{AppManifest, ExperienceModule, Feature, Query, Resource, SpanRef};
use serde_json::{Value, json};

use super::{
    LifecycleGateDiagnostic, LifecycleGateInput, LifecycleGateOrigin, LifecycleGateResume,
    LifecycleGateResumeArm, LifecycleGateResumeSource, LifecycleGateSeverity, LifecycleGateView,
    RequiresLifecycle,
};

struct Index<'a> {
    features: BTreeMap<&'a str, &'a Feature>,
    resources: BTreeMap<(&'a str, &'a str), &'a Resource>,
    queries: BTreeMap<(&'a str, &'a str), &'a Query>,
    resumes: BTreeMap<(String, String), LifecycleGateResume>,
}

struct QueryHit<'a> {
    feature: String,
    query: &'a Query,
    resource: Option<&'a Resource>,
}

pub fn check(
    module: &mut ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
) -> Vec<LifecycleGateDiagnostic> {
    let input = input_from_json(module, app, features);
    let diagnostics = check_input(&input, features);
    cache_resolved_json(module, app, features, &input, &diagnostics);
    diagnostics
}

pub fn check_input(
    input: &LifecycleGateInput,
    features: &[Feature],
) -> Vec<LifecycleGateDiagnostic> {
    let index = Index::new(features, input);
    let mut out = Vec::new();
    check_views(input, &index, &mut out);
    check_resumes(&index, &mut out);
    check_cycles(input, &index, &mut out);
    dedupe(&mut out);
    out
}

impl<'a> Index<'a> {
    fn new(features: &'a [Feature], input: &LifecycleGateInput) -> Self {
        let mut this = Self {
            features: BTreeMap::new(),
            resources: BTreeMap::new(),
            queries: BTreeMap::new(),
            resumes: BTreeMap::new(),
        };
        for feature in features {
            this.features.insert(&feature.name, feature);
            for resource in &feature.resources {
                this.resources
                    .insert((&feature.name, &resource.name), resource);
            }
            for query in &feature.queries {
                this.queries.insert((&feature.name, query.name()), query);
            }
        }
        for resume in &input.resumes {
            this.resumes.insert(
                (resume.feature.clone(), resume.name.clone()),
                resume.clone(),
            );
        }
        this
    }

    fn reachable(&self, feature: &str) -> Vec<&'a Feature> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![feature.to_owned()];
        while let Some(name) = stack.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            let Some(feature) = self.features.get(name.as_str()).copied() else {
                continue;
            };
            stack.extend(feature.uses.iter().cloned());
            out.push(feature);
        }
        out
    }

    fn resource(&self, feature: &str, name: &str) -> Option<(&'a str, &'a Resource)> {
        for reachable in self.reachable(feature) {
            if let Some(resource) = self
                .resources
                .get(&(reachable.name.as_str(), name))
                .copied()
            {
                return Some((reachable.name.as_str(), resource));
            }
        }
        None
    }

    fn declared_resources(&self, feature: &str) -> Vec<String> {
        self.reachable(feature)
            .into_iter()
            .flat_map(|feature| feature.resources.iter().map(|r| r.name.clone()))
            .collect()
    }

    fn resume(&self, feature: &str, resume_ref: &str) -> Option<&LifecycleGateResume> {
        let (resume_feature, name) = parse_resume_ref(feature, resume_ref)?;
        self.resumes.get(&(resume_feature, name))
    }

    fn source_query(&self, resume: &LifecycleGateResume) -> Option<QueryHit<'a>> {
        let source = resume.source.as_ref()?;
        let feature = source.feature.as_deref().unwrap_or(&resume.feature);
        let query = self
            .queries
            .get(&(feature, source.query.as_str()))
            .copied()?;
        Some(QueryHit {
            feature: feature.to_owned(),
            query,
            resource: self.query_resource(feature, query),
        })
    }

    fn query_resource(&self, feature: &str, query: &'a Query) -> Option<&'a Resource> {
        let q_start = query_span(query).map(|s| s.start)?;
        let resources = &self.features.get(feature)?.resources;
        // Candidates: resources declared before the query. We prefer the
        // resource WITH a `lifecycle` block when present, because a feature
        // with one stateful entity + N supporting sub-resources (notes,
        // assignments, history) reads `query.lookup by_id` as the lookup
        // of the principal entity — the one whose lifecycle gate views
        // would target. Falling back to span-locality picks the wrong
        // resource (the last one declared, often a sub-entity) in those
        // shapes. Single-resource features keep the original behavior
        // through the final fallback.
        let candidates: Vec<&'a Resource> = resources
            .iter()
            .filter(|r| r.span_ref.is_some_and(|s| s.start <= q_start))
            .collect();
        if let Some(with_lifecycle) = candidates.iter().rev().find(|r| r.lifecycle.is_some()) {
            return Some(*with_lifecycle);
        }
        candidates
            .into_iter()
            .max_by_key(|r| r.span_ref.map(|s| s.start).unwrap_or(0))
            .or_else(|| (resources.len() == 1).then_some(&resources[0]))
    }
}

fn check_views(
    input: &LifecycleGateInput,
    index: &Index<'_>,
    out: &mut Vec<LifecycleGateDiagnostic>,
) {
    for view in &input.views {
        let Some(req) = view.requires.as_ref() else {
            continue;
        };
        let Some((_, resource)) = index.resource(&view.feature, &req.resource) else {
            out.push(diag(
                "LIFECYCLE-GATE-001",
                LifecycleGateSeverity::Error,
                req.span.or(view.span),
                format!(
                    "view `{}` declares `requires_lifecycle {} = {}`, but resource `{}` is not declared in any reachable feature. Declared resources: {}.",
                    view.name,
                    req.resource,
                    req.state,
                    req.resource,
                    list_or_none(index.declared_resources(&view.feature))
                ),
            ));
            continue;
        };
        let states = lifecycle_states(resource);
        if states.is_empty() {
            out.push(diag(
                "LIFECYCLE-GATE-002",
                LifecycleGateSeverity::Error,
                req.span.or(view.span),
                format!(
                    "view `{}` declares `requires_lifecycle {} = {}`, but resource `{}` does not declare a lifecycle status block.",
                    view.name, req.resource, req.state, req.resource
                ),
            ));
        } else if !states.contains(&req.state) {
            out.push(diag(
                "LIFECYCLE-GATE-002",
                LifecycleGateSeverity::Error,
                req.span.or(view.span),
                format!(
                    "view `{}` declares `requires_lifecycle {} = {}`, but `{}` is not declared in `lifecycle status of {}`. Declared states: {}.",
                    view.name,
                    req.resource,
                    req.state,
                    req.state,
                    req.resource,
                    states.join(", ")
                ),
            ));
        }
        if !view.policy_present {
            out.push(diag(
                "LIFECYCLE-GATE-009",
                LifecycleGateSeverity::Warning,
                req.span.or(view.span),
                format!(
                    "view `{}` declares `requires_lifecycle {}` but has no actor `policy` gate.",
                    view.name, req.resource
                ),
            ));
        }
        let Some(resume_ref) = view
            .on_lifecycle_pending
            .as_deref()
            .or(input.app_on_lifecycle_pending.as_deref())
        else {
            continue;
        };
        let Some(resume) = index.resume(&view.feature, resume_ref) else {
            continue;
        };
        let Some(hit) = valid_lookup_source(index, resume, out) else {
            continue;
        };
        if let Some(source_resource) = hit.resource {
            if source_resource.name != req.resource {
                out.push(diag(
                    "LIFECYCLE-GATE-007",
                    LifecycleGateSeverity::Error,
                    req.span.or(view.span),
                    format!(
                        "view `{}` declares `requires_lifecycle {}` but `on_lifecycle_pending {}` resolves to resume `{}`, whose source query returns `{}`.",
                        view.name, req.resource, resume_ref, resume.name, source_resource.name
                    ),
                ));
            }
        }
    }
}

fn check_resumes(index: &Index<'_>, out: &mut Vec<LifecycleGateDiagnostic>) {
    for resume in index.resumes.values() {
        let Some(hit) = valid_lookup_source(index, resume, out) else {
            continue;
        };
        let Some(resource) = hit.resource else {
            continue;
        };
        let states = lifecycle_states(resource);
        if states.is_empty() {
            continue;
        }
        let explicit: BTreeSet<_> = resume
            .arms
            .iter()
            .filter(|arm| arm.state != "*" && arm.state != "none")
            .map(|arm| arm.state.clone())
            .collect();
        let wildcard = resume.arms.iter().find(|arm| arm.state == "*");
        let missing: Vec<_> = states
            .iter()
            .filter(|state| !explicit.contains(*state))
            .cloned()
            .collect();
        if !missing.is_empty() && wildcard.is_none() {
            out.push(diag(
                "LIFECYCLE-GATE-003",
                LifecycleGateSeverity::Error,
                resume.span,
                format!(
                    "resume `{}` does not cover states of `{}`: {}. Add explicit arms or a wildcard `*` arm.",
                    resume.name,
                    resource.name,
                    missing.join(", ")
                ),
            ));
        }
        let extra: Vec<_> = explicit
            .iter()
            .filter(|state| !states.contains(*state))
            .cloned()
            .collect();
        if !extra.is_empty() {
            out.push(diag(
                "LIFECYCLE-GATE-004",
                LifecycleGateSeverity::Warning,
                resume.span,
                format!(
                    "resume `{}` has arms for states not declared in `{}`: {}. Declared states: {}.",
                    resume.name,
                    resource.name,
                    extra.join(", "),
                    states.join(", ")
                ),
            ));
        }
        if let Some(wildcard) = wildcard.filter(|_| missing.is_empty()) {
            out.push(diag(
                "LIFECYCLE-GATE-005",
                LifecycleGateSeverity::Info,
                wildcard.span.or(resume.span),
                format!(
                    "resume `{}` has wildcard `*` but every declared state is already explicitly covered.",
                    resume.name
                ),
            ));
        }
    }
}

fn valid_lookup_source<'a>(
    index: &'a Index<'a>,
    resume: &LifecycleGateResume,
    out: &mut Vec<LifecycleGateDiagnostic>,
) -> Option<QueryHit<'a>> {
    let source = resume.source.as_ref()?;
    let feature = source.feature.as_deref().unwrap_or(&resume.feature);
    let Some(query) = index
        .queries
        .get(&(feature, source.query.as_str()))
        .copied()
    else {
        out.push(source_diag(
            resume,
            source,
            "does not resolve to a declared query",
        ));
        return None;
    };
    let actual_kind = query_kind(query);
    if source.kind.as_deref() != Some("lookup") || actual_kind != "lookup" {
        out.push(source_diag(
            resume,
            source,
            &format!(
                "is declared as `query.{actual_kind}`, but a resume source must be `query.lookup`"
            ),
        ));
        return None;
    }
    Some(QueryHit {
        feature: feature.to_owned(),
        query,
        resource: index.query_resource(feature, query),
    })
}

fn source_diag(
    resume: &LifecycleGateResume,
    source: &LifecycleGateResumeSource,
    reason: &str,
) -> LifecycleGateDiagnostic {
    diag(
        "LIFECYCLE-GATE-008",
        LifecycleGateSeverity::Error,
        source.span.or(resume.span),
        format!(
            "resume `{}` declares source `{}`, but it {reason}.",
            resume.name, source.text
        ),
    )
}

fn check_cycles(
    input: &LifecycleGateInput,
    index: &Index<'_>,
    out: &mut Vec<LifecycleGateDiagnostic>,
) {
    let views: BTreeMap<_, _> = input
        .views
        .iter()
        .map(|view| ((view.feature.as_str(), view.name.as_str()), view))
        .collect();
    let mut seen = BTreeSet::new();
    for view in &input.views {
        let Some(req) = view.requires.as_ref() else {
            continue;
        };
        if !valid_requires(index, &view.feature, req) {
            continue;
        }
        let Some(resume_ref) = view
            .on_lifecycle_pending
            .as_deref()
            .or(input.app_on_lifecycle_pending.as_deref())
        else {
            continue;
        };
        let Some(resume) = index.resume(&view.feature, resume_ref) else {
            continue;
        };
        for arm in &resume.arms {
            if matches!(arm.state.as_str(), "*" | "none") || arm.state == req.state {
                continue;
            }
            let mut path = vec![format!("{}.{}", view.feature, view.name)];
            if walk_cycle(
                index,
                &views,
                &view.feature,
                &req.resource,
                &arm.state,
                arm,
                &mut path,
            ) {
                let path_text = path.join(" -> ");
                let key = format!(
                    "{}:{}:{}",
                    req.resource,
                    arm.state,
                    path.last().cloned().unwrap_or_default()
                );
                if seen.insert(key) {
                    out.push(diag(
                        "LIFECYCLE-GATE-006",
                        LifecycleGateSeverity::Error,
                        arm.span.or(resume.span),
                        format!(
                            "resume `{}` arm `{}` creates a redirect cycle: {}.",
                            resume.name, arm.state, path_text
                        ),
                    ));
                }
            }
        }
    }
}

fn walk_cycle(
    index: &Index<'_>,
    views: &BTreeMap<(&str, &str), &LifecycleGateView>,
    default_feature: &str,
    resource: &str,
    state: &str,
    arm: &LifecycleGateResumeArm,
    path: &mut Vec<String>,
) -> bool {
    let (feature, view_name) = parse_view_ref(default_feature, &arm.target_view);
    let key = format!("{feature}.{view_name}");
    if path.contains(&key) {
        path.push(key);
        return true;
    }
    let Some(target) = views.get(&(feature.as_str(), view_name.as_str())).copied() else {
        return false;
    };
    let Some(req) = target.requires.as_ref() else {
        return false;
    };
    if !valid_requires(index, &target.feature, req) {
        return false;
    }
    if req.resource != resource || req.state == state {
        return false;
    }
    let Some(resume_ref) = target.on_lifecycle_pending.as_deref() else {
        return false;
    };
    let Some(resume) = index.resume(&target.feature, resume_ref) else {
        return false;
    };
    let Some(next) = resume
        .arms
        .iter()
        .find(|candidate| candidate.state == state)
        .or_else(|| resume.arms.iter().find(|candidate| candidate.state == "*"))
    else {
        return false;
    };
    path.push(key);
    walk_cycle(index, views, &target.feature, resource, state, next, path)
}

fn input_from_json(
    module: &ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
) -> LifecycleGateInput {
    let mut input = LifecycleGateInput::default();
    input.app_on_lifecycle_pending =
        app.and_then(|app| serde_json::to_value(app).ok())
            .and_then(|value| {
                value
                    .pointer("/route_guard/on_lifecycle_pending")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
    for feature in features {
        if let Ok(value) = serde_json::to_value(feature) {
            if let Some(resumes) = value.get("resume_routers").and_then(Value::as_array) {
                input.resumes.extend(
                    resumes
                        .iter()
                        .filter_map(|r| resume_from_json(&feature.name, r)),
                );
            }
        }
    }
    if let Ok(value) = serde_json::to_value(module) {
        let mut policies = BTreeMap::new();
        collect_surface_overrides(&value, &mut policies);
        if let Some(experiences) = value.get("experiences").and_then(Value::as_array) {
            for experience in experiences {
                let feature = experience
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                for view in experience
                    .get("views")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(mut parsed) = view_from_json(feature, view) {
                        if let Some((policy, pending)) =
                            policies.get(&(feature.to_owned(), parsed.name.clone()))
                        {
                            parsed.policy_present |= *policy;
                            if parsed.on_lifecycle_pending.is_none() {
                                parsed.on_lifecycle_pending = pending.clone();
                            }
                        }
                        input.views.push(parsed);
                    }
                }
            }
        }
    }
    input
}

fn cache_resolved_json(
    module: &mut ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
    input: &LifecycleGateInput,
    diagnostics: &[LifecycleGateDiagnostic],
) {
    if diagnostics
        .iter()
        .any(|d| d.severity == LifecycleGateSeverity::Error)
    {
        return;
    }
    let index = Index::new(features, input);
    let Ok(mut value) = serde_json::to_value(&*module) else {
        return;
    };
    let app_pending = input.app_on_lifecycle_pending.as_deref();
    if let Some(experiences) = value.get_mut("experiences").and_then(Value::as_array_mut) {
        for experience in experiences {
            let feature = experience
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if let Some(views) = experience.get_mut("views").and_then(Value::as_array_mut) {
                for view_value in views {
                    let Some(view) = view_from_json(&feature, view_value) else {
                        continue;
                    };
                    let Some(req) = view.requires.as_ref() else {
                        continue;
                    };
                    let Some(resume_ref) = view.on_lifecycle_pending.as_deref().or(app_pending)
                    else {
                        continue;
                    };
                    let Some(resume) = index.resume(&feature, resume_ref) else {
                        continue;
                    };
                    let Some(hit) = index.source_query(resume) else {
                        continue;
                    };
                    if let Some(obj) = view_value.as_object_mut() {
                        obj.insert("resolved_lifecycle_gate".to_owned(), json!({
                            "resource": req.resource,
                            "expected_state": req.state,
                            "resume_router": format!("{}.{}", resume.feature, resume.name),
                            "resume_source_query": format!("{}.{}", hit.feature, hit.query.name()),
                            "resume_source_layer": if view.on_lifecycle_pending.is_some() { "view" } else { "app" },
                            "gate_source_layer": "view",
                            "span_ref": req.span,
                        }));
                    }
                }
            }
        }
    }
    if let Ok(updated) = serde_json::from_value(value) {
        *module = updated;
    }
    let _ = app;
}

fn collect_surface_overrides(
    value: &Value,
    out: &mut BTreeMap<(String, String), (bool, Option<String>)>,
) {
    for surface in value
        .get("surfaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let feature = surface
            .get("uses_experience")
            .and_then(Value::as_str)
            .or_else(|| surface.get("experience").and_then(Value::as_str))
            .unwrap_or_default();
        for audience in surface
            .get("audiences")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let audience_policy = guard_policy(audience.get("guard"));
            let audience_pending = pending(audience.get("guard"));
            for view in audience
                .get("views")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let name = view.get("name").and_then(Value::as_str).unwrap_or_default();
                let policy = guard_policy(view.get("guard")) || audience_policy;
                let pending = pending(view.get("guard")).or_else(|| audience_pending.clone());
                out.insert((feature.to_owned(), name.to_owned()), (policy, pending));
            }
        }
    }
}

fn resume_from_json(default_feature: &str, value: &Value) -> Option<LifecycleGateResume> {
    let name = value.get("name").and_then(Value::as_str)?.to_owned();
    let feature = value
        .get("feature")
        .and_then(Value::as_str)
        .unwrap_or(default_feature)
        .to_owned();
    let source = value
        .get("source")
        .and_then(|source| source_from_json(&feature, source));
    let arms = value
        .get("arms")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(arm_from_json)
        .collect();
    Some(LifecycleGateResume {
        feature,
        name,
        source,
        arms,
        span: span(value),
    })
}

fn source_from_json(default_feature: &str, value: &Value) -> Option<LifecycleGateResumeSource> {
    if let Some(text) = value.as_str() {
        return parse_source(default_feature, text, None);
    }
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let query = value
        .get("query")
        .and_then(Value::as_str)
        .or_else(|| value.get("name").and_then(Value::as_str))?;
    Some(LifecycleGateResumeSource {
        feature: value
            .get("feature")
            .and_then(Value::as_str)
            .map(str::to_owned),
        kind: value
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| Some("lookup".to_owned())),
        query: query.to_owned(),
        text: if text.is_empty() {
            format!("query.lookup {query}")
        } else {
            text.to_owned()
        },
        span: span(value),
    })
}

fn arm_from_json(value: &Value) -> Option<LifecycleGateResumeArm> {
    Some(LifecycleGateResumeArm {
        state: value.get("state").and_then(Value::as_str)?.to_owned(),
        target_view: value
            .get("target_view")
            .or_else(|| value.get("view"))
            .and_then(Value::as_str)?
            .to_owned(),
        span: span(value),
    })
}

fn view_from_json(feature: &str, value: &Value) -> Option<LifecycleGateView> {
    let name = value.get("name").and_then(Value::as_str)?.to_owned();
    let guard = value.get("guard");
    Some(LifecycleGateView {
        feature: feature.to_owned(),
        name,
        policy_present: guard_policy(guard),
        requires: guard
            .and_then(|g| g.get("requires_lifecycle"))
            .and_then(requires_from_json),
        on_lifecycle_pending: pending(guard),
        span: span(value),
    })
}

fn requires_from_json(value: &Value) -> Option<RequiresLifecycle> {
    if let Some(text) = value.as_str() {
        let (resource, state) = parse_requires(text)?;
        return Some(RequiresLifecycle {
            resource,
            state,
            span: None,
        });
    }
    Some(RequiresLifecycle {
        resource: value.get("resource").and_then(Value::as_str)?.to_owned(),
        state: value
            .get("state")
            .or_else(|| value.get("expected_state"))
            .and_then(Value::as_str)?
            .to_owned(),
        span: span(value),
    })
}

fn guard_policy(guard: Option<&Value>) -> bool {
    guard
        .and_then(|g| g.get("policy"))
        .and_then(Value::as_str)
        .map(|p| !p.trim().is_empty())
        .unwrap_or(false)
}

fn pending(guard: Option<&Value>) -> Option<String> {
    guard?
        .get("on_lifecycle_pending")?
        .as_str()
        .map(str::to_owned)
}

fn span(value: &Value) -> Option<SpanRef> {
    let value = value.get("span_ref").unwrap_or(value);
    Some(SpanRef {
        start: value.get("start")?.as_u64()? as usize,
        end: value.get("end")?.as_u64()? as usize,
    })
}

fn parse_source(
    default_feature: &str,
    text: &str,
    span: Option<SpanRef>,
) -> Option<LifecycleGateResumeSource> {
    let raw = text.trim().strip_prefix("source ").unwrap_or(text.trim());
    let parts: Vec<_> = raw.split_whitespace().collect();
    let (head, query) = match parts.as_slice() {
        [head, query] => (*head, *query),
        [single] => (*single, ""),
        _ => return None,
    };
    let dotted: Vec<_> = head.split('.').collect();
    let (feature, kind, query) = match dotted.as_slice() {
        ["query", kind] => (None, Some((*kind).to_owned()), query.to_owned()),
        [feature, "query", kind, name] => (
            Some((*feature).to_owned()),
            Some((*kind).to_owned()),
            (*name).to_owned(),
        ),
        [feature, "query", name] => (
            Some((*feature).to_owned()),
            Some("lookup".to_owned()),
            (*name).to_owned(),
        ),
        [name] if !query.is_empty() => (None, Some("lookup".to_owned()), (*query).to_owned()),
        [name] => (None, Some("lookup".to_owned()), (*name).to_owned()),
        _ => (Some(default_feature.to_owned()), None, query.to_owned()),
    };
    Some(LifecycleGateResumeSource {
        feature,
        kind,
        query,
        text: raw.to_owned(),
        span,
    })
}

fn parse_requires(text: &str) -> Option<(String, String)> {
    let rest = text.trim().strip_prefix("requires_lifecycle")?.trim();
    let (resource, state) = rest.split_once('=')?;
    Some((resource.trim().to_owned(), state.trim().to_owned()))
}

fn parse_resume_ref(default_feature: &str, text: &str) -> Option<(String, String)> {
    let raw = text
        .trim()
        .trim_start_matches("@resume")
        .trim_start_matches(['.', ' ']);
    let parts: Vec<_> = raw.split('.').collect();
    match parts.as_slice() {
        [name] if !name.is_empty() => Some((default_feature.to_owned(), (*name).to_owned())),
        [feature, name] => Some(((*feature).to_owned(), (*name).to_owned())),
        _ => None,
    }
}

fn parse_view_ref(default_feature: &str, text: &str) -> (String, String) {
    let raw = text.trim().strip_prefix("view ").unwrap_or(text.trim());
    let parts: Vec<_> = raw.split('.').collect();
    match parts.as_slice() {
        [feature, "view", name] => ((*feature).to_owned(), (*name).to_owned()),
        [name] => (default_feature.to_owned(), (*name).to_owned()),
        _ => (default_feature.to_owned(), raw.to_owned()),
    }
}

fn lifecycle_states(resource: &Resource) -> Vec<String> {
    resource
        .lifecycle
        .as_ref()
        .map(|l| l.states.iter().map(|s| s.name.clone()).collect())
        .unwrap_or_default()
}

fn valid_requires(index: &Index<'_>, feature: &str, req: &RequiresLifecycle) -> bool {
    index
        .resource(feature, &req.resource)
        .map(|(_, resource)| lifecycle_states(resource).contains(&req.state))
        .unwrap_or(false)
}

fn query_kind(query: &Query) -> &'static str {
    match query {
        Query::List(_) => "list",
        Query::Lookup(_) => "lookup",
        Query::Sql(_) => "sql",
    }
}

fn query_span(query: &Query) -> Option<SpanRef> {
    match query {
        Query::List(q) => q.span_ref,
        Query::Lookup(q) => q.span_ref,
        Query::Sql(q) => q.span_ref,
    }
}

fn list_or_none(values: Vec<String>) -> String {
    if values.is_empty() {
        "<none>".to_owned()
    } else {
        values.join(", ")
    }
}

fn diag(
    code: &'static str,
    severity: LifecycleGateSeverity,
    span: Option<SpanRef>,
    message: String,
) -> LifecycleGateDiagnostic {
    LifecycleGateDiagnostic {
        code,
        severity,
        origin: LifecycleGateOrigin::Lzx,
        span,
        message,
    }
}

fn dedupe(out: &mut Vec<LifecycleGateDiagnostic>) {
    let mut seen = BTreeSet::new();
    out.retain(|d| seen.insert((d.code, d.span.map(|s| (s.start, s.end)), d.message.clone())));
}
