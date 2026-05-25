//! `check_resumes` pass: per-resume diagnostics. Verifies that
//! every `resume_router` covers the lifecycle states of the resource
//! its source query resolves (`LIFECYCLE-GATE-003`), and that the
//! source query is in fact a `query.lookup`
//! (`LIFECYCLE-GATE-008`). Also surfaces redundant wildcards
//! (`LIFECYCLE-GATE-005`) and arms for unknown states
//! (`LIFECYCLE-GATE-004`).

use std::collections::BTreeSet;

use super::{
    Index, LifecycleGateDiagnostic, LifecycleGateResume, LifecycleGateResumeSource,
    LifecycleGateSeverity, QueryHit, diag, lifecycle_states, query_kind,
};

pub(super) fn check_resumes(index: &Index<'_>, out: &mut Vec<LifecycleGateDiagnostic>) {
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

pub(super) fn valid_lookup_source<'a>(
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
