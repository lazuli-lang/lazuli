//! Lifecycle lookup-query + command resource resolution.
//!
//! `resume` blocks bind a `source query.lookup <q>` that returns the
//! actor's resource row; route-gate inference also needs to know
//! which resource a `submit <cmd>` will create / update. This module
//! collects the lookup-query catalog and provides the two resolvers
//! consumed by the gate + resume modules.

use crate::{find_block_end, leading_spaces};

#[derive(Debug, Clone)]
pub(crate) struct LifecycleLookupQueryInfo {
    pub(crate) feature: String,
    pub(crate) name: String,
    pub(crate) returns: Option<String>,
}

pub(crate) fn collect_lifecycle_lookup_queries(source: &str) -> Vec<LifecycleLookupQueryInfo> {
    let lines: Vec<&str> = source.lines().collect();
    let mut queries = Vec::new();
    let mut current_feature: Option<String> = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            continue;
        }
        let Some(feature) = current_feature.as_deref() else {
            continue;
        };
        let Some(rest) = trimmed.strip_prefix("query.lookup ") else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let end = find_block_end(&lines, idx, leading_spaces(line));
        let returns = lines
            .iter()
            .take(end)
            .skip(idx + 1)
            .find_map(|child| {
                child
                    .trim_start()
                    .strip_prefix("returns ")
                    .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned())
            })
            .filter(|value| !value.is_empty());
        queries.push(LifecycleLookupQueryInfo {
            feature: feature.to_owned(),
            name: name.to_owned(),
            returns,
        });
    }
    queries
}

pub(crate) fn resolve_lifecycle_lookup_query(
    source: &str,
    feature_hint: Option<&str>,
    query_ref: &str,
) -> Option<LifecycleLookupQueryInfo> {
    let queries = collect_lifecycle_lookup_queries(source);
    let (feature, name) = if let Some((feature, rest)) = query_ref.split_once(".query.") {
        (Some(feature), rest)
    } else if let Some((feature, name)) = query_ref.split_once('.') {
        (Some(feature), name)
    } else {
        (feature_hint, query_ref)
    };
    queries
        .into_iter()
        .find(|query| query.name == name && feature.map(|f| f == query.feature).unwrap_or(true))
}

pub(crate) fn resolve_lifecycle_command_resource(
    source: &str,
    feature_hint: Option<&str>,
    command_ref: &str,
) -> Option<String> {
    let (feature, name) = if let Some((feature, rest)) = command_ref.split_once(".command.") {
        (Some(feature), rest)
    } else if let Some((feature, name)) = command_ref.split_once('.') {
        (Some(feature), name)
    } else {
        (feature_hint, command_ref)
    };
    let lines: Vec<&str> = source.lines().collect();
    let mut current_feature: Option<String> = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            continue;
        }
        let Some(current) = current_feature.as_deref() else {
            continue;
        };
        if feature.map(|f| f != current).unwrap_or(false) {
            continue;
        }
        if !trimmed
            .strip_prefix("command ")
            .map(|rest| rest.split_whitespace().next().unwrap_or("") == name)
            .unwrap_or(false)
        {
            continue;
        }
        let end = find_block_end(&lines, idx, leading_spaces(line));
        for child in lines.iter().take(end).skip(idx + 1) {
            let child_trimmed = child.trim_start();
            for prefix in ["creates ", "updates ", "target "] {
                if let Some(rest) = child_trimmed.strip_prefix(prefix) {
                    let resource = rest.split_whitespace().next().unwrap_or("").to_owned();
                    if !resource.is_empty() {
                        return Some(resource);
                    }
                }
            }
        }
    }
    None
}
