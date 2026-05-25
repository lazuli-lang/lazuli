use std::collections::{BTreeMap, BTreeSet};

use lazuli_ir::{AppManifest, ExperienceModule, Feature, Query, Resource, SpanRef};

use super::{
    LifecycleGateDiagnostic, LifecycleGateInput, LifecycleGateOrigin, LifecycleGateResume,
    LifecycleGateResumeArm, LifecycleGateResumeSource, LifecycleGateSeverity, LifecycleGateView,
    RequiresLifecycle,
};

mod cycles;
mod json;
mod parse;
mod resumes;
mod views;

use cycles::check_cycles;
use json::{cache_resolved_json, input_from_json};
use parse::parse_resume_ref;
use resumes::check_resumes;
use views::check_views;

pub(super) struct Index<'a> {
    pub(super) features: BTreeMap<&'a str, &'a Feature>,
    pub(super) resources: BTreeMap<(&'a str, &'a str), &'a Resource>,
    pub(super) queries: BTreeMap<(&'a str, &'a str), &'a Query>,
    pub(super) resumes: BTreeMap<(String, String), LifecycleGateResume>,
}

pub(super) struct QueryHit<'a> {
    pub(super) feature: String,
    pub(super) query: &'a Query,
    pub(super) resource: Option<&'a Resource>,
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
    pub(super) fn new(features: &'a [Feature], input: &LifecycleGateInput) -> Self {
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

    pub(super) fn reachable(&self, feature: &str) -> Vec<&'a Feature> {
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

    pub(super) fn resource(&self, feature: &str, name: &str) -> Option<(&'a str, &'a Resource)> {
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

    pub(super) fn declared_resources(&self, feature: &str) -> Vec<String> {
        self.reachable(feature)
            .into_iter()
            .flat_map(|feature| feature.resources.iter().map(|r| r.name.clone()))
            .collect()
    }

    pub(super) fn resume(&self, feature: &str, resume_ref: &str) -> Option<&LifecycleGateResume> {
        let (resume_feature, name) = parse_resume_ref(feature, resume_ref)?;
        self.resumes.get(&(resume_feature, name))
    }

    pub(super) fn source_query(&self, resume: &LifecycleGateResume) -> Option<QueryHit<'a>> {
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

    pub(super) fn query_resource(&self, feature: &str, query: &'a Query) -> Option<&'a Resource> {
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


pub(super) fn lifecycle_states(resource: &Resource) -> Vec<String> {
    resource
        .lifecycle
        .as_ref()
        .map(|l| l.states.iter().map(|s| s.name.clone()).collect())
        .unwrap_or_default()
}

pub(super) fn valid_requires(
    index: &Index<'_>,
    feature: &str,
    req: &RequiresLifecycle,
) -> bool {
    index
        .resource(feature, &req.resource)
        .map(|(_, resource)| lifecycle_states(resource).contains(&req.state))
        .unwrap_or(false)
}

pub(super) fn query_kind(query: &Query) -> &'static str {
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

pub(super) fn list_or_none(values: Vec<String>) -> String {
    if values.is_empty() {
        "<none>".to_owned()
    } else {
        values.join(", ")
    }
}

pub(super) fn diag(
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

#[cfg(test)]
mod tests {
    use lazuli_ir::Feature;
    use serde_json::json;

    use super::{
        LifecycleGateInput, LifecycleGateResume, LifecycleGateResumeArm, LifecycleGateResumeSource,
        LifecycleGateView, RequiresLifecycle, check_input,
    };

    fn feature_with_host_lifecycle() -> Feature {
        serde_json::from_value(json!({
            "name": "host",
            "purpose": null,
            "defaults": {},
            "uses": [],
            "enums": [],
            "resources": [
                {
                    "name": "Host",
                    "fields": [],
                    "lifecycle": {
                        "discriminator_field": "lifecycle_state",
                        "generated_enum": "HostLifecycleState",
                        "states": [
                            { "name": "basic_details_pending", "kind": "intermediate" }
                        ],
                        "transitions": []
                    }
                }
            ],
            "events": [],
            "rules": [],
            "policies": { "categories": [], "fields": [] },
            "commands": [],
            "queries": [
                { "kind": "Lookup", "name": "my_host", "keys": [] }
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

    fn input_with_substep(arm_substep: Option<&str>) -> LifecycleGateInput {
        LifecycleGateInput {
            views: vec![LifecycleGateView {
                feature: "host".to_string(),
                name: "phone_verification".to_string(),
                policy_present: true,
                requires: Some(RequiresLifecycle {
                    resource: "Host".to_string(),
                    state: "basic_details_pending".to_string(),
                    substep: Some("phone_verification".to_string()),
                    span: None,
                }),
                on_lifecycle_pending: Some("host_onboarding".to_string()),
                span: None,
            }],
            resumes: vec![LifecycleGateResume {
                feature: "host".to_string(),
                name: "host_onboarding".to_string(),
                source: Some(LifecycleGateResumeSource {
                    feature: Some("host".to_string()),
                    kind: Some("lookup".to_string()),
                    query: "my_host".to_string(),
                    text: "query.lookup my_host".to_string(),
                    span: None,
                }),
                arms: vec![LifecycleGateResumeArm {
                    state: "basic_details_pending".to_string(),
                    substep: arm_substep.map(str::to_string),
                    target_view: "phone_verification".to_string(),
                    span: None,
                }],
                span: None,
            }],
            app_on_lifecycle_pending: None,
            app_span: None,
        }
    }

    #[test]
    fn lifecycle_substep_001_flags_missing_matching_resume_arm_or_sibling() {
        let input = input_with_substep(None);
        let features = vec![feature_with_host_lifecycle()];

        let diagnostics = check_input(&input, &features);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "LIFECYCLE-SUBSTEP-001"),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn lifecycle_substep_001_allows_matching_resume_arm() {
        let input = input_with_substep(Some("phone_verification"));
        let features = vec![feature_with_host_lifecycle()];

        let diagnostics = check_input(&input, &features);

        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.code != "LIFECYCLE-SUBSTEP-001"),
            "{diagnostics:#?}"
        );
    }
}
