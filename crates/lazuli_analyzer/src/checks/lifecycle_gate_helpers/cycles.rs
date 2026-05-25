//! `check_cycles` pass: detects redirect cycles between lifecycle
//! gates. When a view requires state `X` and pending users get
//! redirected to view `B`, view `B`'s own `requires_lifecycle` must
//! eventually advance the resource forward — never back to a state
//! whose resume points back at view A. Emits `LIFECYCLE-GATE-006`
//! with the full redirect path so authors can see the loop.

use std::collections::{BTreeMap, BTreeSet};

use super::parse::parse_view_ref;
use super::{
    Index, LifecycleGateDiagnostic, LifecycleGateInput, LifecycleGateResumeArm,
    LifecycleGateSeverity, LifecycleGateView, diag, valid_requires,
};

pub(super) fn check_cycles(
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
