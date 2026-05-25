//! `check_views` pass: per-view diagnostics anchored on each
//! `requires_lifecycle` clause. Verifies the gated resource/state
//! exist, that the view carries a policy gate, and that the
//! authored `on_lifecycle_pending` resume actually points at the
//! same resource the gate guards. Also enforces
//! `LIFECYCLE-SUBSTEP-001` — a substep gate must be matched by a
//! resume arm or a sibling view with the same substep.

use super::resumes::valid_lookup_source;
use super::{
    Index, LifecycleGateDiagnostic, LifecycleGateInput, LifecycleGateResume, LifecycleGateSeverity,
    LifecycleGateView, diag, lifecycle_states, list_or_none,
};

pub(super) fn check_views(
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
        if let Some(substep) = req.substep.as_deref()
            && !has_matching_substep(input, view, resume, substep)
        {
            out.push(diag(
                "LIFECYCLE-SUBSTEP-001",
                LifecycleGateSeverity::Error,
                req.span.or(view.span),
                format!(
                    "view `{}` declares `requires_lifecycle {} = {} substep {}`, but no matching resume arm or sibling view declares that substep.",
                    view.name, req.resource, req.state, substep
                ),
            ));
        }
    }
}

fn has_matching_substep(
    input: &LifecycleGateInput,
    view: &LifecycleGateView,
    resume: &LifecycleGateResume,
    substep: &str,
) -> bool {
    let Some(req) = view.requires.as_ref() else {
        return false;
    };
    resume
        .arms
        .iter()
        .any(|arm| arm.state == req.state && arm.substep.as_deref() == Some(substep))
        || input.views.iter().any(|other| {
            other.feature == view.feature
                && other.name != view.name
                && other
                    .requires
                    .as_ref()
                    .map(|other_req| {
                        other_req.resource == req.resource
                            && other_req.state == req.state
                            && other_req.substep.as_deref() == Some(substep)
                    })
                    .unwrap_or(false)
        })
}
