//! Lifecycle resource + state collectors.
//!
//! These collectors walk feature blocks and extract every resource
//! that declares a `lifecycle` block, returning the resource name and
//! the ordered set of `state` children. They are the foundation of
//! the rest of the lifecycle cluster: completions resolve state names
//! against [`collect_lifecycle_resources`], hover prints them, and
//! the resume-arm coverage check compares them to authored arms.
//!
//! The cluster supports both the canonical `resource <name>` body and
//! the `lifecycle status of <name>` shorthand surfaced at the
//! resource-block top level.

use std::collections::HashSet;

use crate::{find_block_end, leading_spaces};

use super::parse::lifecycle_ident;

#[derive(Debug, Clone)]
pub(crate) struct LifecycleResourceInfo {
    pub(crate) feature: Option<String>,
    pub(crate) name: String,
    pub(crate) states: Vec<String>,
}

pub(crate) fn collect_lifecycle_resources(source: &str) -> Vec<LifecycleResourceInfo> {
    let lines: Vec<&str> = source.lines().collect();
    let mut resources = Vec::new();
    let mut current_feature: Option<String> = None;
    let mut feature_end = 0usize;
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if leading_spaces(line) == 0 {
            current_feature = trimmed
                .strip_prefix("feature ")
                .map(|rest| rest.split_whitespace().next().unwrap_or("").to_owned());
            feature_end = if current_feature.is_some() {
                find_block_end(&lines, idx, 0)
            } else {
                idx
            };
        }

        if current_feature.is_some() && idx < feature_end {
            if let Some(rest) = trimmed.strip_prefix("resource ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                let end = find_block_end(&lines, idx, leading_spaces(line));
                if let Some(states) = lifecycle_states_in_resource_block(&lines, idx + 1, end) {
                    resources.push(LifecycleResourceInfo {
                        feature: current_feature.clone(),
                        name,
                        states,
                    });
                }
            } else if let Some(rest) = trimmed.strip_prefix("lifecycle status of ") {
                let name = rest.split_whitespace().next().unwrap_or("").to_owned();
                let end = find_block_end(&lines, idx, leading_spaces(line));
                let states =
                    lifecycle_state_children(&lines, idx + 1, end, leading_spaces(line) + 2);
                if !name.is_empty() && !states.is_empty() {
                    resources.push(LifecycleResourceInfo {
                        feature: current_feature.clone(),
                        name,
                        states,
                    });
                }
            }
        }
        idx += 1;
    }
    resources
}

pub(crate) fn lifecycle_states_in_resource_block(
    lines: &[&str],
    start: usize,
    end: usize,
) -> Option<Vec<String>> {
    for (idx, line) in lines.iter().enumerate().take(end).skip(start) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("lifecycle ") {
            let states = lifecycle_state_children(lines, idx + 1, end, leading_spaces(line) + 2);
            if !states.is_empty() {
                return Some(states);
            }
        }
    }
    None
}

pub(crate) fn lifecycle_state_children(
    lines: &[&str],
    start: usize,
    end: usize,
    child_indent: usize,
) -> Vec<String> {
    let mut states = Vec::new();
    let mut seen = HashSet::new();
    for line in lines.iter().take(end).skip(start) {
        if leading_spaces(line) != child_indent {
            continue;
        }
        if let Some(rest) = line.trim_start().strip_prefix("state ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if lifecycle_ident(name) && seen.insert(name.to_owned()) {
                states.push(name.to_owned());
            }
        }
    }
    states
}
