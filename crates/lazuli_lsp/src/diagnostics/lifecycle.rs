//! Diagnostics, completions, hovers, and code-action backends for the
//! IR Lifecycle Route-Gates family (`requires_lifecycle`, `resume`,
//! `pending`, `→` arms).
//!
//! This module hosts the language layer for IR Route-Gates: the
//! completion / hover helpers consumed by the LSP frontends in
//! `crate::handlers::*`, plus the discovery helpers
//! (`collect_lifecycle_*`, `enclosing_lifecycle_resume_block`, etc.)
//! consumed by `crate::code_actions::lifecycle_gate`.
//!
//! Two public entry points are surfaced to `lib.rs::server`:
//! [`lifecycle_gate_completions`] and [`lifecycle_gate_hover`]. The rest
//! are `pub(crate)` and reach call sites in code_actions via the
//! `pub(crate) use diagnostics::lifecycle::*;` re-export in lib.rs.
//!
//! Shared helpers from lib.rs (`line_prefix_at_position`,
//! `byte_index_for_utf16_position`, `snippet_completion`,
//! `first_quoted_value`, `route_guard_context_feature`,
//! `enclosing_view_block`, `find_block_end`, `position_at_line_start`)
//! are imported by name — they're general-purpose and used outside
//! the lifecycle cluster too.

use std::collections::HashSet;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Documentation, Position};

use crate::{
    RouteGuardViewBlock, byte_index_for_utf16_position, enclosing_view_block, find_block_end,
    first_quoted_value, leading_spaces, line_prefix_at_position, route_guard_context_feature,
    snippet_completion,
};

// ── IR Lifecycle Route-Gates — LSP completion / hover / code actions ───────

pub(crate) const LIFECYCLE_REQUIRES_HOVER: &str = "Gate this view on the actor's `<Resource>.lifecycle_state`. Codegen emits a TanStack `beforeLoad` that fetches the source query and redirects via `@resume` on mismatch.";
pub(crate) const LIFECYCLE_PENDING_HOVER: &str = "Name of the `resume <name>` block to redirect through when `requires_lifecycle` doesn't match.";
pub(crate) const LIFECYCLE_RESUME_HOVER: &str = "Block declaring how to route a user whose lifecycle state of a particular resource is mid-flow.";
pub(crate) const LIFECYCLE_SOURCE_QUERY_HOVER: &str = "The lookup query that fetches the actor's row of the resource. Must return a single record OR not-found (404).";
pub(crate) const LIFECYCLE_NONE_HOVER: &str =
    "Arm matched when the source query returns 404 (the actor's row doesn't exist yet).";
pub(crate) const LIFECYCLE_WILDCARD_HOVER: &str = "Catch-all arm. Matches any state not explicitly listed. Required when `resume` arms don't cover every state in the lifecycle, OR for forward-compatibility.";
pub(crate) const LIFECYCLE_ARROW_HOVER: &str = "Arrow token mapping a lifecycle state arm to a target view in a `resume` block. Both Unicode `→` and ASCII `->` accepted.";

#[derive(Debug, Clone)]
pub(crate) struct LifecycleResourceInfo {
    feature: Option<String>,
    name: String,
    states: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleLookupQueryInfo {
    feature: String,
    name: String,
    returns: Option<String>,
}

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

pub fn lifecycle_gate_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = line_prefix_at_position(line, position.character);
    let trimmed_before = before.trim_start();

    if let Some(resource) = requires_lifecycle_state_trigger(trimmed_before) {
        let feature = route_guard_context_feature(source, position);
        return Some(lifecycle_state_completion_items(
            source,
            feature.as_deref(),
            resource,
        ));
    }

    if let Some(rest) = trimmed_before.strip_prefix("requires_lifecycle ") {
        if lifecycle_identifier_prefix(rest) {
            let feature = route_guard_context_feature(source, position);
            return Some(lifecycle_resource_completion_items(
                source,
                feature.as_deref(),
            ));
        }
    }

    if let Some(rest) = trimmed_before.strip_prefix("on_lifecycle_pending @resume ") {
        if lifecycle_identifier_prefix(rest) {
            let feature = route_guard_context_feature(source, position);
            return Some(lifecycle_resume_completion_items(
                source,
                feature.as_deref(),
            ));
        }
    }

