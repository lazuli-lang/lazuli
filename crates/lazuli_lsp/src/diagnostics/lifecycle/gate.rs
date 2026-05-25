//! `requires_lifecycle` route-gate inference + reachability.
//!
//! When the LSP offers a code action that inserts a
//! `requires_lifecycle <Resource> = <state>` line into a view, it
//! needs to know:
//!
//! 1. Which resource(s) the view hosts (via `source query.lookup ...`
//!    or `submit <command>`), and
//! 2. What state the path implies — `/onboarding/<resource>/<state>`
//!    is the canonical convention.
//!
//! This module owns those resolvers. The reachability helpers live
//! here too: a feature can only reference resources / resumes /
//! queries declared by itself or by a feature it `uses`.

use std::collections::HashSet;

use crate::{RouteGuardViewBlock, find_block_end, first_quoted_value, leading_spaces};

use super::lookup::{resolve_lifecycle_command_resource, resolve_lifecycle_lookup_query};
use super::parse::{
    lifecycle_top_level_named_header, lifecycle_uses_in_block, slug_for_lifecycle_token,
};
use super::state::{LifecycleResourceInfo, collect_lifecycle_resources};

#[derive(Debug, Clone)]
pub(crate) struct LifecycleGateCandidate {
    pub(crate) resource: String,
    pub(crate) state: String,
}

pub(crate) fn lifecycle_gate_candidate_for_view(
    source: &str,
    view: &RouteGuardViewBlock,
) -> Option<LifecycleGateCandidate> {
    for resource in lifecycle_resources_hosted_by_view(source, view) {
        let state = lifecycle_state_from_view_path(source, view, &resource).or_else(|| {
            lifecycle_default_gate_state(source, view.feature_hint.as_deref(), &resource)
        })?;
        return Some(LifecycleGateCandidate { resource, state });
    }
    None
}

pub(crate) fn lifecycle_resources_hosted_by_view(
    source: &str,
    view: &RouteGuardViewBlock,
) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut resources = Vec::new();
    let mut seen = HashSet::new();
    for line in lines.iter().take(view.end_line).skip(view.header_line + 1) {
        if leading_spaces(line) != view.header_indent + 2 {
            continue;
        }
        let trimmed = line.trim_start();
        let hosted = if let Some(rest) = trimmed.strip_prefix("source ") {
            lifecycle_resource_for_source_ref(source, view.feature_hint.as_deref(), rest)
        } else if let Some(rest) = trimmed.strip_prefix("submit ") {
            lifecycle_resource_for_submit_ref(source, view.feature_hint.as_deref(), rest)
        } else {
            None
        };
        if let Some(resource) = hosted {
            if lifecycle_resource_for_name(source, view.feature_hint.as_deref(), &resource)
                .is_some()
                && seen.insert(resource.clone())
            {
                resources.push(resource);
            }
        }
    }
    resources
}

pub(crate) fn lifecycle_resource_for_source_ref(
    source: &str,
    feature_hint: Option<&str>,
    rest: &str,
) -> Option<String> {
    let mut tokens = rest.split_whitespace();
    let first = tokens.next()?;
    let query_ref = if first == "query.lookup" {
        tokens.next()?.to_owned()
    } else {
        first.split('(').next().unwrap_or(first).to_owned()
    };
    resolve_lifecycle_lookup_query(source, feature_hint, &query_ref)?.returns
}

pub(crate) fn lifecycle_resource_for_submit_ref(
    source: &str,
    feature_hint: Option<&str>,
    rest: &str,
) -> Option<String> {
    let command_ref = rest
        .split_whitespace()
        .next()?
        .split('(')
        .next()
        .unwrap_or("");
    resolve_lifecycle_command_resource(source, feature_hint, command_ref)
}

pub(crate) fn lifecycle_default_gate_state(
    source: &str,
    feature_hint: Option<&str>,
    resource_name: &str,
) -> Option<String> {
    let resource = lifecycle_resource_for_name(source, feature_hint, resource_name)?;
    resource
        .states
        .iter()
        .find(|state| state.as_str() == "complete")
        .cloned()
        .or_else(|| resource.states.first().cloned())
}

