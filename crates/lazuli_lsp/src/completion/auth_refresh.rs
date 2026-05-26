//! Completion provider for the IR Auth-Refresh family
//! (`auth { sessions { rotation { ... } } }`).
//!
//! Mirrors `diagnostics/auth.rs` (validation backend) and
//! `code_actions/auth_refresh.rs` (refactor backend). This module
//! owns the **completion** backend that surfaces:
//!
//! - duration-literal value completions for `access_ttl`,
//!   `refresh_ttl`, and `grace` (catalog literals from
//!   `crate::AUTH_REFRESH_*_DURATION_LITERALS`),
//! - theft-action enum completions for `theft_detection_action`
//!   (`crate::AUTH_REFRESH_THEFT_ACTION_VALUES` +
//!   `crate::auth_refresh_theft_action_detail`),
//! - the `rotation` block scaffold snippet, and
//! - the three-clause snippet bundle inserted on a blank rotation
//!   line (`refresh_ttl`, `grace`, `theft_detection_action`).
//!
//! Two block carrier structs ([`AuthSessionsBlock`],
//! [`AuthRotationBlock`]) plus their text/indent enclosing scanners
//! (`enclosing_auth_sessions_block`, `enclosing_auth_rotation_block`)
//! and the small parent/child probes (`is_sessions_line`,
//! `is_rotation_line`, `has_auth_parent`, `block_end_line`,
//! `auth_sessions_has_child`, `auth_rotation_has_children`) live
//! here too because they are consumed cross-module by
//! `code_actions::auth_refresh`.
//!
//! Shared lib.rs helpers consumed here:
//!
//! - [`crate::leading_spaces`] / [`crate::is_trivia_line`] for
//!   indent-aware block scanning.
//! - [`crate::AUTH_REFRESH_ACCESS_DURATION_LITERALS`] /
//!   [`crate::AUTH_REFRESH_REFRESH_DURATION_LITERALS`] /
//!   [`crate::AUTH_REFRESH_GRACE_DURATION_LITERALS`] /
//!   [`crate::AUTH_REFRESH_THEFT_ACTION_VALUES`] /
//!   [`crate::auth_refresh_theft_action_detail`] — catalogs the
//!   completion items wrap.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat, Position};