    if let Some(resume) = enclosing_lifecycle_resume_block(source, position) {
        if let Some(rest) = trimmed_before.strip_prefix("source query.lookup ") {
            if lifecycle_identifier_prefix(rest) {
                return Some(lifecycle_lookup_query_completion_items(
                    source,
                    resume.feature_hint.as_deref(),
                ));
            }
        }

        if resume_arm_view_target_trigger(trimmed_before) {
            return Some(lifecycle_view_completion_items(
                source,
                resume.feature_hint.as_deref(),
            ));
        }

        if resume_arm_view_keyword_trigger(trimmed_before, &resume) {
            return Some(vec![lifecycle_keyword_completion(
                "view ",
                "view ",
                "Map this lifecycle arm to a target view.",
            )]);
        }

        if resume_header_source_trigger(trimmed_before, &resume, position) {
            return Some(vec![snippet_completion(
                "source query.lookup <q>",
                "source query.lookup ${1:lookup_query}",
                "Choose the lookup query that fetches the actor's lifecycle row.",
            )]);
        }

        if resume_arm_start_trigger(source, before, &resume, position) {
            return Some(lifecycle_resume_arm_completion_items(source, &resume));
        }
    }

    if lifecycle_view_slot_trigger(source, position, before) {
        return Some(vec![
            lifecycle_keyword_completion(
                "requires_lifecycle ",
                "requires_lifecycle ",
                "Gate this view on a resource lifecycle state.",
            ),
            lifecycle_keyword_completion(
                "on_lifecycle_pending @resume ",
                "on_lifecycle_pending @resume ",
                "Redirect lifecycle mismatches through a resume router.",
            ),
        ]);
    }

    None
}

pub(crate) fn lifecycle_keyword_completion(
    label: &str,
    insert_text: &str,
    detail: &str,
) -> CompletionItem {
    CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::KEYWORD),
        detail: Some(detail.to_owned()),
        insert_text: Some(insert_text.to_owned()),
        documentation: Some(Documentation::String(detail.to_owned())),
        ..CompletionItem::default()
    }
}

pub(crate) fn lifecycle_reference_completion(label: String, detail: &str) -> CompletionItem {
    CompletionItem {
        label,
        kind: Some(CompletionItemKind::REFERENCE),
        detail: Some(detail.to_owned()),
        ..CompletionItem::default()
    }
}

pub(crate) fn lifecycle_identifier_prefix(rest: &str) -> bool {
    rest.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

pub(crate) fn requires_lifecycle_state_trigger(trimmed_before: &str) -> Option<&str> {
    let rest = trimmed_before.strip_prefix("requires_lifecycle ")?;
    let (resource, value_part) = rest.split_once('=')?;
    let resource = resource.trim();
    if resource.is_empty()
        || !resource
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    if value_part
        .trim_start()
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Some(resource)
    } else {
        None
    }
}

