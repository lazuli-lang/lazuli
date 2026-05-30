//! Completion-item builders + trigger detectors for lifecycle gates.
//!
//! Two responsibilities:
//!
//! 1. Recognize when the cursor is sitting at a position where a
//!    lifecycle completion should fire (`requires_lifecycle_state_trigger`,
//!    `resume_arm_*_trigger`, `lifecycle_view_slot_trigger`, etc.).
//! 2. Build the corresponding `CompletionItem` lists
//!    (`lifecycle_state_completion_items`,
//!    `lifecycle_resource_completion_items`,
//!    `lifecycle_resume_arm_completion_items`, etc.).
//!
//! `lifecycle_after_arrow` lives here because it is the shared parser
//! for both the trigger detectors and resume-arm parsing.
//! `lifecycle_scoped_label` produces a `feature.name` or bare `name`
//! label depending on whether the candidate sits in the same feature
//! scope as the caller — also used by resume.rs when building resume
//! completions.

use std::collections::HashSet;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, Documentation, Position};

use crate::{enclosing_view_block, leading_spaces, snippet_completion};

use super::gate::{lifecycle_feature_is_reachable, lifecycle_resource_for_name};
use super::lookup::collect_lifecycle_lookup_queries;
use super::parse::{lifecycle_ident, lifecycle_top_level_named_header};
use super::resume::{
    LifecycleResumeBlock, collect_lifecycle_resume_blocks, enclosing_lifecycle_resume_block,
    lifecycle_resource_for_resume,
};
use super::state::collect_lifecycle_resources;

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

/// Public entry point used by `lib.rs::server`. Returns lifecycle
/// completions when the cursor sits inside `requires_lifecycle ...`,
/// `on_lifecycle_pending @resume ...`, a `resume <name>` block body,
/// or a view slot that hasn't yet declared a lifecycle gate.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::lifecycle_gate_completions;
/// use tower_lsp::lsp_types::Position;
///
/// // Outside any lifecycle trigger — None.
/// let result = lifecycle_gate_completions(
///     "feature billing\n",
///     Position { line: 0, character: 0 },
/// );
/// assert!(result.is_none());
/// ```
pub fn lifecycle_gate_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let before = crate::line_prefix_at_position(line, position.character);
    let trimmed_before = before.trim_start();

    if let Some(resource) = requires_lifecycle_state_trigger(trimmed_before) {
        let feature = crate::route_guard_context_feature(source, position);
        return Some(lifecycle_state_completion_items(
            source,
            feature.as_deref(),
            resource,
        ));
    }

    if let Some(rest) = trimmed_before.strip_prefix("requires_lifecycle ") {
        if lifecycle_identifier_prefix(rest) {
            let feature = crate::route_guard_context_feature(source, position);
            return Some(lifecycle_resource_completion_items(
                source,
                feature.as_deref(),
            ));
        }
    }

    if let Some(rest) = trimmed_before.strip_prefix("on_lifecycle_pending @resume ") {
        if lifecycle_identifier_prefix(rest) {
            let feature = crate::route_guard_context_feature(source, position);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    #[test]
    fn no_completions_outside_lifecycle_context() {
        assert!(
            lifecycle_gate_completions(
                "feature billing\n",
                Position {
                    line: 0,
                    character: 0
                }
            )
            .is_none()
        );
    }

    #[test]
    fn no_completions_for_empty_source() {
        assert!(
            lifecycle_gate_completions(
                "",
                Position {
                    line: 0,
                    character: 0
                }
            )
            .is_none()
        );
    }
}
