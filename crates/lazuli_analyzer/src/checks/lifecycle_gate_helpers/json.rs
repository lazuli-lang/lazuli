//! JSON <-> `LifecycleGateInput` glue. The analyzer talks to the
//! `ExperienceModule` / `AppManifest` / `Feature` IR via `serde_json`
//! values so it can pivot through optional fields without rewriting
//! every consumer when the IR shape evolves.
//!
//! Public functions in this module:
//! - `input_from_json`: extract a typed `LifecycleGateInput` from the
//!   IR before the check runs.
//! - `cache_resolved_json`: after the check passes (no errors),
//!   stamp `resolved_lifecycle_gate` metadata back into the
//!   `ExperienceModule` JSON so consumers can read the resolution
//!   without re-running the analyzer.

use std::collections::BTreeMap;

use lazuli_ir::{AppManifest, ExperienceModule, Feature, SpanRef};
use serde_json::{Value, json};

use super::parse::{parse_requires, parse_source};
use super::{
    Index, LifecycleGateDiagnostic, LifecycleGateInput, LifecycleGateResume,
    LifecycleGateResumeArm, LifecycleGateResumeSource, LifecycleGateSeverity, LifecycleGateView,
    RequiresLifecycle,
};

pub(super) fn input_from_json(
    module: &ExperienceModule,
    app: Option<&AppManifest>,
    features: &[Feature],
) -> LifecycleGateInput {
    let mut input = LifecycleGateInput {
        app_on_lifecycle_pending: app.and_then(|app| serde_json::to_value(app).ok()).and_then(
            |value| {
                value
                    .pointer("/route_guard/on_lifecycle_pending")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            },
        ),
        ..Default::default()
    };
    for feature in features {
        if let Ok(value) = serde_json::to_value(feature)
            && let Some(resumes) = value.get("resume_routers").and_then(Value::as_array)
        {
            input.resumes.extend(
                resumes
                    .iter()
                    .filter_map(|r| resume_from_json(&feature.name, r)),
            );
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

pub(super) fn cache_resolved_json(
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
                            "substep": req.substep,
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
        substep: value
            .get("substep")
            .and_then(Value::as_str)
            .map(str::to_owned),
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
            substep: None,
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
        substep: value
            .get("substep")
            .and_then(Value::as_str)
            .map(str::to_owned),
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