pub(crate) fn lifecycle_resource_completion_items(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<CompletionItem> {
    collect_lifecycle_resources(source)
        .into_iter()
        .filter(|resource| {
            lifecycle_feature_is_reachable(source, feature_hint, resource.feature.as_deref())
        })
        .map(|resource| CompletionItem {
            label: resource.name,
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("Resource with a declared lifecycle.".to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn lifecycle_state_completion_items(
    source: &str,
    feature_hint: Option<&str>,
    resource_name: &str,
) -> Vec<CompletionItem> {
    lifecycle_resource_for_name(source, feature_hint, resource_name)
        .map(|resource| {
            resource
                .states
                .into_iter()
                .map(|state| CompletionItem {
                    label: state,
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some(format!("Declared state of `{resource_name}.lifecycle`.")),
                    ..CompletionItem::default()
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn lifecycle_resume_completion_items(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<CompletionItem> {
    collect_lifecycle_resume_blocks(source)
        .into_iter()
        .filter(|resume| {
            lifecycle_feature_is_reachable(source, feature_hint, resume.feature_hint.as_deref())
        })
        .map(|resume| {
            let label =
                lifecycle_scoped_label(feature_hint, resume.feature_hint.as_deref(), &resume.name);
            lifecycle_reference_completion(label, "Declared `resume <name>` router.")
        })
        .collect()
}

pub(crate) fn lifecycle_lookup_query_completion_items(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<CompletionItem> {
    collect_lifecycle_lookup_queries(source)
        .into_iter()
        .filter(|query| lifecycle_feature_is_reachable(source, feature_hint, Some(&query.feature)))
        .map(|query| {
            let label = lifecycle_scoped_label(feature_hint, Some(&query.feature), &query.name);
            lifecycle_reference_completion(label, "Declared `query.lookup` source.")
        })
        .collect()
}

pub(crate) fn lifecycle_view_completion_items(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<CompletionItem> {
    collect_lifecycle_view_names(source, feature_hint)
        .into_iter()
        .map(|view| lifecycle_reference_completion(view, "Declared view in this experience."))
        .collect()
}

pub(crate) fn lifecycle_resume_arm_completion_items(
    source: &str,
    resume: &LifecycleResumeBlock,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    let consumed: HashSet<String> = resume.arms.iter().map(|arm| arm.state.clone()).collect();
    items.push(CompletionItem {
        label: "none".to_owned(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some("No-row lifecycle arm.".to_owned()),
        ..CompletionItem::default()
    });
    if let Some(resource) = lifecycle_resource_for_resume(source, resume) {
        for state in resource.states {
            if !consumed.contains(&state) {
                items.push(CompletionItem {
                    label: state,
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some("Unconsumed lifecycle state.".to_owned()),
                    ..CompletionItem::default()
                });
            }
        }
    }
    items.push(CompletionItem {
        label: "*".to_owned(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some("Wildcard lifecycle arm.".to_owned()),
        ..CompletionItem::default()
    });
    items
}

pub(crate) fn lifecycle_scoped_label(
    context_feature: Option<&str>,
    item_feature: Option<&str>,
    name: &str,
) -> String {
    match (context_feature, item_feature) {
        (Some(context), Some(feature)) if context != feature => format!("{feature}.{name}"),
        (None, Some(feature)) => format!("{feature}.{name}"),
        _ => name.to_owned(),
    }
}

pub(crate) fn resume_header_source_trigger(
    trimmed_before: &str,
    resume: &LifecycleResumeBlock,
    position: Position,
) -> bool {
    let cursor_line = position.line as usize;
    if cursor_line == resume.header_line && trimmed_before.starts_with("resume ") {
        return true;
    }
    resume.source_query.is_none()
        && cursor_line > resume.header_line
        && (trimmed_before.is_empty()
            || trimmed_before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'))
}

pub(crate) fn resume_arm_start_trigger(
    source: &str,
    before: &str,
    resume: &LifecycleResumeBlock,
    position: Position,
) -> bool {
    if position.line as usize <= resume.header_line {
        return false;
    }
    let trimmed_before = before.trim_start();
    let line_indent = source
        .lines()
        .nth(position.line as usize)
        .map(leading_spaces)
        .unwrap_or_else(|| leading_spaces(before));
    line_indent == resume.header_indent + 2
        && (trimmed_before.is_empty()
            || trimmed_before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '*'))
}

pub(crate) fn resume_arm_view_keyword_trigger(
    trimmed_before: &str,
    resume: &LifecycleResumeBlock,
) -> bool {
    let state = trimmed_before.split_whitespace().next().unwrap_or("");
    if !lifecycle_resume_arm_state_known(state, resume) {
        return false;
    }
    if trimmed_before.ends_with(' ') && trimmed_before.split_whitespace().count() == 1 {
        return true;
    }
    if let Some(after_arrow) = lifecycle_after_arrow(trimmed_before) {
        return after_arrow
            .trim_start()
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '_');
    }
    false
}

pub(crate) fn resume_arm_view_target_trigger(trimmed_before: &str) -> bool {
    let Some(after_arrow) = lifecycle_after_arrow(trimmed_before) else {
        return false;
    };
    let after_arrow = after_arrow.trim_start();
    let Some(rest) = after_arrow.strip_prefix("view ") else {
        return false;
    };
    rest.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

pub(crate) fn lifecycle_after_arrow(trimmed_before: &str) -> Option<&str> {
    if let Some(index) = trimmed_before.rfind("->") {
        return Some(&trimmed_before[index + 2..]);
    }
    if let Some(index) = trimmed_before.rfind('→') {
        return Some(&trimmed_before[index + '→'.len_utf8()..]);
    }
    None
}

pub(crate) fn lifecycle_resume_arm_state_known(state: &str, resume: &LifecycleResumeBlock) -> bool {
    !state.is_empty()
        && (state == "none"
            || state == "*"
            || resume.arms.iter().any(|arm| arm.state == state)
            || state.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
}

pub(crate) fn lifecycle_view_slot_trigger(source: &str, position: Position, before: &str) -> bool {
    if enclosing_lifecycle_resume_block(source, position).is_some() {
        return false;
    }
    let trimmed_before = before.trim_start();
    if !(trimmed_before.is_empty()
        || trimmed_before
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_'))
    {
        return false;
    }
    let Some(view) = enclosing_view_block(source, position) else {
        return false;
    };
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    if leading_spaces(line) != view.header_indent + 2 {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    lines
        .iter()
        .enumerate()
        .take(position.line as usize)
        .skip(view.header_line + 1)
        .any(|(_, line)| {
            leading_spaces(line) == view.header_indent + 2
                && matches!(
                    line.trim_start().split_whitespace().next(),
                    Some("policy" | "path" | "submit")
                )
        })
}

pub fn lifecycle_gate_hover(
    source: &str,
    position: Position,
    word: Option<&str>,
) -> Option<String> {
    let line = source.lines().nth(position.line as usize).unwrap_or("");
    if lifecycle_hover_is_arrow(line, position)
        && enclosing_lifecycle_resume_block(source, position).is_some()
    {
        return Some(format!("`→` / `->`\n\n{LIFECYCLE_ARROW_HOVER}"));
    }

    if lifecycle_hover_is_wildcard(line, position)
        && enclosing_lifecycle_resume_block(source, position).is_some()
    {
        return Some(format!("`*`\n\n{LIFECYCLE_WILDCARD_HOVER}"));
    }

    let word = word?;
    if word == "source" || word == "query.lookup" {
        if line.trim_start().starts_with("source query.lookup ")
            && enclosing_lifecycle_resume_block(source, position).is_some()
        {
            return Some(format!(
                "`source query.lookup`\n\n{LIFECYCLE_SOURCE_QUERY_HOVER}"
            ));
        }
    }

    match word {
        "requires_lifecycle" if enclosing_view_block(source, position).is_some() => Some(format!(
            "`requires_lifecycle`\n\n{LIFECYCLE_REQUIRES_HOVER}"
        )),
        "on_lifecycle_pending" if enclosing_view_block(source, position).is_some() => Some(
            format!("`on_lifecycle_pending`\n\n{LIFECYCLE_PENDING_HOVER}"),
        ),
        "resume" if enclosing_lifecycle_resume_block(source, position).is_some() => {
            Some(format!("`resume`\n\n{LIFECYCLE_RESUME_HOVER}"))
        }
        "none" if enclosing_lifecycle_resume_block(source, position).is_some() => {
            Some(format!("`none`\n\n{LIFECYCLE_NONE_HOVER}"))
        }
        _ => lifecycle_resolved_gate_hover(source, position, word),
    }
}

pub(crate) fn lifecycle_hover_is_wildcard(line: &str, position: Position) -> bool {
    let index = byte_index_for_utf16_position(line, position.character);
    let bytes = line.as_bytes();
    (index < bytes.len() && bytes[index] == b'*') || (index > 0 && bytes[index - 1] == b'*')
}

pub(crate) fn lifecycle_hover_is_arrow(line: &str, position: Position) -> bool {
    let index = byte_index_for_utf16_position(line, position.character);
    let before = &line[..index.min(line.len())];
    let after = &line[index.min(line.len())..];
    before.ends_with("->")
        || after.starts_with("->")
        || before.ends_with('→')
        || after.starts_with('→')
}

pub(crate) fn lifecycle_resolved_gate_hover(
    source: &str,
    position: Position,
    word: &str,
) -> Option<String> {
    let line = source.lines().nth(position.line as usize)?;
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("requires_lifecycle ")?;
    let (resource, state) = rest.split_once('=')?;
    let resource = resource.trim();
    let state = state.split_whitespace().next().unwrap_or("");
    if word != state || state.is_empty() {
        return None;
    }
    let view = enclosing_view_block(source, position)?;
    let resume_name =
        lifecycle_pending_resume_for_view(source, &view).unwrap_or_else(|| "<resume>".to_owned());
    let states = lifecycle_resource_for_name(source, view.feature_hint.as_deref(), resource)
        .map(|resource| resource.states.join(", "))
        .unwrap_or_else(|| "unresolved".to_owned());
    Some(format!(
        "currently the view requires `{resource}.lifecycle_state = {state}`. On mismatch, redirects via `resume {resume_name}`. Lifecycle states declared: `{states}`."
    ))
}

#[derive(Debug, Clone)]
pub(crate) struct LifecycleGateCandidate {
    pub(crate) resource: String,
    pub(crate) state: String,
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
        .unwrap_or_else(|| resume.end_line)
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

pub(crate) fn lifecycle_top_level_named_header(trimmed: &str) -> Option<(&str, &str)> {
    for keyword in ["feature", "experience", "surface"] {
        if let Some(rest) = trimmed.strip_prefix(&format!("{keyword} ")) {
            let name = rest.split_whitespace().next().unwrap_or("");
            if !name.is_empty() {
                return Some((keyword, name));
            }
        }
    }
    None
}

pub(crate) fn lifecycle_uses_in_block(lines: &[&str], start: usize, end: usize) -> Vec<String> {
    let mut uses = Vec::new();
    let mut seen = HashSet::new();
    for idx in start..end {
        let line = lines.get(idx).copied().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed == "uses" {
            let indent = leading_spaces(line);
            for child in lines.iter().take(end).skip(idx + 1) {
                if child.trim_start().is_empty() || child.trim_start().starts_with('#') {
                    continue;
                }
                if leading_spaces(child) <= indent {
                    break;
                }
                let name = child.trim_start().split_whitespace().next().unwrap_or("");
                if lifecycle_ident(name) && seen.insert(name.to_owned()) {
                    uses.push(name.to_owned());
                }
            }
        } else if let Some(rest) = trimmed.strip_prefix("uses ") {
            for name in lifecycle_parse_uses_rest(rest) {
                if seen.insert(name.clone()) {
                    uses.push(name);
                }
            }
        }
    }
    uses
}

pub(crate) fn lifecycle_parse_uses_rest(rest: &str) -> Vec<String> {
    let mut names = Vec::new();
    for token in rest.replace(',', " ").split_whitespace() {
        if token == "version" {
            break;
        }
        if matches!(token, "feature" | "experience") {
            continue;
        }
        if lifecycle_ident(token) {
            names.push(token.to_owned());
        }
    }
    names
}

pub(crate) fn lifecycle_ident(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && token
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
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

pub(crate) fn collect_lifecycle_view_names(
    source: &str,
    feature_hint: Option<&str>,
) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut current_top: Option<String> = None;
    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if leading_spaces(line) == 0 {
            current_top =
                lifecycle_top_level_named_header(trimmed).map(|(_, name)| name.to_owned());
        }
        let context_matches = feature_hint
            .map(|feature| current_top.as_deref() == Some(feature))
            .unwrap_or(true);
        if !context_matches {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("view ") {
            let name = rest.split_whitespace().next().unwrap_or("");
            if lifecycle_ident(name) && seen.insert(name.to_owned()) {
                names.push(name.to_owned());
            }
        }
    }
    names
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

pub(crate) fn slug_for_lifecycle_token(token: &str) -> String {
    let mut slug = String::new();
    for (idx, ch) in token.chars().enumerate() {
        if ch == '_' || ch == ' ' {
            slug.push('-');
        } else if ch.is_ascii_uppercase() {
            if idx > 0 {
                slug.push('-');
            }
            slug.push(ch.to_ascii_lowercase());
        } else {
            slug.push(ch.to_ascii_lowercase());
        }
    }
    slug
}

pub(crate) fn snake_case(token: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in token.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == ' ' {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    out
}
