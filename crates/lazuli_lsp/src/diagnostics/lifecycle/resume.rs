//! `resume` block collectors and coverage helpers.
//!
//! A `resume <name>` block declares how to route a user whose
//! lifecycle state of a particular resource is mid-flow. Each arm
//! maps a state (or `none` / `*`) to a target view via `→` / `->`.
//!
//! This module:
//!
//! * Collects every `resume` block with its arms, header line,
//!   `source query.lookup` binding, and feature scope
//!   (`collect_lifecycle_resume_blocks`).
//! * Parses individual arms (`lifecycle_parse_resume_arm`).
//! * Reports missing / stale arms against the resource's lifecycle
//!   (`lifecycle_missing_resume_states`,
//!   `lifecycle_stale_resume_arm_on_line`).
//! * Maps resume blocks to / from their bound resource
//!   (`lifecycle_resource_for_resume`,
//!   `lifecycle_resume_for_resource`).
//! * Finds the enclosing resume block at a cursor position
//!   (`enclosing_lifecycle_resume_block`).

use std::collections::HashSet;

use tower_lsp::lsp_types::Position;

use crate::{find_block_end, leading_spaces};

use super::completion::{lifecycle_after_arrow, lifecycle_scoped_label};
use super::gate::{lifecycle_feature_is_reachable, lifecycle_resource_for_name};
use super::lookup::resolve_lifecycle_lookup_query;
use super::parse::{lifecycle_ident, lifecycle_top_level_named_header};
use super::state::LifecycleResourceInfo;

#[derive(Debug, Clone)]
pub(crate) struct LifecycleResumeBlock {
    pub(crate) name: String,
    pub(crate) feature_hint: Option<String>,
    pub(crate) header_line: usize,
    pub(crate) header_indent: usize,
    pub(crate) end_line: usize,
    pub(crate) source_query: Option<String>,
    pub(crate) arms: Vec<LifecycleResumeArm>,
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleResumeArm {
    pub(crate) state: String,
    pub(crate) line: usize,
}

pub(crate) fn collect_lifecycle_resume_blocks(source: &str) -> Vec<LifecycleResumeBlock> {
    let lines: Vec<&str> = source.lines().collect();
    let mut blocks = Vec::new();
    let mut current_top: Option<String> = None;
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            current_top =
                lifecycle_top_level_named_header(trimmed).map(|(_, name)| name.to_owned());
        }
        let Some(rest) = trimmed.strip_prefix("resume ") else {
            continue;
        };
        let name = rest.split_whitespace().next().unwrap_or("").to_owned();
        if name.is_empty() {
            continue;
        }
        let header_indent = leading_spaces(line);
        let end_line = find_block_end(&lines, idx, header_indent);
        let mut source_query = None;
        let mut arms = Vec::new();
        for (child_idx, child) in lines.iter().enumerate().take(end_line).skip(idx + 1) {
            if leading_spaces(child) != header_indent + 2 {
                continue;
            }
            let child_trimmed = child.trim_start();
            if let Some(rest) = child_trimmed.strip_prefix("source query.lookup ") {
                source_query = rest
                    .split_whitespace()
                    .next()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                continue;
            }
            if let Some(arm) = lifecycle_parse_resume_arm(child_trimmed, child_idx) {
                arms.push(arm);
            }
        }
        blocks.push(LifecycleResumeBlock {
            name,
            feature_hint: current_top.clone(),
            header_line: idx,
            header_indent,
            end_line,
            source_query,
            arms,
        });
    }
    blocks
}

pub(crate) fn lifecycle_parse_resume_arm(trimmed: &str, line: usize) -> Option<LifecycleResumeArm> {
    let state = trimmed.split_whitespace().next()?.to_owned();
    if !(state == "none" || state == "*" || lifecycle_ident(&state)) {
        return None;
    }
    let _after_arrow = lifecycle_after_arrow(trimmed)?;
    Some(LifecycleResumeArm { state, line })
}

pub(crate) fn enclosing_lifecycle_resume_block(
    source: &str,
    position: Position,
) -> Option<LifecycleResumeBlock> {
    let line_idx = position.line as usize;
    collect_lifecycle_resume_blocks(source)
        .into_iter()
        .find(|block| line_idx >= block.header_line && line_idx < block.end_line)
}

pub(crate) fn lifecycle_missing_resume_states(
    source: &str,
    resume: &LifecycleResumeBlock,
) -> Vec<String> {
    if resume.arms.iter().any(|arm| arm.state == "*") {
        return Vec::new();
    }
    let Some(resource) = lifecycle_resource_for_resume(source, resume) else {
        return Vec::new();
    };
    let consumed: HashSet<&str> = resume.arms.iter().map(|arm| arm.state.as_str()).collect();
    resource
        .states
        .into_iter()
        .filter(|state| !consumed.contains(state.as_str()))
        .collect()
}

pub(crate) fn lifecycle_stale_resume_arm_on_line(
    source: &str,
    resume: &LifecycleResumeBlock,
    line_idx: usize,
) -> Option<LifecycleResumeArm> {
    let resource = lifecycle_resource_for_resume(source, resume)?;
    let states: HashSet<&str> = resource.states.iter().map(String::as_str).collect();
    resume
        .arms
        .iter()
        .find(|arm| {
            arm.line == line_idx
                && arm.state != "none"
                && arm.state != "*"
                && !states.contains(arm.state.as_str())
        })
        .cloned()
}

pub(crate) fn lifecycle_resume_arm_insertion_line(resume: &LifecycleResumeBlock) -> usize {
    resume
        .arms
        .iter()
        .map(|arm| arm.line + 1)
        .max()
        .unwrap_or(resume.end_line)
}

pub(crate) fn lifecycle_resume_for_resource(
    source: &str,
    feature_hint: Option<&str>,
    resource_name: &str,
) -> Option<String> {
    collect_lifecycle_resume_blocks(source)
        .into_iter()
        .find(|resume| {
            lifecycle_feature_is_reachable(source, feature_hint, resume.feature_hint.as_deref())
                && lifecycle_resource_for_resume(source, resume)
                    .map(|resource| resource.name == resource_name)
                    .unwrap_or(false)
        })
        .map(|resume| {
            lifecycle_scoped_label(feature_hint, resume.feature_hint.as_deref(), &resume.name)
        })
}

pub(crate) fn lifecycle_resource_for_resume(
    source: &str,
    resume: &LifecycleResumeBlock,
) -> Option<LifecycleResourceInfo> {
    let source_query = resume.source_query.as_deref()?;
    let query =
        resolve_lifecycle_lookup_query(source, resume.feature_hint.as_deref(), source_query)?;
    let resource_name = query.returns?;
    lifecycle_resource_for_name(source, resume.feature_hint.as_deref(), &resource_name)
}
