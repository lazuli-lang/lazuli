//! Text-level parsers for lifecycle-gate authoring sugar.
//!
//! Resume routers and `requires_lifecycle` clauses can be authored
//! either as structured JSON (the canonical IR shape) or as compact
//! strings like `source query.lookup my_host`,
//! `requires_lifecycle Host = basic_details_pending`, and
//! `@resume.host_onboarding`. These helpers normalize the string
//! shapes into the typed records the analyzer reasons over.

use lazuli_ir::SpanRef;

use super::LifecycleGateResumeSource;

pub(super) fn parse_source(
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

pub(super) fn parse_requires(text: &str) -> Option<(String, String)> {
    let rest = text.trim().strip_prefix("requires_lifecycle")?.trim();
    let (resource, state) = rest.split_once('=')?;
    let state = state.split_whitespace().next().unwrap_or(state.trim());
    Some((resource.trim().to_owned(), state.to_owned()))
}

pub(super) fn parse_resume_ref(default_feature: &str, text: &str) -> Option<(String, String)> {
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

pub(super) fn parse_view_ref(default_feature: &str, text: &str) -> (String, String) {
    let raw = text.trim().strip_prefix("view ").unwrap_or(text.trim());
    let parts: Vec<_> = raw.split('.').collect();
    match parts.as_slice() {
        [feature, "view", name] => ((*feature).to_owned(), (*name).to_owned()),
        [name] => (default_feature.to_owned(), (*name).to_owned()),
        _ => (default_feature.to_owned(), raw.to_owned()),
    }
}