use crate::{
    AUTH_REFRESH_ACCESS_DURATION_LITERALS, AUTH_REFRESH_GRACE_DURATION_LITERALS,
    AUTH_REFRESH_REFRESH_DURATION_LITERALS, AUTH_REFRESH_THEFT_ACTION_VALUES,
    auth_refresh_theft_action_detail, is_trivia_line, leading_spaces,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuthSessionsBlock {
    pub(crate) line_idx: usize,
    pub(crate) indent: usize,
    pub(crate) end_line: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AuthRotationBlock {
    pub(crate) line_idx: usize,
    pub(crate) indent: usize,
}

/// Completion provider for Cell LSP-1 of auth refresh rotation. This stays
/// text/indent based to mirror the rest of this crate's lightweight LSP
/// helpers and avoid touching parser, IR, codegen, or runtime layers.
///
/// ## Examples
///
/// ```
/// use lazuli_lsp::auth_refresh_completions;
/// use tower_lsp::lsp_types::Position;
///
/// // Outside an auth block — no completions fire.
/// let result = auth_refresh_completions("feature billing\n", Position { line: 0, character: 6 });
/// assert!(result.is_none());
/// ```
pub fn auth_refresh_completions(source: &str, position: Position) -> Option<Vec<CompletionItem>> {
    let line = source.lines().nth(position.line as usize)?;
    let cursor = (position.character as usize).min(line.len());
    let before = &line[..cursor];
    let trimmed_before = before.trim_start();

    if enclosing_auth_sessions_block(source, position).is_some()
        && after_keyword_value_prefix(trimmed_before, "access_ttl")
    {
        return Some(duration_literal_completion_items(
            AUTH_REFRESH_ACCESS_DURATION_LITERALS,
        ));
    }

    if enclosing_auth_sessions_block(source, position).is_some() && trimmed_before == "rotation" {
        return Some(vec![rotation_block_snippet_completion(leading_spaces(
            line,
        ))]);
    }

    if enclosing_auth_rotation_block(source, position).is_some() {
        if after_keyword_value_prefix(trimmed_before, "refresh_ttl") {
            return Some(duration_literal_completion_items(
                AUTH_REFRESH_REFRESH_DURATION_LITERALS,
            ));
        }
        if after_keyword_value_prefix(trimmed_before, "grace") {
            return Some(duration_literal_completion_items(
                AUTH_REFRESH_GRACE_DURATION_LITERALS,
            ));
        }
        if after_keyword_value_prefix(trimmed_before, "theft_detection_action") {
            return Some(auth_refresh_theft_action_completion_items());
        }

        let is_blank_indented = trimmed_before.is_empty() && !before.is_empty();
        let is_partial_child = !trimmed_before.is_empty()
            && !trimmed_before.chars().any(char::is_whitespace)
            && trimmed_before
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_');
        if is_blank_indented || is_partial_child {
            return Some(auth_refresh_rotation_clause_completion_items());
        }
    }

    None
}

pub(crate) fn after_keyword_value_prefix(trimmed_before: &str, keyword: &str) -> bool {
    let Some(rest) = trimmed_before.strip_prefix(keyword) else {
        return false;
    };
    rest.starts_with(' ')
        && rest
            .trim_start()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '"' || c == ' ')
}

pub(crate) fn duration_literal_completion_items(values: &[&str]) -> Vec<CompletionItem> {
    values
        .iter()
        .map(|value| CompletionItem {
            label: (*value).to_owned(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some("Duration literal for auth session rotation.".to_owned()),
            insert_text: Some((*value).to_owned()),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn auth_refresh_theft_action_completion_items() -> Vec<CompletionItem> {
    AUTH_REFRESH_THEFT_ACTION_VALUES
        .iter()
        .map(|value| CompletionItem {
            label: (*value).to_owned(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: auth_refresh_theft_action_detail(value).map(str::to_owned),
            ..CompletionItem::default()
        })
        .collect()
}

pub(crate) fn rotation_block_snippet_completion(line_indent: usize) -> CompletionItem {
    let child_indent = " ".repeat(line_indent + 2);
    CompletionItem {
        label: "scaffold rotation block".to_owned(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(
            "Insert refresh_ttl, grace, and theft_detection_action defaults under rotation."
                .to_owned(),
        ),
        insert_text: Some(format!(
            "\n{child_indent}refresh_ttl \"30 days\" # framework default\n{child_indent}grace \"30 seconds\" # framework default\n{child_indent}theft_detection_action revoke_session_family # framework default"
        )),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    }
}

pub(crate) fn auth_refresh_rotation_clause_completion_items() -> Vec<CompletionItem> {
    [
        (
            "refresh_ttl \"30 days\"",
            "refresh_ttl \"30 days\"",
            "Long-lived refresh token TTL. Framework default: 30 days.",
        ),
        (
            "grace \"30 seconds\"",
            "grace \"30 seconds\"",
            "Two-tab refresh race window. Framework default: 30 seconds.",
        ),
        (
            "theft_detection_action revoke_session_family",
            "theft_detection_action revoke_session_family",
            "Default theft response: revoke this session family.",
        ),
    ]
    .into_iter()
    .map(|(label, insert_text, detail)| CompletionItem {
        label: label.to_owned(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(detail.to_owned()),
        insert_text: Some(insert_text.to_owned()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    })
    .collect()
}

pub(crate) fn enclosing_auth_sessions_block(
    source: &str,
    position: Position,
) -> Option<AuthSessionsBlock> {
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));

    for idx in 0..=cursor_line_idx {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if is_trivia_line(line) || !is_sessions_line(trimmed) {
            continue;
        }
        let indent = leading_spaces(line);
        if !has_auth_parent(&lines, idx, indent) {
            continue;
        }
        let end_line = block_end_line(&lines, idx, indent);
        if cursor_line_idx >= idx && cursor_line_idx < end_line {
            return Some(AuthSessionsBlock {
                line_idx: idx,
                indent,
                end_line,
            });
        }
    }

    None
}

pub(crate) fn enclosing_auth_rotation_block(
    source: &str,
    position: Position,
) -> Option<AuthRotationBlock> {
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let cursor_line_idx = (position.line as usize).min(lines.len().saturating_sub(1));

    for idx in 0..=cursor_line_idx {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if is_trivia_line(line) || !is_rotation_line(trimmed) {
            continue;
        }
        if enclosing_auth_sessions_block(
            source,
            Position {
                line: idx as u32,
                character: 0,
            },
        )
        .is_none()
        {
            continue;
        }
        let indent = leading_spaces(line);
        let end_line = block_end_line(&lines, idx, indent);
        if cursor_line_idx >= idx && cursor_line_idx < end_line {
            return Some(AuthRotationBlock {
                line_idx: idx,
                indent,
            });
        }
    }

    None
}

pub(crate) fn is_sessions_line(trimmed: &str) -> bool {
    trimmed.split_whitespace().next() == Some("sessions")
}

pub(crate) fn is_rotation_line(trimmed: &str) -> bool {
    trimmed.split_whitespace().next() == Some("rotation")
}

pub(crate) fn has_auth_parent(lines: &[&str], line_idx: usize, child_indent: usize) -> bool {
    for idx in (0..line_idx).rev() {
        let line = lines[idx];
        if is_trivia_line(line) {
            continue;
        }
        let indent = leading_spaces(line);
        if indent < child_indent {
            let trimmed = line.trim_start();
            return trimmed == "auth" || trimmed.starts_with("auth ");
        }
    }
    false
}

pub(crate) fn block_end_line(lines: &[&str], start_idx: usize, block_indent: usize) -> usize {
    for idx in (start_idx + 1)..lines.len() {
        let line = lines[idx];
        if is_trivia_line(line) {
            continue;
        }
        if leading_spaces(line) <= block_indent {
            return idx;
        }
    }
    lines.len()
}

pub(crate) fn auth_sessions_has_child(
    source: &str,
    block: AuthSessionsBlock,
    keyword: &str,
) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    for idx in (block.line_idx + 1)..block.end_line.min(lines.len()) {
        let line = lines[idx];
        if is_trivia_line(line) || leading_spaces(line) <= block.indent {
            continue;
        }
        if line.trim_start().split_whitespace().next() == Some(keyword) {
            return true;
        }
    }
    false
}

pub(crate) fn auth_rotation_has_children(source: &str, rotation: AuthRotationBlock) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    for idx in (rotation.line_idx + 1)..lines.len() {
        let line = lines[idx];
        if is_trivia_line(line) {
            continue;
        }
        let indent = leading_spaces(line);
        if indent <= rotation.indent {
            return false;
        }
        return true;
    }
    false
}