pub(crate) fn lifecycle_state_from_view_path(
    source: &str,
    view: &RouteGuardViewBlock,
    resource_name: &str,
) -> Option<String> {
    let path = lifecycle_view_path(source, view)?;
    let segments = path
        .trim_matches('"')
        .trim_start_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    if segments.len() < 3 || segments.first().copied() != Some("onboarding") {
        return None;
    }
    let resource_slug = slug_for_lifecycle_token(resource_name);
    if segments.get(1).copied() != Some(resource_slug.as_str()) {
        return None;
    }
    let state_slug = segments[2..].join("-");
    let resource =
        lifecycle_resource_for_name(source, view.feature_hint.as_deref(), resource_name)?;
    resource.states.into_iter().find(|state| {
        let slug = slug_for_lifecycle_token(state);
        slug == state_slug || slug.strip_suffix("-pending") == Some(state_slug.as_str())
    })
}

pub(crate) fn lifecycle_view_path(source: &str, view: &RouteGuardViewBlock) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    for line in lines.iter().take(view.end_line).skip(view.header_line + 1) {
        if leading_spaces(line) == view.header_indent + 2 {
            if let Some(rest) = line.trim_start().strip_prefix("path ") {
                let trimmed = rest.trim();
                return first_quoted_value(trimmed).or_else(|| {
                    trimmed
                        .split_whitespace()
                        .next()
                        .filter(|path| path.starts_with('/'))
                        .map(str::to_owned)
                });
            }
        }
    }
    None
}

pub(crate) fn lifecycle_gate_insertion_line(source: &str, view: &RouteGuardViewBlock) -> usize {
    let lines: Vec<&str> = source.lines().collect();
    let mut fallback = view.header_line + 1;
    for (idx, line) in lines
        .iter()
        .enumerate()
        .take(view.end_line)
        .skip(view.header_line + 1)
    {
        if leading_spaces(line) != view.header_indent + 2 {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("policy ") {
            return idx + 1;
        }
        if trimmed.starts_with("path ") {
            fallback = idx + 1;
        }
    }
    fallback
}

pub(crate) fn view_has_requires_lifecycle(source: &str, view: &RouteGuardViewBlock) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    lines
        .iter()
        .take(view.end_line)
        .skip(view.header_line + 1)
        .any(|line| {
            leading_spaces(line) == view.header_indent + 2
                && line.trim_start().starts_with("requires_lifecycle ")
        })
}

pub(crate) fn lifecycle_pending_resume_for_view(
    source: &str,
    view: &RouteGuardViewBlock,
) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    for line in lines.iter().take(view.end_line).skip(view.header_line + 1) {
        if leading_spaces(line) != view.header_indent + 2 {
            continue;
        }
        if let Some(rest) = line
            .trim_start()
            .strip_prefix("on_lifecycle_pending @resume ")
        {
            return Some(rest.split_whitespace().next()?.to_owned());
        }
    }
    None
}

pub(crate) fn lifecycle_resource_for_name(
    source: &str,
    feature_hint: Option<&str>,
    resource_name: &str,
) -> Option<LifecycleResourceInfo> {
    collect_lifecycle_resources(source)
        .into_iter()
        .find(|resource| {
            resource.name == resource_name
                && lifecycle_feature_is_reachable(source, feature_hint, resource.feature.as_deref())
        })
}

pub(crate) fn lifecycle_feature_is_reachable(
    source: &str,
    context_feature: Option<&str>,
    candidate_feature: Option<&str>,
) -> bool {
    let Some(candidate) = candidate_feature else {
        return true;
    };
    let Some(context) = context_feature else {
        return true;
    };
    lifecycle_reachable_features(source, context)
        .iter()
        .any(|feature| feature == candidate)
}

pub(crate) fn lifecycle_reachable_features(source: &str, context_feature: &str) -> Vec<String> {
    let mut features = vec![context_feature.to_owned()];
    let mut seen = HashSet::from([context_feature.to_owned()]);
    let lines: Vec<&str> = source.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        if leading_spaces(line) != 0 {
            continue;
        }
        let trimmed = line.trim_start();
        let Some((_, name)) = lifecycle_top_level_named_header(trimmed) else {
            continue;
        };
        if name != context_feature {
            continue;
        }
        let end = find_block_end(&lines, idx, 0);
        for used in lifecycle_uses_in_block(&lines, idx + 1, end) {
            if seen.insert(used.clone()) {
                features.push(used);
            }
        }
    }
    features
}
